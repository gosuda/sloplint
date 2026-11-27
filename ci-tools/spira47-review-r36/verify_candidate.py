"""Verify the exact reviewed Spira repair and restore the original checkout."""
from __future__ import annotations

import hashlib
import io
import json
import os
import re
import subprocess
import zipfile
from dataclasses import asdict
from pathlib import Path
from typing import Final

import artifact_support as artifacts
import baseline_support as baseline

SOURCE: Final[Path] = baseline.SOURCE
PROOF: Final[Path] = baseline.PROOF
OUTPUT: Final[Path] = baseline.OUTPUT
BUILD: Final[Path] = baseline.BUILD


def read_object(name: str) -> dict[str, object]:
    return baseline.object_value(json.loads((PROOF / name).read_bytes()))


def object_rows(value: object) -> list[dict[str, object]]:
    if not isinstance(value, list):
        raise ValueError("expected a JSON array")
    return [baseline.object_value(row) for row in value]


def text_rows(value: object) -> list[str]:
    if not isinstance(value, list):
        raise ValueError("expected a JSON text array")
    return [baseline.text(row) for row in value]


def source_identity(entries: list[dict[str, str]], tree: str, label: str) -> dict[str, object]:
    raw = subprocess.run(["git", "ls-files", "-s", "-z"], cwd=SOURCE, check=True,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout
    actual: list[dict[str, str]] = []
    for item in raw.split(b"\0"):
        if not item:
            continue
        header, raw_path = item.split(b"\t", 1)
        mode, blob, stage = header.decode("ascii").split()
        if stage != "0":
            raise RuntimeError("candidate index has unresolved entries")
        actual.append({"path": raw_path.decode("utf-8"), "mode": mode, "sha": blob})
    sort_key = lambda row: row["path"]
    if sorted(actual, key=sort_key) != sorted(entries, key=sort_key):
        raise RuntimeError("complete candidate source index differs")
    rows: list[dict[str, str]] = []
    for item in entries:
        path = SOURCE / item["path"]
        data = path.read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data,
                            usedforsecurity=False).hexdigest()
        if blob != item["sha"] or bool(path.stat().st_mode & 0o111) != (item["mode"] == "100755"):
            raise RuntimeError(f"candidate content or mode differs: {item['path']}")
        rows.append({**item, "sha256": baseline.digest(data)})
    observed_tree = subprocess.run(["git", "write-tree"], cwd=SOURCE, check=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout.decode().strip()
    if observed_tree != tree:
        raise RuntimeError("candidate Git tree differs")
    index_hash = baseline.digest(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode())
    record: dict[str, object] = {"label": label, "tree": tree, "blobs": len(rows),
                                 "index_sha256": index_hash}
    (OUTPUT / f"candidate-index-{label}.json").write_text(json.dumps(rows, indent=2) + "\n",
                                                       encoding="utf-8")
    print("SPIRA_CANDIDATE_IDENTITY " + json.dumps(record), flush=True)
    return record


def authenticate_baseline(manifest: dict[str, object], token: str) -> dict[str, object]:
    specification = baseline.object_value(manifest["baseline_artifact"])
    archive = artifacts.download(specification, token)
    expected_manifest = read_object("baseline-manifest.json")
    expected_report = read_object("baseline-report.json")
    with zipfile.ZipFile(io.BytesIO(archive.data)) as bundle:
        report = baseline.object_value(json.loads(bundle.read("report.json")))
        if report != expected_report or report["manifest"] != expected_manifest:
            raise RuntimeError("frozen baseline report or source manifest differs")
        if not report["baseline_verified"] or not report["source_restored"]:
            raise RuntimeError("baseline was not completed and restored")
        commands = object_rows(report["commands"])
        if len(commands) != 29 or len({row["name"] for row in commands}) != 29:
            raise RuntimeError("baseline command set differs")
        for command in commands:
            name = baseline.text(command["name"])
            if command["raw_exit"] != command["expected"] or command["timed_out"]:
                raise RuntimeError("baseline contains an unexpected command outcome")
            if baseline.digest(bundle.read(name + ".log")) != command["log_sha256"]:
                raise RuntimeError(f"baseline command log differs: {name}")
        receipts = {baseline.text(row["name"]): row for row in commands}
        if (receipts["generator-compile"]["raw_exit"] != 0
                or receipts["generator-unchanged-red"]["raw_exit"] != 1
                or receipts["existing-generator-controls"]["raw_exit"] != 0
                or receipts["existing-migration-controls"]["raw_exit"] != 0):
            raise RuntimeError("baseline RED or adjacent control outcomes differ")
    cases: list[dict[str, object]] = json.loads((PROOF / "continued-strings.json").read_bytes())
    native = object_rows(expected_report["native"])
    if len(cases) != 14 or {row["id"] for row in native} != {row["name"] for row in cases}:
        raise RuntimeError("frozen native case identities differ")
    by_name = {baseline.text(case["name"]): case for case in cases}
    for record in native:
        case = by_name[baseline.text(record["id"])]
        if (record["sql_sha256"] != baseline.digest(baseline.text(case["sql"]).encode())
                or record["server_version_num"] != "180007"
                or record["observed"] != {"name": case["native_name"], "version": case["native_version"]}
                or record["matches"] is not True):
            raise RuntimeError("frozen native value or SQL identity differs")
    identity = expected_report["initial_index_sha256"]
    if identity != expected_report["restored_index_sha256"]:
        raise RuntimeError("baseline source restoration identity differs")
    receipt = {**archive.receipt, "source": expected_manifest["source"],
               "native_cases": 14, "generator_raw_exit": 1,
               "command_logs_verified": len(commands), "source_restored": True,
               "native_reexecuted_in_this_job": False}
    (OUTPUT / "authenticated-baseline.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                       encoding="utf-8")
    print("AUTHENTICATED_SPIRA_BASELINE " + json.dumps(receipt), flush=True)
    return receipt


def install_linter(manifest: dict[str, object], token: str) -> dict[str, object]:
    specification = baseline.object_value(manifest["linter_artifact"])
    archive = artifacts.download(specification, token)
    with zipfile.ZipFile(io.BytesIO(archive.data)) as bundle:
        info = bundle.getinfo("sloplint")
        if info.file_size <= 0 or info.file_size > 256 * 1024 * 1024:
            raise RuntimeError("native linter exceeds the fixed extraction bound")
        binary = bundle.read(info)
        expected = baseline.text(specification["binary_sha256"])
        if baseline.digest(binary) != expected:
            raise RuntimeError("native linter binary digest differs")
        recorded_hash = bundle.read("logs/CLI_SHA256").decode().split()[0]
        recorded_source = bundle.read("logs/SOURCE_COMMIT").decode().strip()
        version = bundle.read("logs/CLI_VERSION").decode().strip()
        if recorded_hash != expected or recorded_source != specification["producer"]:
            raise RuntimeError("native linter producer records differ")
    tool_dir = BUILD / "verified-tools"
    tool_dir.mkdir(parents=True, exist_ok=True)
    tool = tool_dir / "sloplint"
    tool.write_bytes(binary)
    tool.chmod(0o755)
    os.environ["PATH"] = str(tool_dir) + os.pathsep + os.environ["PATH"]
    receipt = {**archive.receipt, "binary_sha256": expected,
               "binary_bytes": len(binary), "version": version, "source": recorded_source}
    (OUTPUT / "authenticated-linter.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                     encoding="utf-8")
    print("AUTHENTICATED_SLOPLINT " + json.dumps(receipt), flush=True)
    return receipt


def require_named_passes(log: str, names: set[str]) -> None:
    passed = set(re.findall(r"^\s*--- PASS: ([^ ]+) ", log, flags=re.MULTILINE))
    if not names <= passed:
        raise RuntimeError(f"missing required literal PASS records: {names - passed}")


def watchdog_mutation() -> dict[str, object]:
    path = SOURCE / "migrate/migrate.lgo"
    original = path.read_bytes()
    before = ("func Apply(ctx context.Context, p *db.Pool, steps const []Step) error {\n"
              "\trelease, err := p.AcquireMigrationTurn(ctx)")
    after = before.replace("AcquireMigrationTurn(ctx)", "AcquireMigrationTurn(context.Background())")
    source = original.decode()
    if source.count(before) != 1:
        raise RuntimeError("watchdog mutation boundary differs")
    mutated = source.replace(before, after, 1).encode()
    receipt: dict[str, object] = {"original_sha256": baseline.digest(original),
                                  "mutant_sha256": baseline.digest(mutated), "restored": False}
    try:
        path.write_bytes(mutated)
        baseline.run("watchdog-mutant-compile", ["logos", "test", "-run", "^$", "-count=1", "./migrate"])
        log = baseline.run("watchdog-mutant-red", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestQueuedApplyHonorsContext$", "./migrate"], expected=1, timeout=45)
        if ("--- FAIL: TestQueuedApplyHonorsContext " not in log
                or "queued Apply ignored its context deadline; the watchdog freed the turn" not in log):
            raise RuntimeError("watchdog mutation failed for a different reason")
        receipt["raw_exit"] = 1
    finally:
        path.write_bytes(original)
        if path.read_bytes() != original:
            raise RuntimeError("watchdog mutation was not restored")
        receipt["restored"] = True
        print("SPIRA_WATCHDOG_MUTATION " + json.dumps(receipt), flush=True)
    return receipt


def main() -> None:
    manifest = read_object("candidate-manifest.json")
    files = baseline.object_value(manifest["proof_sha256"])
    for name, expected in files.items():
        if Path(name).name != name or baseline.digest((PROOF / name).read_bytes()) != baseline.text(expected):
            raise RuntimeError(f"proof file digest differs: {name}")
    original = baseline.object_value(manifest["source"])
    candidate = baseline.object_value(manifest["candidate"])
    head = baseline.text(original["head"])
    for suffix, expected in (("HEAD", head), ("HEAD^{tree}", original["tree"])):
        result = baseline.run("source-" + ("head" if suffix == "HEAD" else "tree"),
                              ["git", "rev-parse", suffix]).strip()
        if result != expected:
            raise RuntimeError("source checkout differs")
    report: dict[str, object] = {
        "schema": 1, "manifest": manifest, "source_restored": False,
        "source_gates_passed": False, "publication_eligible": False,
    }
    created: list[Path] = []
    try:
        report["original_index_sha256"] = baseline.index_identity(original, "before")
        token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
        try:
            report["baseline_evidence"] = authenticate_baseline(manifest, token)
            report["linter"] = install_linter(manifest, token)
        finally:
            token = ""
        for name, command, wanted in (
            ("logos-version", ["logos", "version"], r"^logos version go1\.27\.4 linux/amd64$"),
            ("node-version", ["node", "--version"], r"^v26\.13\.1$"),
            ("pnpm-version", ["pnpm", "--version"], r"^12\.21\.0$"),
            ("just-version", ["just", "--version"], r"^just 1\.59\.0$"),
        ):
            if not re.fullmatch(wanted, baseline.run(name, command).strip()):
                raise RuntimeError(f"wrong {name}")
        if baseline.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external Go cache remained active")
        baseline.run("sloplint-version", ["sloplint", "--version"])
        authored: list[dict[str, str]] = json.loads((PROOF / "authored-candidate.json").read_bytes())
        allowed = text_rows(candidate["changed_paths"])
        if len(authored) != 7 or [row["path"] for row in authored] != allowed:
            raise RuntimeError("authored source path set differs")
        for row in authored:
            content = row["content"].encode()
            if baseline.digest(content) != row["sha256"]:
                raise RuntimeError("authored source digest differs")
            relative = Path(row["path"])
            if relative.is_absolute() or ".." in relative.parts:
                raise RuntimeError("authored source path escapes the checkout")
            path = SOURCE / relative
            if not path.exists():
                created.append(path)
            path.write_bytes(content)
        baseline.run("stage-exact-candidate", ["git", "add", "--", *allowed])
        entries: list[dict[str, str]] = json.loads((PROOF / "candidate-index.json").read_bytes())
        if len(entries) != candidate["blobs"]:
            raise RuntimeError("candidate blob count differs")
        before_identity = source_identity(entries, baseline.text(candidate["tree"]), "before")
        report["candidate_before"] = before_identity
        baseline.run("candidate-compile", ["logos", "test", "-run", "^$", "-count=1",
                                          "./cmd/sqlc-gen-spira", "./migrate"], timeout=1200)
        log = baseline.run("continued-generator-green", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestPostgresContinuedEscapeStrings$", "./cmd/sqlc-gen-spira"],
                           timeout=1200)
        cases: list[dict[str, object]] = json.loads((PROOF / "continued-strings.json").read_bytes())
        require_named_passes(log, {"TestPostgresContinuedEscapeStrings/" + baseline.text(row["name"])
                                  for row in cases} | {"TestPostgresContinuedEscapeStrings"})
        migration_names = {"TestQueuedApplyHonorsContext", "TestReleaseFreesTheTurnOnlyOnce",
                           "TestMigrationTurnRejectsEndedContextOnFreePool", "TestApplyPartialPrefixLeavesSchema"}
        migration_pattern = "^(" + "|".join(sorted(migration_names)) + ")$"
        log = baseline.run("migration-controls-green", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", migration_pattern, "./migrate"], timeout=1200)
        require_named_passes(log, migration_names)
        report["watchdog_mutation"] = watchdog_mutation()
        report["after_mutation"] = source_identity(entries, baseline.text(candidate["tree"]), "after-mutation")
        baseline.run("just-gate", ["just", "gate"], timeout=3600)
        baseline.run("just-slop", ["just", "slop"], timeout=600)
        baseline.run("just-eject", ["just", "eject"], timeout=1800)
        baseline.run("whitespace", ["git", "diff", "--cached", "--check", "HEAD"])
        baseline.run("tracked-source-unchanged", ["git", "diff", "--exit-code"])
        untracked = baseline.run("untracked-source", ["git", "ls-files", "--others", "--exclude-standard"])
        if untracked.strip():
            raise RuntimeError("unexpected untracked source remains")
        after_identity = source_identity(entries, baseline.text(candidate["tree"]), "after")
        report["candidate_after"] = after_identity
        if after_identity["index_sha256"] != before_identity["index_sha256"]:
            raise RuntimeError("source identity changed during the full gates")
        report["source_gates_passed"] = True
    finally:
        try:
            baseline.run("restore-original-source", ["git", "restore", "--source", head,
                                                     "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored_index_sha256"] = baseline.index_identity(original, "restored")
            baseline.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            report["source_restored"] = True
        finally:
            report["publication_eligible"] = bool(report["source_gates_passed"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in baseline.RECEIPTS]
            (OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_CANDIDATE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
