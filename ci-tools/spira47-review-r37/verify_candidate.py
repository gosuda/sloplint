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
import pipeline_support as pipeline

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
    frozen = read_object("baseline-failure.json")
    expected_report = baseline.object_value(frozen["report"])
    with zipfile.ZipFile(io.BytesIO(archive.data)) as bundle:
        report = baseline.object_value(json.loads(bundle.read("report.json")))
        if report != expected_report:
            raise RuntimeError("Frozen failed-baseline report differs")
        if (report["baseline_verified"] is not False or report["publication_eligible"] is not False
                or report["source_restored"] is not True):
            raise RuntimeError("The failed all-input gate was incorrectly relabeled")
        commands = object_rows(report["commands"])
        if len(commands) != 67 or len({row["name"] for row in commands}) != 67:
            raise RuntimeError("Baseline command set differs")
        for command in commands:
            name = baseline.text(command["name"])
            if command["raw_exit"] != command["expected"] or command["timed_out"]:
                raise RuntimeError("Baseline contains an unexpected subprocess outcome")
            if baseline.digest(bundle.read(name + ".log")) != command["log_sha256"]:
                raise RuntimeError(f"Baseline command bytes differ: {name}")
        receipts = {baseline.text(row["name"]): row for row in commands}
        if (receipts["generator-compile"]["raw_exit"] != 0
                or receipts["typed-literal-unchanged-red"]["raw_exit"] != 1
                or receipts["retained-generator-controls"]["raw_exit"] != 0):
            raise RuntimeError("Compiled-first RED/control evidence differs")
        observations = object_rows(json.loads(bundle.read("pipeline-observations.json")))
        if observations != object_rows(frozen["pipeline"]):
            raise RuntimeError("Complete frozen pipeline observations differ")
        native = object_rows(json.loads(bundle.read("native-observations.json")))
        original = [row for row in object_rows(frozen["native"]) if row["lane"] == "original-native"]
        if native != original or len(native) != 26 or any(row["matches"] is not True for row in native):
            raise RuntimeError("Frozen native PostgreSQL observations differ")
        cases = pipeline.cases_from("native-cases.json", 26)
        expectations = pipeline.cases_from("sqlc-literal-pipeline.json", 26)
        reachable = 0
        upstream: set[str] = set()
        for case, expectation, observation, native_row in zip(cases, expectations, observations, native, strict=True):
            name = baseline.text(case["name"])
            sql = baseline.text(case["sql"])
            if (expectation["name"] != name or observation["id"] != name or native_row["id"] != name
                    or observation["input_sql"] != sql or native_row["sql"] != sql
                    or observation["input_sha256"] != baseline.digest(sql.encode())
                    or native_row["observed"] != {"name": case["native_name"], "version": case["native_version"]}):
                raise RuntimeError("Frozen input identity or independent value differs")
            if baseline.digest(bundle.read(f"sqlc-{name}.log")) != observation["log_sha256"]:
                raise RuntimeError("Actual sqlc command log differs")
            if observation["captured_request"]:
                reachable += 1
                request = bundle.read(f"sqlc-{name}-request.json")
                if baseline.digest(request) != observation["request_sha256"]:
                    raise RuntimeError("Captured request bytes differ")
                captured = baseline.object_value(json.loads(request))
                query = object_rows(captured["queries"])
                if len(query) != 1 or query[0]["text"] != expectation["text"]:
                    raise RuntimeError("Permanent processed-text oracle differs from the actual request")
                if expectation["sqlc_error"] != "" or observation["processed_sql"] != expectation["text"]:
                    raise RuntimeError("Reachable pipeline oracle differs")
                processed = baseline.object_value(observation["processed_native"])
                if (processed["matches"] is not True
                        or processed["observed"] != native_row["observed"]
                        or processed["sql"] != query[0]["text"]):
                    raise RuntimeError("Captured native semantics differ")
            else:
                upstream.add(name)
                if (observation["raw_exit"] != 1 or observation["generated_files"]
                        or baseline.text(observation["log"]).strip() != expectation["sqlc_error"]):
                    raise RuntimeError("Frozen pre-plugin rejection differs")
        if reachable != 22 or upstream != {"escape-comment-lf", "escape-comment-crlf",
                                           "comment-only-version", "typed-qualified-comment"}:
            raise RuntimeError("Frozen reachable/upstream partition differs")
        baseline_manifest = baseline.object_value(report["manifest"])
        if baseline_manifest["source"] != manifest["source"]:
            raise RuntimeError("Baseline source differs from the current unchanged source")
        frozen_hashes = baseline.object_value(baseline_manifest["proof_sha256"])
        for name in ("typed_literal_test.lgo", "typed-literals.json", "native-cases.json", "schema.sql"):
            if baseline.digest((PROOF / name).read_bytes()) != frozen_hashes[name]:
                raise RuntimeError("A frozen RED input changed")
    if report["initial_index_sha256"] != report["restored_index_sha256"]:
        raise RuntimeError("Baseline restoration identity differs")
    receipt = {**archive.receipt, "original_gate_status": "failure",
               "original_failure": "Four native-valid inputs were rejected by pinned sqlc before the plugin.",
               "reused_scope": "Authenticated native values and compiled-first direct/pipeline RED observations only.",
               "original_native_matches": 26, "captured_native_matches": 22,
               "direct_generator_failures": report["generator_failed"],
               "pipeline_false_refusals": 7, "upstream_rejections": sorted(upstream),
               "command_logs_verified": len(commands), "source_restored": True,
               "baseline_verified": False}
    (OUTPUT / "authenticated-failed-baseline.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                              encoding="utf-8")
    print("AUTHENTICATED_SPIRA_FAILED_BASELINE " + json.dumps(receipt), flush=True)
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


def authored_rows() -> list[dict[str, str]]:
    rows: list[dict[str, str]] = json.loads((PROOF / "authored-candidate.json").read_bytes())
    for row in rows:
        relative = Path(row["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise RuntimeError("Authored source path escapes checkout")
        content = row["content"].encode()
        if baseline.digest(content) != row["sha256"]:
            raise RuntimeError("Authored source digest differs")
        blob = hashlib.sha1(b"blob " + str(len(content)).encode() + b"\0" + content,
                            usedforsecurity=False).hexdigest()
        if blob != row["blob"]:
            raise RuntimeError("Authored source Git blob differs")
    return rows


def permanent_pipeline_red(original: dict[str, object]) -> dict[str, object]:
    paths = {
        "cmd/sqlc-gen-spira/typed_literal_test.lgo",
        "cmd/sqlc-gen-spira/testdata/typed-literals.json",
        "cmd/sqlc-gen-spira/sqlc_postgres_literal_test.lgo",
        "cmd/sqlc-gen-spira/testdata/sqlc-literal-pipeline.json",
        "cmd/sqlc-gen-spira/testdata/literal-schema.sql",
    }
    created: list[Path] = []
    receipt: dict[str, object] = {"source_restored": False, "raw_exit": None}
    try:
        for row in authored_rows():
            if row["path"] not in paths:
                continue
            path = SOURCE / row["path"]
            if path.exists():
                raise RuntimeError("Permanent baseline fixture already exists")
            created.append(path)
            path.write_bytes(row["content"].encode())
        if len(created) != 5:
            raise RuntimeError("Permanent baseline fixture set differs")
        baseline.run("baseline-sqlc-modules", ["go", "-C", str(SOURCE / "tools"), "mod", "download"],
                     timeout=900)
        baseline.run("permanent-pipeline-compile", ["logos", "test", "-race", "-run", "^$", "-count=1",
                                                   "./cmd/sqlc-gen-spira"], timeout=1200)
        log = baseline.run("permanent-pipeline-unchanged-red", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestSQLcPostgresLiteralPipeline$", "./cmd/sqlc-gen-spira"],
                           expected=1, timeout=900)
        prefix = r"^\s*--- {}: TestSQLcPostgresLiteralPipeline/([a-z0-9-]+) "
        failed = set(re.findall(prefix.format("FAIL"), log, re.MULTILINE))
        passed = set(re.findall(prefix.format("PASS"), log, re.MULTILINE))
        expected = {"typed-name-lf", "typed-name-crlf", "typed-name-single", "typed-digit-tail",
                    "typed-underscore-tail", "typed-dollar-tail", "typed-utf8-tail"}
        names = {baseline.text(row["name"]) for row in pipeline.cases_from("native-cases.json", 26)}
        if failed != expected or passed != names - expected:
            raise RuntimeError("Permanent real-pipeline RED/control split differs")
        receipt.update({"raw_exit": 1, "failed": sorted(failed), "passed": sorted(passed)})
    finally:
        for path in created:
            path.unlink(missing_ok=True)
        receipt["restored_index_sha256"] = baseline.index_identity(original, "after-permanent-red")
        baseline.run("permanent-baseline-source-restoration", ["git", "diff", "--exit-code", "HEAD"])
        receipt["source_restored"] = True
        print("PERMANENT_SQLC_PIPELINE_BASELINE " + json.dumps(receipt), flush=True)
    return receipt


def main() -> None:
    manifest = read_object("candidate-manifest.json")
    for name, expected in baseline.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or baseline.digest((PROOF / name).read_bytes()) != baseline.text(expected):
            raise RuntimeError(f"Frozen proof digest differs: {name}")
    original = baseline.object_value(manifest["source"])
    candidate = baseline.object_value(manifest["candidate"])
    head = baseline.text(original["head"])
    for suffix, expected in (("HEAD", head), ("HEAD^{tree}", original["tree"])):
        actual = baseline.run("source-" + ("head" if suffix == "HEAD" else "tree"),
                              ["git", "rev-parse", suffix]).strip()
        if actual != expected:
            raise RuntimeError("Wrong immutable source")
    report: dict[str, object] = {"schema": 1, "manifest": manifest, "source_restored": False,
                                "source_gates_passed": False, "publication_eligible": False}
    created: list[Path] = []
    try:
        report["original_index_sha256"] = baseline.index_identity(original, "before")
        token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
        try:
            report["baseline_evidence"] = authenticate_baseline(manifest, token)
            report["linter"] = install_linter(manifest, token)
        finally:
            token = ""
        os.environ["GOWORK"] = "off"
        for name, command, wanted in (
            ("logos-version", ["logos", "version"], r"^logos version go1\.27\.4 linux/amd64$"),
            ("node-version", ["node", "--version"], r"^v26\.13\.1$"),
            ("pnpm-version", ["pnpm", "--version"], r"^12\.21\.0$"),
            ("just-version", ["just", "--version"], r"^just 1\.59\.0$"),
        ):
            if not re.fullmatch(wanted, baseline.run(name, command).strip()):
                raise RuntimeError(f"Wrong {name}")
        if baseline.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("External Go cache remained active")
        baseline.run("sloplint-version", ["sloplint", "--version"])
        report["permanent_pipeline_baseline"] = permanent_pipeline_red(original)
        authored = authored_rows()
        allowed = text_rows(candidate["changed_paths"])
        if len(authored) != 7 or [row["path"] for row in authored] != allowed:
            raise RuntimeError("Authored source path set differs")
        for row in authored:
            path = SOURCE / row["path"]
            if not path.exists():
                created.append(path)
            path.write_bytes(row["content"].encode())
        baseline.run("stage-exact-candidate", ["git", "add", "--", *allowed])
        entries: list[dict[str, str]] = json.loads((PROOF / "candidate-index.json").read_bytes())
        if len(entries) != candidate["blobs"]:
            raise RuntimeError("Candidate blob count differs")
        before = source_identity(entries, baseline.text(candidate["tree"]), "before")
        report["candidate_before"] = before
        baseline.run("candidate-compile", ["logos", "test", "-race", "-run", "^$", "-count=1",
                                          "./cmd/sqlc-gen-spira"], timeout=1200)
        pattern = "^(TestPostgresTypedLiteralBoundaries|TestPostgresContinuedEscapeStrings)$"
        log = baseline.run("literal-generator-green", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", pattern, "./cmd/sqlc-gen-spira"], timeout=1200)
        typed = pipeline.cases_from("typed-literals.json", 12)
        retained = pipeline.cases_from("continued-strings.json", 14)
        names = {"TestPostgresTypedLiteralBoundaries/" + baseline.text(row["name"]) for row in typed}
        names |= {"TestPostgresContinuedEscapeStrings/" + baseline.text(row["name"]) for row in retained}
        require_named_passes(log, names | {"TestPostgresTypedLiteralBoundaries", "TestPostgresContinuedEscapeStrings"})
        log = baseline.run("permanent-pipeline-green", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestSQLcPostgresLiteralPipeline$", "./cmd/sqlc-gen-spira"], timeout=1200)
        cases = pipeline.cases_from("native-cases.json", 26)
        require_named_passes(log, {"TestSQLcPostgresLiteralPipeline/" + baseline.text(row["name"])
                                   for row in cases} | {"TestSQLcPostgresLiteralPipeline"})
        report["native_and_pipeline"] = pipeline.native_and_pipeline(manifest)
        report["after_focused"] = source_identity(entries, baseline.text(candidate["tree"]), "after-focused")
        baseline.run("just-gate", ["just", "gate"], timeout=3600)
        baseline.run("just-slop", ["just", "slop"], timeout=600)
        baseline.run("just-eject", ["just", "eject"], timeout=1800)
        baseline.run("whitespace", ["git", "diff", "--cached", "--check", "HEAD"])
        baseline.run("tracked-source-unchanged", ["git", "diff", "--exit-code"])
        if baseline.run("untracked-source", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
            raise RuntimeError("Unexpected untracked source remains")
        after = source_identity(entries, baseline.text(candidate["tree"]), "after")
        report["candidate_after"] = after
        if after["index_sha256"] != before["index_sha256"]:
            raise RuntimeError("Source changed during required gates")
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
