"""Freeze catalog/CTE failures and permanent SQLc coverage on unchanged production."""
from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import native_support as native
import observer
import proof_support as proof


def authored_rows() -> list[dict[str, object]]:
    raw: object = json.loads((base.PROOF / "authored-tests.json").read_bytes())
    if not isinstance(raw, list) or len(raw) != 7:
        raise ValueError("wrong frozen test path set")
    rows = [base.object_value(row) for row in raw]
    if len({base.text(row["path"]) for row in rows}) != 7:
        raise ValueError("duplicate frozen test path")
    for row in rows:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise ValueError("invalid frozen test path")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("frozen test identity differs: " + str(relative))
    return rows


def apply_tests(rows: list[dict[str, object]], created: set[Path]) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-test-only-source", ["git", "add", "--"] + [base.text(row["path"]) for row in rows])


def suite(name: str, test: str, names: set[str], failures: set[str],
          timeout: int = 900) -> dict[str, list[str]]:
    output = base.run(name, ["logos", "test", "-race", "-count=1", "-v", "-run",
                            "^" + test + "$", "./cmd/sqlc-gen-spira"],
                      expected=1 if failures else 0, timeout=timeout)
    actual = proof.test_outcomes(output, test, names)
    if set(actual["failed"]) != failures or set(actual["passed"]) != names - failures:
        raise RuntimeError("frozen outcome partition differs: " + name)
    parent = "FAIL" if failures else "PASS"
    if re.search(r"^\s*--- " + parent + ": " + re.escape(test) + r" ", output, re.MULTILINE) is None:
        raise RuntimeError("parent suite outcome missing: " + name)
    return actual


def baseline_diagnostic(case: dict[str, object], refusals: set[str]) -> str:
    if base.text(case["name"]) not in refusals:
        return ""
    return ("sqlc-gen-spira: ContinueItem: writes versioned table items without "
            + base.text(case.get("option", "version")))


def main() -> None:
    manifest = base.object_value(json.loads((base.PROOF / "manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof file hash differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r41.yml"
    if base.digest(active.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    contracts = base.object_value(manifest["contracts"])
    os.environ["GOWORK"] = "off"
    report: dict[str, object] = {
        "schema": 1, "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "manifest": manifest, "manifest_sha256": base.digest((base.PROOF / "manifest.json").read_bytes()),
        "observations_complete": False, "source_restored": False,
        "baseline_verified": False, "source_gates_passed": False, "publication_eligible": False,
    }
    created: set[Path] = set()
    try:
        head = base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip()
        if head != os.environ["GITHUB_SHA"]:
            raise RuntimeError("proof producer differs")
        if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
            raise RuntimeError("unchanged source head differs")
        report["before"] = proof.source_snapshot(manifest, "source")
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache remained active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run(
            "sqlc-module", ["go", "-C", str(base.SOURCE / "tools"), "list", "-m", "-json",
                            "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned SQLc differs")
        catalog = observer.rows_from("version-catalog.json", 10)
        ctes = observer.rows_from("version-ctes.json", 24)
        cases = catalog + ctes
        if len({base.text(row["name"]) for row in cases}) != 34:
            raise RuntimeError("combined case identities differ")
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            native_rows: list[dict[str, object]] = []
            report["native_original"] = native_rows
            for case in cases:
                native_rows.append(observer.observe_native(container, case, base.text(case["sql"]), "original"))
            apply_tests(authored_rows(), created)
            report["tests_before"] = proof.source_snapshot(manifest, "tests")
            base.run("unchanged-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
            report["catalog_red"] = suite(
                "catalog-unchanged-red", "TestPostgresVersionCatalogResolution",
                {base.text(case["name"]) for case in catalog}, observer.names(contracts["catalog_failed"]))
            report["cte_red"] = suite(
                "cte-unchanged-red", "TestPostgresVersionCTEs",
                {base.text(case["name"]) for case in ctes}, observer.names(contracts["cte_failed"]))
            plugin = base.BUILD / "unchanged-plugin"
            base.run("build-unchanged-plugin", ["logos", "build", "-o", str(plugin),
                                               "./cmd/sqlc-gen-spira"], timeout=900)
            refusals = observer.names(contracts["catalog_pipeline_refusal"]) | observer.names(contracts["cte_pipeline_refusal"])
            pipeline: list[dict[str, object]] = []
            report["pipeline"] = pipeline
            for case in cases:
                pipeline.append(observer.observe_pipeline(
                    container, case, plugin, "unchanged", baseline_diagnostic(case, refusals)))
                (base.OUTPUT / "pipeline.json").write_text(json.dumps(pipeline, indent=2) + "\n", encoding="utf-8")
        names = observer.names(contracts["pipeline_names"])
        if len(names) != 60:
            raise RuntimeError("permanent pipeline identity set differs")
        report["current_pipeline"] = suite(
            "current-permanent-pipeline", "TestSQLcPostgresLiteralPipeline", names, set(), timeout=1800)
        base.run("current-retained-roles", ["logos", "test", "-race", "-count=1", "-v", "-run",
            "^TestPostgresVersionReference(Roles|RoleBoundaries)$", "./cmd/sqlc-gen-spira"], timeout=900)
        report["tests_after"] = proof.source_snapshot(manifest, "tests")
        if report["tests_before"] != report["tests_after"]:
            raise RuntimeError("unchanged production/test tree moved")
        model = base.SOURCE / "cmd/sqlc-gen-spira/model.lgo"
        model.write_bytes((base.PROOF / "model-before-r40.lgo").read_bytes())
        base.run("stage-counterfactual-model", ["git", "add", "--", "cmd/sqlc-gen-spira/model.lgo"])
        report["counterfactual_before"] = proof.source_snapshot(manifest, "counterfactual")
        base.run("counterfactual-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
        report["counterfactual_pipeline"] = suite(
            "counterfactual-permanent-pipeline-red", "TestSQLcPostgresLiteralPipeline",
            names, observer.names(contracts["counterfactual_failed"]), timeout=1800)
        report["counterfactual_after"] = proof.source_snapshot(manifest, "counterfactual")
        if report["counterfactual_before"] != report["counterfactual_after"]:
            raise RuntimeError("counterfactual source/test tree moved")
        base.run("restore-current-model", ["git", "restore", "--source", base.text(source["head"]),
                                          "--staged", "--worktree", "--", "cmd/sqlc-gen-spira/model.lgo"])
        report["current_model_restored"] = proof.source_snapshot(manifest, "tests")
        if report["current_model_restored"] != report["tests_before"]:
            raise RuntimeError("current model restoration differs")
        report["observations_complete"] = True
    finally:
        try:
            base.run("restore-original-source", ["git", "restore", "--source", base.text(source["head"]),
                                                "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored"] = proof.source_snapshot(manifest, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            if base.run("untracked-restoration", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
                raise RuntimeError("unexpected untracked source after restoration")
            report["source_restored"] = True
        finally:
            report["baseline_verified"] = bool(report["observations_complete"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R41_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
