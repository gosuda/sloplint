"""Prove the final reference-role repair and nested race argv, then required source gates."""
from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import candidate_archive
import linter_support
import native_support as native
import proof_support as proof
import reference_observer as observer


def authored_rows() -> list[dict[str, object]]:
    value: object = json.loads((base.PROOF / "authored-candidate.json").read_bytes())
    if not isinstance(value, list) or len(value) != 8:
        raise ValueError("wrong authored candidate set")
    rows = [base.object_value(row) for row in value]
    if len({base.text(row["path"]) for row in rows}) != 8:
        raise ValueError("duplicate authored path")
    for row in rows:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise ValueError("invalid authored path")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("authored content identity differs: " + str(relative))
    return rows


def apply(rows: list[dict[str, object]], created: set[Path], lane: str) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-" + lane, ["git", "add", "--"] + [base.text(row["path"]) for row in rows])


def run_reference_suite(test: str, cases: list[dict[str, object]], lane: str,
                        unchanged: bool) -> dict[str, list[str]]:
    expected = 1 if unchanged else 0
    output = base.run(lane, ["logos", "test", "-race", "-count=1", "-v", "-run",
                            "^" + test + "$", "./cmd/sqlc-gen-spira"], expected=expected, timeout=900)
    names = {base.text(case["name"]) for case in cases}
    outcomes = proof.test_outcomes(output, test, names)
    wanted_failed = {base.text(case["name"]) for case in cases if case["allow"] is False} if unchanged else set()
    if set(outcomes["failed"]) != wanted_failed or set(outcomes["passed"]) != names - wanted_failed:
        raise RuntimeError("frozen reference outcome partition differs: " + lane)
    if not unchanged and re.search(r"^\s*--- PASS: " + re.escape(test) + r" ", output, re.MULTILINE) is None:
        raise RuntimeError("candidate parent suite did not pass: " + test)
    return outcomes


def compare_query_metadata(previous: dict[str, object], current: dict[str, object]) -> None:
    for key in ("name", "sql", "sql_sha256", "processed_sql", "processed_sql_sha256", "query_metadata"):
        if previous[key] != current[key]:
            raise RuntimeError("actual SQLc request changed: " + base.text(current["name"]) + ": " + key)
    print("REFERENCE_REQUEST_METADATA_UNCHANGED " + json.dumps({
        "name": current["name"], "processed_sql_sha256": current["processed_sql_sha256"],
        "query_metadata_sha256": base.digest(json.dumps(current["query_metadata"], sort_keys=True,
                                                      separators=(",", ":")).encode()),
    }), flush=True)


def pipeline_contract(case: dict[str, object], unchanged: bool) -> str:
    if case["name"] == "qualified-operator-operand":
        return "unnamed-column"
    if unchanged or case["allow"] is True:
        return "generated"
    return "version-refusal"


def verify_runtime_suites(report: dict[str, object]) -> None:
    real, launcher, old_path = observer.install_argv_observer()
    try:
        report["real_logos"] = {"path": str(real), "sha256": base.digest(real.read_bytes())}
        for name, test in (("retained-direct-runtime", "TestGeneratedQueryConstants"),
                           ("retained-pipeline-runtime", "TestSQLcPostgresLiteralPipeline")):
            output = base.run(name, ["logos", "test", "-race", "-exec", str(launcher), "-count=1",
                                     "-v", "-run", "^" + test + "$", "./cmd/sqlc-gen-spira"], timeout=1800)
            if re.search(r"^\s*--- PASS: " + re.escape(test) + r" ", output, re.MULTILINE) is None:
                raise RuntimeError("retained runtime suite did not pass: " + test)
        report["nested_test_argv"] = observer.nested_race_records()
    finally:
        os.environ["PATH"] = old_path


def main() -> None:
    artifact_token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
    manifest = base.object_value(json.loads((base.PROOF / "candidate-manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof digest differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r40.yml"
    if base.digest(active.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    producer = base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip()
    if producer != os.environ["GITHUB_SHA"]:
        raise RuntimeError("producer checkout differs")
    os.environ["GOWORK"] = "off"
    if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
        raise RuntimeError("wrong unchanged source")
    report: dict[str, object] = {
        "schema": 1, "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "manifest": manifest, "manifest_sha256": base.digest((base.PROOF / "candidate-manifest.json").read_bytes()),
        "focused_gates_passed": False, "source_gates_passed": False,
        "source_restored": False, "publication_eligible": False,
    }
    created: set[Path] = set()
    initial_path = os.environ["PATH"]
    try:
        report["before"] = proof.source_snapshot(manifest, "source")
        report["authenticated_baseline"], baseline = candidate_archive.authenticate(manifest, artifact_token)
        report["authenticated_linter"] = linter_support.install_linter(manifest, artifact_token)
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run("sqlc-module", ["go", "-C", str(base.SOURCE / "tools"),
                                                     "list", "-m", "-json", "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned SQLc differs")
        original = observer.rows_from("version-references.json", 20)
        boundaries = observer.rows_from("version-reference-boundaries.json", 6)
        cases = original + boundaries
        if len({base.text(row["name"]) for row in cases}) != 26:
            raise RuntimeError("combined frozen case identities differ")
        authored = authored_rows()
        test_paths = {
            "cmd/sqlc-gen-spira/continued_escape_test.lgo",
            "cmd/sqlc-gen-spira/version_reference_test.lgo",
            "cmd/sqlc-gen-spira/testdata/version-references.json",
            "cmd/sqlc-gen-spira/testdata/version-reference-boundaries.json",
        }
        tests = [row for row in authored if row["path"] in test_paths]
        if {base.text(row["path"]) for row in tests} != test_paths:
            raise RuntimeError("unchanged-production test set differs")
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            report["native_original"] = [
                observer.observe_native(container, case, base.text(case["sql"]), "original") for case in cases
            ]
            apply(tests, created, "unchanged-tests")
            report["tests_before"] = proof.source_snapshot(manifest, "tests")
            base.run("unchanged-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
            report["original_reference_red"] = run_reference_suite(
                "TestPostgresVersionReferenceRoles", original, "unchanged-reference-red", True)
            report["boundary_reference_red"] = run_reference_suite(
                "TestPostgresVersionReferenceRoleBoundaries", boundaries, "unchanged-boundary-red", True)
            unchanged_plugin = base.BUILD / "unchanged-plugin"
            base.run("build-unchanged-plugin", ["logos", "build", "-o", str(unchanged_plugin),
                                               "./cmd/sqlc-gen-spira"], timeout=900)
            prior_new: list[dict[str, object]] = []
            report["unchanged_boundary_pipeline"] = prior_new
            for case in boundaries:
                prior_new.append(observer.observe_pipeline(
                    container, case, unchanged_plugin, "unchanged", pipeline_contract(case, True)))
                (base.OUTPUT / "unchanged-boundary-pipeline.json").write_text(
                    json.dumps(prior_new, indent=2) + "\n", encoding="utf-8")
            report["tests_after"] = proof.source_snapshot(manifest, "tests")
            if report["tests_before"] != report["tests_after"]:
                raise RuntimeError("unchanged production/test snapshot moved")
            apply(authored, created, "candidate")
            report["candidate_before"] = proof.source_snapshot(manifest, "candidate")
            base.run("candidate-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
            report["original_reference_green"] = run_reference_suite(
                "TestPostgresVersionReferenceRoles", original, "candidate-reference-green", False)
            report["boundary_reference_green"] = run_reference_suite(
                "TestPostgresVersionReferenceRoleBoundaries", boundaries, "candidate-boundary-green", False)
            plugin = base.BUILD / "candidate-plugin"
            base.run("build-candidate-plugin", ["logos", "build", "-o", str(plugin),
                                               "./cmd/sqlc-gen-spira"], timeout=900)
            old_pipeline = baseline["pipeline"]
            if not isinstance(old_pipeline, list):
                raise ValueError("authenticated baseline pipeline is not an array")
            prior_rows = [base.object_value(row) for row in old_pipeline] + prior_new
            previous = {base.text(row["name"]): row for row in prior_rows}
            if len(previous) != 26:
                raise RuntimeError("actual unchanged request coverage differs")
            candidate_pipeline: list[dict[str, object]] = []
            report["candidate_pipeline"] = candidate_pipeline
            for case in cases:
                row = observer.observe_pipeline(container, case, plugin, "candidate", pipeline_contract(case, False))
                compare_query_metadata(previous[base.text(case["name"])], row)
                candidate_pipeline.append(row)
                (base.OUTPUT / "candidate-pipeline.json").write_text(
                    json.dumps(candidate_pipeline, indent=2) + "\n", encoding="utf-8")
            actual_partition = {
                kind: sum(row["pipeline_contract"] == kind for row in candidate_pipeline)
                for kind in ("generated", "version-refusal", "unnamed-column")
            }
            if actual_partition != {"generated": 10, "version-refusal": 15, "unnamed-column": 1}:
                raise RuntimeError("final actual pipeline partition differs")
            report["pipeline_partition"] = actual_partition
        verify_runtime_suites(report)
        base.run("retained-generator-controls", ["logos", "test", "-race", "-count=1", "-v", "-run",
                 "^(TestPostgresDollarQuotedBoundaries|TestPostgresTypedLiteralBoundaries|TestPostgresContinuedEscapeStrings|TestVersionedWrite)",
                 "./cmd/sqlc-gen-spira"], timeout=1800)
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
            raise RuntimeError("candidate source changed during its required gates")
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
            report["publication_eligible"] = bool(
                report["focused_gates_passed"] and report["source_gates_passed"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R40_CANDIDATE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
