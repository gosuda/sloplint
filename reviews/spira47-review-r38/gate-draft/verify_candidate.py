"""Prove the frozen final tests RED, apply the reviewed repair, and run all source gates."""
from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import candidate_support as support
import native_support as native
import pipeline_support as pipeline
import verify_baseline as observation


def read_rows(name: str, count: int) -> list[dict[str, object]]:
    rows = observation.read_rows(name, count)
    if len({base.text(row["name"]) for row in rows}) != count:
        raise RuntimeError("duplicate frozen case: " + name)
    return rows


def authored_files() -> list[dict[str, object]]:
    value = observation.read_object("authored-candidate.json")
    rows = value["files"]
    if not isinstance(rows, list) or len(rows) != 10:
        raise RuntimeError("authored source path count differs")
    result = [base.object_value(row) for row in rows]
    if len({base.text(row["path"]) for row in result}) != len(result):
        raise RuntimeError("duplicate authored source path")
    for row in result:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise RuntimeError("invalid authored source path or mode")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("authored source digest differs")
    return result


def apply_files(rows: list[dict[str, object]], created: set[Path]) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
    base.run("stage-source-" + str(len(rows)), ["git", "add", "--"] +
             [base.text(row["path"]) for row in rows])


def require_passes(log: str, names: set[str]) -> None:
    passed = set(re.findall(r"^\s*--- PASS: ([^ ]+) ", log, re.MULTILINE))
    if not names <= passed:
        raise RuntimeError("missing literal PASS: " + repr(names - passed))


def run_test(name: str, test: str, expected: int = 0) -> str:
    return base.run(name, ["logos", "test", "-race", "-count=1", "-v", "-run", "^" + test + "$",
                          "./cmd/sqlc-gen-spira"], expected=expected, timeout=1200)


def require_cases(log: str, test: str, cases: list[dict[str, object]],
                  expected_failures: set[str]) -> dict[str, list[str]]:
    names = {base.text(row["name"]) for row in cases}
    outcomes = observation.test_outcomes(log, test, names)
    if set(outcomes["failed"]) != expected_failures:
        raise RuntimeError("frozen failure partition differs: " + test)
    if not expected_failures:
        require_passes(log, {test})
    return outcomes


def unchanged_tests(manifest: dict[str, object], rows: list[dict[str, object]], created: set[Path],
                    dollars: list[dict[str, object]], cases: list[dict[str, object]],
                    constants: list[dict[str, object]]) -> dict[str, object]:
    source_paths = {"cmd/sqlc-gen-spira/model.lgo", "cmd/sqlc-gen-spira/gen.lgo", "docs/specs/sqlc-gen-spira.md"}
    tests = [row for row in rows if row["path"] not in source_paths]
    if len(tests) != 7:
        raise RuntimeError("final unchanged-production test path set differs")
    apply_files(tests, created)
    identity = observation.source_snapshot(manifest, "current_tests")
    base.run("permanent-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                      "./cmd/sqlc-gen-spira"], timeout=1200)
    frozen = observation.read_object("baseline-report.json")
    raw_failures = base.object_value(frozen["dollar_current"])["failed"]
    if not isinstance(raw_failures, list):
        raise RuntimeError("frozen dollar failures are not a list")
    failures = {base.text(name) for name in raw_failures}
    failures.add("dollar-escaped-quoted-identifier")
    dollar_log = run_test("permanent-dollar-unchanged-red", "TestPostgresDollarQuotedBoundaries", 1)
    dollar = require_cases(dollar_log, "TestPostgresDollarQuotedBoundaries", dollars, failures)
    direct_log = run_test("permanent-compiled-unchanged-red", "TestGeneratedQueryConstants", 1)
    expected = {base.text(row["name"]): base.text(row["sql"]) for row in constants}
    direct = observation.compiled_values(direct_log, expected,
        {"runtime-cr", "runtime-crlf", "runtime-cr-value", "runtime-crlf-value"})
    pipeline_log = run_test("permanent-pipeline-unchanged-red", "TestSQLcPostgresLiteralPipeline", 1)
    outcomes = require_cases(pipeline_log, "TestSQLcPostgresLiteralPipeline", cases, failures)
    if len(outcomes["failed"]) != 11 or len(outcomes["passed"]) != 29:
        raise RuntimeError("final pipeline RED/control cardinality differs")
    after = observation.source_snapshot(manifest, "current_tests")
    if identity != after:
        raise RuntimeError("unchanged-production test source changed")
    return {"before": identity, "dollar": dollar, "compiled_direct": direct, "pipeline": outcomes, "after": after}


def final_pipeline(container: str, cases: list[dict[str, object]], expected: list[dict[str, object]],
                   compiled: dict[str, object]) -> list[dict[str, object]]:
    plugin = base.BUILD / "sqlc-gen-spira-fixed"
    base.run("build-fixed-plugin", ["logos", "build", "-o", str(plugin), "./cmd/sqlc-gen-spira"], timeout=900)
    rows: list[dict[str, object]] = []
    for case, want in zip(cases, expected, strict=True):
        row = pipeline.observe_sqlc(container, case, plugin, want)
        if row["captured_request"]:
            if base.object_value(row["processed_native"])["matches"] is not True:
                raise RuntimeError("sqlc preprocessing changed native query semantics")
            if row["outcome"] == "generated" and compiled[base.text(case["name"])] != row["processed_sql"]:
                raise RuntimeError("compiled value differs from independently captured request")
        rows.append(row)
        (base.OUTPUT / "final-pipeline.json").write_text(json.dumps(rows, indent=2) + "\n", encoding="utf-8")
    counts = {name: sum(row["outcome"] == name for row in rows)
              for name in ("generated", "version-refusal", "upstream-rejection")}
    if counts != {"generated": 32, "version-refusal": 4, "upstream-rejection": 4}:
        raise RuntimeError("final actual pipeline partition differs")
    return rows


def main() -> None:
    manifest = observation.read_object("candidate-manifest.json")
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("frozen proof digest differs: " + name)
    source = base.object_value(manifest["source"])
    if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
        raise RuntimeError("wrong maintained source")
    report: dict[str, object] = {"schema": 1, "manifest": manifest, "source_gates_passed": False,
                                "source_restored": False, "publication_eligible": False}
    created: set[Path] = set()
    try:
        report["before"] = observation.source_snapshot(manifest, "source")
        token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
        try:
            report["baseline_evidence"] = support.authenticate_baseline(manifest, token)
            report["linter"] = support.install_linter(manifest, token)
        finally:
            token = ""
        os.environ["GOWORK"] = "off"
        for name, command, pattern in (
            ("logos-version", ["logos", "version"], r"^logos version go1\.27\.4 linux/amd64$"),
            ("node-version", ["node", "--version"], r"^v26\.13\.1$"),
            ("pnpm-version", ["pnpm", "--version"], r"^12\.21\.0$"),
            ("just-version", ["just", "--version"], r"^just 1\.59\.0$"),
        ):
            if not re.fullmatch(pattern, base.run(name, command).strip()):
                raise RuntimeError("wrong " + name)
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sloplint-version", ["sloplint", "--version"])
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run("sqlc-module", ["go", "-C", str(base.SOURCE / "tools"),
                  "list", "-m", "-json", "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned sqlc differs")
        dollars = read_rows("dollar-strings-final.json", 14)
        constants = read_rows("query-constant-values.json", 8)
        old_cases = read_rows("retained-native-cases.json", 26)
        cases = old_cases + dollars
        expected = read_rows("sqlc-literal-pipeline-final.json", 40)
        baseline_report = observation.read_object("baseline-report.json")
        native_original = baseline_report["original_native"]
        if not isinstance(native_original, list):
            raise RuntimeError("frozen native baseline is not a list")
        for case, native_row in zip(dollars[:13], native_original[:13], strict=True):
            recorded = base.object_value(native_row)
            if recorded["id"] != case["name"] or recorded["sql"] != case["sql"]:
                raise RuntimeError("an original dollar fixture changed")
        rows = authored_files()
        if [row["path"] for row in rows] != base.object_value(manifest["candidate"])["changed_paths"]:
            raise RuntimeError("candidate authored path order differs")
        with native.postgres(manifest) as (container, version):
            report["native_version"] = version
            report["original_native"] = observation.original_native(container, cases + constants)
            report["unchanged_tests"] = unchanged_tests(manifest, rows, created, dollars, cases, constants)
            apply_files(rows, created)
            report["candidate_before"] = observation.source_snapshot(manifest, "candidate")
            base.run("candidate-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                          "./cmd/sqlc-gen-spira"], timeout=1200)
            log = run_test("dollar-generator-green", "TestPostgresDollarQuotedBoundaries")
            report["dollar_green"] = require_cases(log, "TestPostgresDollarQuotedBoundaries", dollars, set())
            log = run_test("direct-compiled-green", "TestGeneratedQueryConstants")
            require_passes(log, {"TestGeneratedQueryConstants"})
            expected_direct = {base.text(row["name"]): base.text(row["sql"]) for row in constants}
            direct = observation.compiled_values(log, expected_direct, set())
            report["compiled_direct"] = direct
            log = run_test("permanent-pipeline-green", "TestSQLcPostgresLiteralPipeline")
            report["pipeline_green"] = require_cases(log, "TestSQLcPostgresLiteralPipeline", cases, set())
            expected_values = {base.text(row["name"]): base.text(want["text"])
                               for row, want in zip(cases, expected, strict=True)
                               if row["allow"] is True and not want["sqlc_error"]}
            if len(expected_values) != 32:
                raise RuntimeError("compiled pipeline query count differs")
            values = observation.compiled_values(log, expected_values, set())
            report["compiled_pipeline"] = values
            retained = base.run("retained-generator-controls", ["logos", "test", "-race", "-count=1", "-v", "-run",
                     "^(TestPostgresTypedLiteralBoundaries|TestPostgresContinuedEscapeStrings|TestVersionedWrite)",
                     "./cmd/sqlc-gen-spira"], timeout=1200)
            continued = {"TestPostgresContinuedEscapeStrings/" + base.text(row["name"]) for row in old_cases[:14]}
            typed = {"TestPostgresTypedLiteralBoundaries/" + base.text(row["name"]) for row in old_cases[14:]}
            require_passes(retained, continued | typed |
                           {"TestPostgresContinuedEscapeStrings", "TestPostgresTypedLiteralBoundaries"})
            report["native_pipeline"] = final_pipeline(container, cases, expected, base.object_value(values["observed"]))
            report["direct_compiled_native"] = observation.compiled_native(
                container, constants, base.object_value(direct["observed"]), "direct-compiled-green", set())
            selected = [case for case in cases if base.text(case["name"]) in expected_values]
            report["pipeline_compiled_native"] = observation.compiled_native(
                container, selected, base.object_value(values["observed"]), "pipeline-compiled-green", set())
            report["after_focused"] = observation.source_snapshot(manifest, "candidate")
        base.run("just-gate", ["just", "gate"], timeout=3600)
        base.run("just-slop", ["just", "slop"], timeout=600)
        base.run("just-eject", ["just", "eject"], timeout=1800)
        base.run("whitespace", ["git", "diff", "--cached", "--check", "HEAD"])
        base.run("tracked-source-unchanged", ["git", "diff", "--exit-code"])
        if base.run("untracked-source", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
            raise RuntimeError("unexpected untracked source")
        report["candidate_after"] = observation.source_snapshot(manifest, "candidate")
        if report["candidate_after"] != report["candidate_before"]:
            raise RuntimeError("candidate source changed during gates")
        report["source_gates_passed"] = True
    finally:
        try:
            base.run("restore-original-source", ["git", "restore", "--source", base.text(source["head"]),
                                                "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored"] = observation.source_snapshot(manifest, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            if base.run("restored-untracked-source", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
                raise RuntimeError("temporary source remains after restoration")
            report["source_restored"] = True
        finally:
            report["publication_eligible"] = bool(report["source_gates_passed"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R38_CANDIDATE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
