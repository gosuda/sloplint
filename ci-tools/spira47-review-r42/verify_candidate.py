"""Verify the exact SELECT role repair, permanent SQLc contracts and source gates."""
from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import archive_support
import baseline_support as base
import linter_support
import native_support as native
import observer
import proof_support as proof
import runtime_observer


def authored_rows(manifest: dict[str, object]) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    proposal = base.object_value(json.loads((base.PROOF / "authored-candidate.json").read_bytes()))
    raw = proposal["files"]
    if not isinstance(raw, list) or len(raw) != 7:
        raise ValueError("wrong authored candidate path set")
    rows = [base.object_value(row) for row in raw]
    if (len({base.text(row["path"]) for row in rows}) != 7
            or proposal["candidate_tree"] != base.object_value(manifest["candidate"])["tree"]):
        raise ValueError("candidate identity differs")
    for row in rows:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise ValueError("invalid authored candidate path")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("authored candidate content differs: " + str(relative))
    raw_tests: object = json.loads((base.PROOF / "authored-tests.json").read_bytes())
    if not isinstance(raw_tests, list) or len(raw_tests) != 4:
        raise ValueError("wrong frozen test path set")
    tests = [base.object_value(row) for row in raw_tests]
    by_path = {base.text(row["path"]): row for row in rows}
    if len({base.text(row["path"]) for row in tests}) != 4:
        raise ValueError("duplicate frozen test path")
    for row in tests:
        if by_path[base.text(row["path"])] != row:
            raise RuntimeError("candidate changes its frozen test bytes")
    return rows, tests


def apply(rows: list[dict[str, object]], created: set[Path], lane: str) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-" + lane, ["git", "add", "--"] + [base.text(row["path"]) for row in rows])


def test_outcomes(output: str, test: str, names: set[str], failures: set[str]) -> dict[str, list[str]]:
    actual = proof.test_outcomes(output, test, names)
    if set(actual["failed"]) != failures or set(actual["passed"]) != names - failures:
        raise RuntimeError("frozen test partition differs: " + test)
    parent = "FAIL" if failures else "PASS"
    if re.search(r"^\s*--- " + parent + ": " + re.escape(test) + r" ", output, re.MULTILINE) is None:
        raise RuntimeError("parent outcome is absent: " + test)
    return actual


def suite(name: str, test: str, names: set[str], failures: set[str]) -> dict[str, list[str]]:
    output = base.run(name, ["logos", "test", "-race", "-count=1", "-v", "-run",
                            "^" + test + "$", "./cmd/sqlc-gen-spira"],
                      expected=1 if failures else 0, timeout=1800)
    return test_outcomes(output, test, names, failures)


def compare_native(previous: dict[str, object], current: dict[str, object]) -> None:
    for key in ("name", "sql", "sql_sha256", "observed", "expected", "matches", "raw_output"):
        if previous[key] != current[key]:
            raise RuntimeError("native contract changed: " + base.text(current["name"]) + ": " + key)


def compare_query_metadata(previous: dict[str, object], current: dict[str, object]) -> None:
    keys = ["name", "sql", "sql_sha256", "option", "columns", "request_present"]
    if current["request_present"] is True:
        keys.extend(["processed_sql", "processed_sql_sha256", "query_metadata",
                     "catalog_sha256", "catalog_canonical_bytes", "catalog_encoding"])
        compare_native(base.object_value(previous["processed_native"]),
                       base.object_value(current["processed_native"]))
    else:
        keys.extend(["pipeline_contract", "raw_output", "files", "processed_native"])
    for key in keys:
        if previous[key] != current[key]:
            raise RuntimeError("actual SQLc handoff changed: " + base.text(current["name"]) + ": " + key)
    print("R42_SQLC_HANDOFF_UNCHANGED " + json.dumps({
        "name": current["name"], "request_present": current["request_present"],
        "query_metadata_sha256": (base.digest(json.dumps(current["query_metadata"], sort_keys=True,
                                                        separators=(",", ":")).encode())
                                  if current["request_present"] else None),
    }), flush=True)


def verify_requirements(manifest: dict[str, object]) -> dict[str, object]:
    specification = base.object_value(manifest["documentation"])
    lines = specification["added_requirement_lines"]
    if not isinstance(lines, list) or len(lines) != 21:
        raise ValueError("changed requirement set differs")
    document = (base.SOURCE / "docs/specs/sqlc-gen-spira.md").read_text(encoding="utf-8")
    requirements = document.split("## Requirements\n", 1)[1].split("## Errors\n", 1)[0]
    counts: list[int] = []
    for raw in lines:
        line = base.text(raw)
        count = len(line.removeprefix("- ").split())
        if line not in requirements.splitlines() or count > 20 or "MUST" not in line:
            raise RuntimeError("changed requirement violates its sentence contract")
        counts.append(count)
    return {"sentences": len(counts), "maximum_whitespace_delimited_words": max(counts)}


def verify_runtime_suites(report: dict[str, object], manifest: dict[str, object]) -> None:
    real, launcher, old_path = runtime_observer.install_argv_observer()
    tests = ("TestGeneratedQueryConstants", "TestSQLcPostgresLiteralPipeline",
             "TestSQLcPostgresVersionPipeline")
    try:
        report["real_logos"] = {"path": str(real), "sha256": base.digest(real.read_bytes())}
        output = base.run("candidate-generated-runtime-suites",
                          ["logos", "test", "-race", "-exec", str(launcher), "-count=1", "-v",
                           "-run", "^(" + "|".join(tests) + ")$", "./cmd/sqlc-gen-spira"], timeout=2400)
        contracts = base.object_value(manifest["contracts"])
        for test in tests:
            names = observer.names(contracts[test])
            report[test] = test_outcomes(output, test, names, set())
        report["compiled_query_values"] = verify_compiled_records(output, contracts)
        report["nested_test_argv"] = runtime_observer.nested_race_records()
    finally:
        os.environ["PATH"] = old_path


def verify_compiled_records(output: str, contracts: dict[str, object]) -> list[dict[str, object]]:
    expected = contracts["compiled_query_values"]
    if not isinstance(expected, list) or len(expected) != 3:
        raise ValueError("wrong compiled query-value expectation set")
    remaining = [base.object_value(row) for row in expected]
    observed = [base.object_value(json.loads(match)) for match in
                re.findall(r"COMPILED_SQLC_QUERY_VALUES (\{[^\n]+\})", output)]
    if len(observed) != 3:
        raise RuntimeError("missing actual compiled query-value records")
    for row in observed:
        if row not in remaining:
            raise RuntimeError("actual compiled query values differ")
        remaining.remove(row)
    if remaining:
        raise RuntimeError("incomplete compiled query-value set")
    return observed


def main() -> None:
    artifact_token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
    manifest = base.object_value(json.loads((base.PROOF / "candidate-manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof digest differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r42.yml"
    if base.digest(active.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    contracts = base.object_value(manifest["contracts"])
    os.environ["GOWORK"] = "off"
    report: dict[str, object] = {
        "schema": 1, "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "manifest": manifest, "manifest_sha256": base.digest((base.PROOF / "candidate-manifest.json").read_bytes()),
        "regressions_verified": False, "focused_gates_passed": False, "source_gates_passed": False,
        "source_restored": False, "publication_eligible": False,
    }
    created: set[Path] = set()
    initial_path = os.environ["PATH"]
    try:
        producer = base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip()
        if producer != os.environ["GITHUB_SHA"]:
            raise RuntimeError("proof checkout differs")
        if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
            raise RuntimeError("wrong unchanged source")
        report["before"] = proof.source_snapshot(manifest, "source")
        report["authenticated_baseline"], baseline = archive_support.authenticate_baseline(manifest, artifact_token)
        report["authenticated_prior"], prior = archive_support.authenticate_prior(manifest, artifact_token)
        report["authenticated_linter"] = linter_support.install_linter(manifest, artifact_token)
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run(
            "sqlc-module", ["go", "-C", str(base.SOURCE / "tools"), "list", "-m", "-json",
                            "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned SQLc differs")
        catalog = observer.rows_from("version-catalog.json", 11)
        ctes = observer.rows_from("version-ctes.json", 26)
        roles = observer.rows_from("version-relation-roles.json", 30)
        cases = catalog + ctes + roles
        expectations = observer.rows_from("sqlc-version-pipeline.json", 67)
        if [case["name"] for case in cases] != [row["name"] for row in expectations]:
            raise RuntimeError("complete permanent/native case order differs")
        for case, expected in zip(cases, expectations, strict=True):
            if (case["sql"] != expected["sql"] or observer.columns(case) != expected["columns"]
                    or case.get("option", "version") != expected["option"]):
                raise RuntimeError("permanent SQLc input contract differs")
        candidate, tests = authored_rows(manifest)
        apply(tests, created, "unchanged-tests")
        report["tests_before"] = proof.source_snapshot(manifest, "tests")
        base.run("unchanged-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                           "./cmd/sqlc-gen-spira"], timeout=1200)
        report["unchanged_roles"] = suite(
            "unchanged-relation-roles", "TestPostgresRelationReferenceRoles",
            {base.text(row["name"]) for row in roles}, observer.names(contracts["current_failed"]))
        report["unchanged_permanent_pipeline"] = suite(
            "unchanged-permanent-version-pipeline", "TestSQLcPostgresVersionPipeline",
            {base.text(row["name"]) for row in expectations},
            observer.names(contracts["permanent_current_failed"]))
        report["tests_after"] = proof.source_snapshot(manifest, "tests")
        if report["tests_before"] != report["tests_after"]:
            raise RuntimeError("unchanged production/test tree moved")
        report["regressions_verified"] = True
        apply(candidate, created, "candidate")
        report["candidate_before"] = proof.source_snapshot(manifest, "candidate")
        report["documentation"] = verify_requirements(manifest)
        base.run("candidate-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                           "./cmd/sqlc-gen-spira"], timeout=1200)
        direct = base.object_value(contracts["direct_suites"])
        output = base.run("candidate-direct-and-retained-controls",
                          ["logos", "test", "-race", "-count=1", "-v",
                           "-run", "^(" + "|".join(direct) + ")$", "./cmd/sqlc-gen-spira"], timeout=1800)
        report["direct_suites"] = {
            test: test_outcomes(output, test, observer.names(names), set())
            for test, names in direct.items()
        }
        plugin = base.BUILD / "candidate-plugin"
        base.run("build-candidate-plugin", ["logos", "build", "-o", str(plugin),
                                           "./cmd/sqlc-gen-spira"], timeout=900)
        original_native, role_native = prior["native_original"], baseline["native_original"]
        original_pipeline, role_pipeline = prior["pipeline"], baseline["pipeline"]
        if (not isinstance(original_native, list) or not isinstance(role_native, list)
                or not isinstance(original_pipeline, list) or not isinstance(role_pipeline, list)):
            raise ValueError("authenticated observation arrays differ")
        prior_native = original_native + role_native
        prior_pipeline = original_pipeline + role_pipeline
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            native_rows: list[dict[str, object]] = []
            pipeline: list[dict[str, object]] = []
            report["native_original"] = native_rows
            report["candidate_pipeline"] = pipeline
            for case, expected, old_native, old_pipeline in zip(
                    cases, expectations, prior_native, prior_pipeline, strict=True):
                original = observer.observe_native(container, case, base.text(case["sql"]), "original")
                compare_native(base.object_value(old_native), original)
                native_rows.append(original)
                row = observer.observe_pipeline(container, case, plugin, "candidate",
                                                base.text(expected["error"]),
                                                upstream=expected["upstream"] is True)
                compare_query_metadata(base.object_value(old_pipeline), row)
                if row["request_present"] is True and row["processed_sql"] != expected["text"]:
                    raise RuntimeError("permanent captured text differs: " + base.text(case["name"]))
                pipeline.append(row)
                (base.OUTPUT / "candidate-pipeline.json").write_text(
                    json.dumps(pipeline, indent=2) + "\n", encoding="utf-8")
            partition = {kind: sum(row["pipeline_contract"] == kind for row in pipeline)
                         for kind in ("generated", "plugin-refusal", "upstream")}
            if partition != {"generated": 30, "plugin-refusal": 34, "upstream": 3}:
                raise RuntimeError("complete candidate pipeline partition differs")
            report["pipeline_partition"] = partition
        verify_runtime_suites(report, manifest)
        report["focused_gates_passed"] = True
        base.run("just-gate", ["just", "gate"], timeout=3600)
        base.run("just-slop", ["just", "slop"], timeout=1200)
        base.run("just-eject", ["just", "eject"], timeout=1800)
        base.run("candidate-whitespace", ["git", "diff", "--cached", "--check", "HEAD"])
        base.run("candidate-unstaged", ["git", "diff", "--exit-code"])
        if base.run("candidate-untracked", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
            raise RuntimeError("unexpected untracked candidate source")
        report["candidate_after"] = proof.source_snapshot(manifest, "candidate")
        if report["candidate_before"] != report["candidate_after"]:
            raise RuntimeError("candidate source changed during required gates")
        report["source_gates_passed"] = True
    finally:
        os.environ["PATH"] = initial_path
        try:
            base.run("restore-original-source", ["git", "restore", "--source", base.text(source["head"]),
                                                "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored"] = proof.source_snapshot(manifest, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            if base.run("untracked-restoration", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
                raise RuntimeError("unexpected untracked original source")
            report["source_restored"] = True
        finally:
            commands = [asdict(row) for row in base.RECEIPTS]
            unique = len({row["name"] for row in commands}) == len(commands)
            report["commands"] = commands
            report["unique_command_names"] = unique
            report["publication_eligible"] = bool(
                unique and report["regressions_verified"] and report["focused_gates_passed"]
                and report["source_gates_passed"] and report["source_restored"])
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R42_CANDIDATE_REPORT " + json.dumps(report), flush=True)
            if not unique:
                raise RuntimeError("command names must not overwrite earlier evidence")


if __name__ == "__main__":
    main()
