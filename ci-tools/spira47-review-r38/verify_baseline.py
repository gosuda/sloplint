"""Prove dollar-token and compiled-query failures without changing production."""
from __future__ import annotations

import hashlib
import json
import re
import subprocess
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import native_support as native
import pipeline_observer as pipeline


def read_object(name: str) -> dict[str, object]:
    return base.object_value(json.loads((base.PROOF / name).read_bytes()))


def read_rows(name: str, count: int) -> list[dict[str, object]]:
    value: object = json.loads((base.PROOF / name).read_bytes())
    if not isinstance(value, list) or len(value) != count:
        raise ValueError("wrong frozen row count: " + name)
    return [base.object_value(row) for row in value]


def test_outcomes(log: str, test: str, names: set[str]) -> dict[str, list[str]]:
    started = set(re.findall(r"^=== RUN   " + re.escape(test) + r"/([a-z0-9-]+)$", log, re.MULTILINE))
    passed = set(re.findall(r"^\s*--- PASS: " + re.escape(test) + r"/([a-z0-9-]+) ", log, re.MULTILINE))
    failed = set(re.findall(r"^\s*--- FAIL: " + re.escape(test) + r"/([a-z0-9-]+) ", log, re.MULTILINE))
    if started != names or passed & failed or passed | failed != names:
        raise RuntimeError("missing or duplicate frozen test outcomes: " + test)
    return {"passed": sorted(passed), "failed": sorted(failed)}


def compiled_values(log: str, expected: dict[str, str], failures: set[str]) -> dict[str, object]:
    matches = re.findall(r"COMPILED_SQLC_QUERY_VALUES (\{[^\n]*\})", log)
    if len(matches) != 1:
        raise RuntimeError("missing unique actual compiled-query record")
    observed = base.object_value(json.loads(matches[0]))
    if set(observed) != set(expected) or any(not isinstance(value, str) for value in observed.values()):
        raise RuntimeError("compiled-query identity set differs")
    mismatches = {name for name, value in expected.items() if observed[name] != value}
    if mismatches != failures:
        raise RuntimeError("compiled-query mismatch partition differs: " + repr(mismatches))
    return {"observed": observed, "expected": expected, "mismatches": sorted(mismatches)}


def source_snapshot(manifest: dict[str, object], key: str) -> dict[str, object]:
    spec = base.object_value(manifest[key])
    entries: list[dict[str, str]] = json.loads((base.PROOF / base.text(spec["index_file"])).read_bytes())
    tracked = subprocess.run(["git", "ls-files", "-s", "-z"], cwd=base.SOURCE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout
    index: list[dict[str, str]] = []
    for entry in tracked.split(b"\0"):
        if not entry:
            continue
        header, path = entry.split(b"\t", 1)
        mode, blob, stage = header.decode().split()
        if stage != "0":
            raise RuntimeError("unmerged source index")
        index.append({"path": path.decode(), "mode": mode, "sha": blob})
    if sorted(index, key=lambda row: row["path"]) != sorted(entries, key=lambda row: row["path"]):
        raise RuntimeError("full native index differs: " + key)
    actual: list[dict[str, str]] = []
    for row in entries:
        path = base.SOURCE / row["path"]
        data = path.read_bytes()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if blob != row["sha"] or bool(path.stat().st_mode & 0o111) != (row["mode"] == "100755"):
            raise RuntimeError("source bytes or mode differ: " + row["path"])
        actual.append({**row, "sha256": base.digest(data)})
    tree = subprocess.run(["git", "write-tree"], cwd=base.SOURCE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, check=True).stdout.decode().strip()
    if tree != spec["tree"] or len(actual) != spec["blobs"]:
        raise RuntimeError("native source tree/count differs")
    record: dict[str, object] = {"label": key, "tree": tree, "blobs": len(actual),
                                "index_sha256": base.digest(json.dumps(actual, sort_keys=True,
                                                             separators=(",", ":")).encode())}
    print("SPIRA_R38_SOURCE_IDENTITY " + json.dumps(record), flush=True)
    return record


def original_native(container: str, cases: list[dict[str, object]]) -> list[dict[str, object]]:
    rows = [native.observe_native(container, case, base.text(case["sql"]), "new-original") for case in cases]
    if any(row["matches"] is not True for row in rows):
        raise RuntimeError("new frozen native expectation differs")
    return rows


def compiled_native(container: str, cases: list[dict[str, object]], values: dict[str, object],
                    lane: str, mismatches: set[str]) -> list[dict[str, object]]:
    rows = [native.observe_native(container, case, base.text(values[base.text(case["name"])]), lane)
            for case in cases]
    failures = {base.text(row["id"]) for row in rows if row["matches"] is not True}
    if failures != mismatches:
        raise RuntimeError("native compiled-query mismatch partition differs: " + repr(failures))
    return rows


def main() -> None:
    manifest = read_object("manifest.json")
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof payload digest differs: " + name)
    source = base.object_value(manifest["source"])
    if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
        raise RuntimeError("wrong maintained source")
    report: dict[str, object] = {"schema": 1, "manifest": manifest, "baseline_gates_passed": False,
                                "source_restored": False, "baseline_verified": False,
                                "publication_eligible": False}
    created: list[Path] = []
    try:
        report["before"] = source_snapshot(manifest, "source")
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = json.loads(base.run("sqlc-module", ["go", "-C", str(base.SOURCE / "tools"), "list",
                                                   "-m", "-json", "github.com/sqlc-dev/sqlc"]))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned sqlc differs")
        dollars = read_rows("dollar-strings.json", 12)
        constants = read_rows("query-constant-values.json", 8)
        old_cases = read_rows("retained-native-cases.json", 26)
        old_expected = read_rows("retained-pipeline.json", 26)
        expected_pipeline = {base.text(row["name"]): base.text(want["text"])
                             for row, want in zip(old_cases, old_expected, strict=True)
                             if row["allow"] is True and not want["sqlc_error"]}
        if len(expected_pipeline) != 20:
            raise RuntimeError("retained generated query identity differs")
        authored = read_rows("authored-tests.json", 6)
        with native.postgres(manifest) as (container, version):
            report["native_version"] = version
            report["original_native"] = original_native(container, dollars + constants)
            for row in authored:
                path = base.SOURCE / base.text(row["path"])
                data = base.text(row["content"]).encode()
                if base.digest(data) != row["sha256"]:
                    raise RuntimeError("authored test hash differs")
                if not path.exists():
                    created.append(path)
                path.write_bytes(data)
            base.run("stage-frozen-tests", ["git", "add", "--"] + [base.text(row["path"]) for row in authored])
            report["current_test_tree"] = source_snapshot(manifest, "current_tests")
            base.run("test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$", "./cmd/sqlc-gen-spira"], timeout=900)
            log = base.run("dollar-current-red", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestPostgresDollarQuotedBoundaries$", "./cmd/sqlc-gen-spira"], expected=1)
            names = {base.text(row["name"]) for row in dollars}
            current = test_outcomes(log, "TestPostgresDollarQuotedBoundaries", names)
            if "dollar-untagged" not in current["failed"] or "dollar-identifier-boundary" not in current["passed"]:
                raise RuntimeError("reported dollar regression or identifier control not observed")
            report["dollar_current"] = current
            log = base.run("compiled-direct-red", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestGeneratedQueryConstants$", "./cmd/sqlc-gen-spira"], expected=1, timeout=900)
            expected_direct = {base.text(row["name"]): base.text(row["sql"]) for row in constants}
            direct_values = compiled_values(log, expected_direct,
                {"runtime-cr", "runtime-crlf", "runtime-cr-value", "runtime-crlf-value"})
            report["compiled_direct"] = direct_values
            log = base.run("compiled-pipeline-red", ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^TestSQLcPostgresLiteralPipeline$", "./cmd/sqlc-gen-spira"], expected=1, timeout=900)
            old_outcomes = test_outcomes(log, "TestSQLcPostgresLiteralPipeline",
                                        {base.text(row["name"]) for row in old_cases})
            if old_outcomes["failed"]:
                raise RuntimeError("a retained source-level pipeline assertion failed")
            report["retained_source_pipeline"] = old_outcomes
            pipeline_values = compiled_values(log, expected_pipeline, {"escape-cr"})
            report["compiled_pipeline"] = pipeline_values
            base.run("retained-generator-controls", ["logos", "test", "-race", "-count=1", "-v",
                     "-run", "^(TestPostgresTypedLiteralBoundaries|TestPostgresContinuedEscapeStrings|TestVersionedWrite)",
                     "./cmd/sqlc-gen-spira"], timeout=900)
            model_path = base.SOURCE / "cmd/sqlc-gen-spira/model.lgo"
            current_model = model_path.read_bytes()
            previous_model = (base.PROOF / "previous-model.lgo").read_bytes()
            if base.digest(previous_model) != manifest["previous_model_sha256"]:
                raise RuntimeError("wrong prior prefix model")
            try:
                model_path.write_bytes(previous_model)
                base.run("stage-prior-prefix", ["git", "add", "--", "cmd/sqlc-gen-spira/model.lgo"])
                report["previous_prefix_tree"] = source_snapshot(manifest, "previous_prefix_tests")
                base.run("previous-prefix-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                                   "./cmd/sqlc-gen-spira"], timeout=900)
                log = base.run("previous-prefix-dollar-observation", ["logos", "test", "-race", "-count=1", "-v",
                               "-run", "^TestPostgresDollarQuotedBoundaries$", "./cmd/sqlc-gen-spira"], expected=1)
                previous = test_outcomes(log, "TestPostgresDollarQuotedBoundaries", names)
                if "dollar-untagged" not in previous["passed"]:
                    raise RuntimeError("reported old-pass dollar contrast not observed")
                report["dollar_previous_prefix"] = previous
            finally:
                model_path.write_bytes(current_model)
                base.run("restore-current-prefix", ["git", "add", "--", "cmd/sqlc-gen-spira/model.lgo"])
            report["after_prefix_restore"] = source_snapshot(manifest, "current_tests")
            plugin = base.BUILD / "sqlc-gen-spira"
            base.run("build-current-plugin", ["logos", "build", "-o", str(plugin), "./cmd/sqlc-gen-spira"], timeout=900)
            pipeline_rows = []
            for case in dollars:
                pipeline_rows.append(pipeline.observe_case(container, case, plugin, set(current["passed"])))
                (base.OUTPUT / "dollar-pipeline-observations.json").write_text(
                    json.dumps(pipeline_rows, indent=2) + "\n", encoding="utf-8")
            if any(base.object_value(row["processed_native"])["matches"] is not True for row in pipeline_rows):
                raise RuntimeError("pinned sqlc changed a dollar-query native value")
            report["dollar_pipeline"] = pipeline_rows
            report["direct_compiled_native"] = compiled_native(
                container, constants, base.object_value(direct_values["observed"]), "direct-compiled",
                {"runtime-cr", "runtime-cr-value", "runtime-crlf-value"})
            selected = [row for row in old_cases if base.text(row["name"]) in expected_pipeline]
            report["pipeline_compiled_native"] = compiled_native(
                container, selected, base.object_value(pipeline_values["observed"]), "pipeline-compiled", {"escape-cr"})
            report["after_observations"] = source_snapshot(manifest, "current_tests")
            report["baseline_gates_passed"] = True
    finally:
        try:
            base.run("restore-original-source", ["git", "restore", "--source", base.text(source["head"]),
                                                "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored"] = source_snapshot(manifest, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            untracked = base.run("untracked-source", ["git", "ls-files", "--others", "--exclude-standard"])
            if untracked.strip():
                raise RuntimeError("temporary source paths remain")
            report["source_restored"] = True
        finally:
            report["baseline_verified"] = bool(report["baseline_gates_passed"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R38_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
