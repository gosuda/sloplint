"""Verify the exact catalog/CTE repair against authenticated REDs and native full gates."""
from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import alias_archive
import baseline_support as base
import candidate_archive
import linter_support
import native_support as native
import observer
import proof_support as proof
import projection_boundary
import runtime_observer


def authored_rows(manifest: dict[str, object]) -> list[dict[str, object]]:
    proposal = base.object_value(json.loads((base.PROOF / "authored-candidate.json").read_bytes()))
    raw = proposal["files"]
    if not isinstance(raw, list) or len(raw) != 15:
        raise ValueError("wrong authored candidate set")
    rows = [base.object_value(row) for row in raw]
    if (len({base.text(row["path"]) for row in rows}) != 15
            or proposal["candidate_tree"] != base.object_value(manifest["candidate"])["tree"]):
        raise ValueError("candidate identity differs")
    for row in rows:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise ValueError("invalid authored path")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("authored content identity differs: " + str(relative))
    frozen: object = json.loads((base.PROOF / "authored-tests.json").read_bytes())
    if not isinstance(frozen, list) or len(frozen) != 9:
        raise ValueError("wrong frozen test set")
    by_path = {base.text(row["path"]): row for row in rows}
    for raw_test in frozen:
        test = base.object_value(raw_test)
        if by_path[base.text(test["path"])] != test:
            raise RuntimeError("candidate altered a compiled RED test")
    for test in projection_boundary.source_rows("boundary-tests.json", 11):
        if by_path[base.text(test["path"])] != test:
            raise RuntimeError("candidate altered a frozen clause-boundary test")
    return rows


def apply(rows: list[dict[str, object]], created: set[Path]) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-candidate", ["git", "add", "--"] + [base.text(row["path"]) for row in rows])


def suite(lane: str, test: str, cases: list[dict[str, object]]) -> dict[str, list[str]]:
    output = base.run(lane, ["logos", "test", "-race", "-count=1", "-v", "-run",
                            "^" + test + "$", "./cmd/sqlc-gen-spira"], timeout=900)
    names = {base.text(case["name"]) for case in cases}
    actual = proof.test_outcomes(output, test, names)
    if actual["failed"] or set(actual["passed"]) != names:
        raise RuntimeError("candidate direct suite differs: " + test)
    if re.search(r"^\s*--- PASS: " + re.escape(test) + r" ", output, re.MULTILINE) is None:
        raise RuntimeError("candidate parent did not pass: " + test)
    return actual


def candidate_diagnostic(case: dict[str, object]) -> str:
    catalog = {
        "mixed-option-alias-refusal": "sqlc-gen-spira: ContinueItem: writes versioned table items without version",
        "quoted-column-alias-refusal": "sqlc-gen-spira: ContinueItem: writes versioned table items without Version",
        "exact-option-rejects-quoted-sibling": "sqlc-gen-spira: ContinueItem: writes versioned table items without Version",
        "missing-configured-column": 'sqlc-gen-spira: tables.items.version "missing" is not a column of public.items',
        "ambiguous-folded-option": 'sqlc-gen-spira: tables.items.version "version" is ambiguous in public.items',
    }
    name = base.text(case["name"])
    if name in catalog:
        return catalog[name]
    if case.get("allow") is False:
        return "sqlc-gen-spira: ContinueItem: writes versioned table items without version"
    return ""


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
    print("CATALOG_CTE_HANDOFF_UNCHANGED " + json.dumps({
        "name": current["name"], "request_present": current["request_present"],
        "query_metadata_sha256": (base.digest(json.dumps(current["query_metadata"], sort_keys=True,
                                                        separators=(",", ":")).encode())
                                  if current["request_present"] else None),
    }), flush=True)


def verify_runtime_suites(report: dict[str, object], manifest: dict[str, object]) -> None:
    real, launcher, old_path = runtime_observer.install_argv_observer()
    try:
        report["real_logos"] = {"path": str(real), "sha256": base.digest(real.read_bytes())}
        for name, test in (("retained-direct-runtime", "TestGeneratedQueryConstants"),
                           ("retained-pipeline-runtime", "TestSQLcPostgresLiteralPipeline")):
            output = base.run(name, ["logos", "test", "-race", "-exec", str(launcher), "-count=1",
                                     "-v", "-run", "^" + test + "$", "./cmd/sqlc-gen-spira"], timeout=1800)
            if re.search(r"^\s*--- PASS: " + re.escape(test) + r" ", output, re.MULTILINE) is None:
                raise RuntimeError("retained runtime suite did not pass: " + test)
            if test == "TestSQLcPostgresLiteralPipeline":
                names = observer.names(base.object_value(manifest["contracts"])["pipeline_names"])
                if len(names) != 60:
                    raise RuntimeError("permanent pipeline identity set differs")
                actual = proof.test_outcomes(output, test, names)
                if actual["failed"] or set(actual["passed"]) != names:
                    raise RuntimeError("permanent pipeline did not pass all60 cases")
                report["permanent_pipeline"] = actual
        report["nested_test_argv"] = runtime_observer.nested_race_records()
    finally:
        os.environ["PATH"] = old_path


def main() -> None:
    artifact_token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
    manifest = base.object_value(json.loads((base.PROOF / "candidate-manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof digest differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r41.yml"
    if base.digest(active.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    os.environ["GOWORK"] = "off"
    report: dict[str, object] = {
        "schema": 1, "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "manifest": manifest, "manifest_sha256": base.digest((base.PROOF / "candidate-manifest.json").read_bytes()),
        "boundary_observations_complete": False, "focused_gates_passed": False, "source_gates_passed": False,
        "source_restored": False, "publication_eligible": False,
    }
    created: set[Path] = set()
    initial_path = os.environ["PATH"]
    try:
        producer = base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip()
        if producer != os.environ["GITHUB_SHA"]:
            raise RuntimeError("producer checkout differs")
        if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
            raise RuntimeError("wrong unchanged source")
        report["before"] = proof.source_snapshot(manifest, "source")
        report["authenticated_baseline"], baseline = candidate_archive.authenticate(manifest, artifact_token)
        report["authenticated_alias"], alias_baseline = alias_archive.authenticate(manifest, artifact_token)
        report["authenticated_linter"] = linter_support.install_linter(manifest, artifact_token)
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run("sqlc-module", ["go", "-C", str(base.SOURCE / "tools"),
                                                     "list", "-m", "-json", "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned SQLc differs")
        catalog = observer.rows_from("version-catalog.json", 11)
        ctes = observer.rows_from("version-ctes.json", 26)
        projections = observer.rows_from("version-projections.json", 10)
        clauses = observer.rows_from("version-projection-clauses.json", 2)
        cases = catalog + ctes + projections + clauses
        if len({base.text(row["name"]) for row in cases}) != 49:
            raise RuntimeError("combined frozen case identities differ")
        authored = authored_rows(manifest)
        boundary_native, boundary_pipeline = projection_boundary.observe(manifest, report, created)
        apply(authored, created)
        report["candidate_before"] = proof.source_snapshot(manifest, "candidate")
        base.run("candidate-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                           "./cmd/sqlc-gen-spira"], timeout=1200)
        report["catalog_green"] = suite(
            "catalog-candidate-green", "TestPostgresVersionCatalogResolution", catalog)
        report["cte_green"] = suite("cte-candidate-green", "TestPostgresVersionCTEs", ctes)
        report["projection_green"] = suite(
            "projection-candidate-green", "TestPostgresCTEProjectionRoles", projections)
        report["clause_green"] = suite(
            "clause-candidate-green", "TestPostgresCTEProjectionClauseBoundaries", clauses)
        plugin = base.BUILD / "candidate-plugin"
        base.run("build-candidate-plugin", ["logos", "build", "-o", str(plugin),
                                           "./cmd/sqlc-gen-spira"], timeout=900)
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            original_native, alias_native = baseline["native_original"], alias_baseline["native_original"]
            original_pipeline, alias_pipeline = baseline["pipeline"], alias_baseline["unchanged_pipeline"]
            if (not isinstance(original_native, list) or not isinstance(alias_native, list)
                    or not isinstance(original_pipeline, list) or not isinstance(alias_pipeline, list)):
                raise ValueError("authenticated observations are not arrays")
            prior_native = original_native + alias_native + boundary_native
            prior_pipeline = original_pipeline + alias_pipeline + boundary_pipeline
            native_rows: list[dict[str, object]] = []
            pipeline: list[dict[str, object]] = []
            report["native_original"] = native_rows
            report["candidate_pipeline"] = pipeline
            for case, old_native, old_pipeline in zip(cases, prior_native, prior_pipeline, strict=True):
                original = observer.observe_native(container, case, base.text(case["sql"]), "original")
                compare_native(base.object_value(old_native), original)
                native_rows.append(original)
                upstream = case["name"] == "nested-materialized-cte-reference"
                diagnostic = ('queries.sql:1:1: relation "b" does not exist' if upstream
                              else candidate_diagnostic(case))
                row = observer.observe_pipeline(container, case, plugin, "candidate", diagnostic, upstream=upstream)
                compare_query_metadata(base.object_value(old_pipeline), row)
                pipeline.append(row)
                (base.OUTPUT / "candidate-pipeline.json").write_text(
                    json.dumps(pipeline, indent=2) + "\n", encoding="utf-8")
            partition = {kind: sum(row["pipeline_contract"] == kind for row in pipeline)
                         for kind in ("generated", "plugin-refusal", "upstream")}
            if partition != {"generated": 27, "plugin-refusal": 21, "upstream": 1}:
                raise RuntimeError("final actual pipeline partition differs")
            report["pipeline_partition"] = partition
        verify_runtime_suites(report, manifest)
        base.run("retained-generator-controls", ["logos", "test", "-race", "-count=1", "-v", "-run",
                 "^(TestPostgresDollarQuotedBoundaries|TestPostgresTypedLiteralBoundaries|TestPostgresContinuedEscapeStrings|TestVersionedWrite|TestPostgresVersionReference(Roles|RoleBoundaries))",
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
            print("SPIRA_R41_CANDIDATE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
