"""Freeze native, current, and one-file prior reference-role outcomes."""
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

def authored(name: str, count: int) -> list[dict[str, object]]:
    value: object = json.loads((base.PROOF / name).read_bytes())
    if not isinstance(value, list) or len(value) != count:
        raise ValueError("wrong authored source path set")
    rows = [base.object_value(row) for row in value]
    if len({base.text(row["path"]) for row in rows}) != count:
        raise ValueError("duplicate authored path")
    for row in rows:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise ValueError("invalid authored path")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("authored identity differs: " + str(relative))
    return rows


def apply(rows: list[dict[str, object]], created: set[Path], lane: str) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-" + lane, ["git", "add", "--"] + [base.text(row["path"]) for row in rows])


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



def main() -> None:
    manifest = base.object_value(json.loads((base.PROOF / "manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof file hash differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r42.yml"
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
        cases = observer.rows_from("version-relation-roles.json", 30)
        names = {base.text(row["name"]) for row in cases}
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            native_rows: list[dict[str, object]] = []
            report["native_original"] = native_rows
            for case in cases:
                native_rows.append(observer.observe_native(container, case, base.text(case["sql"]), "original"))
            apply(authored("authored-tests.json", 2), created, "unchanged-tests")
            report["tests_before"] = proof.source_snapshot(manifest, "tests")
            base.run("unchanged-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
            report["current_roles"] = suite(
                "current-relation-roles", "TestPostgresRelationReferenceRoles",
                names, observer.names(contracts["current_failed"]))
            plugin = base.BUILD / "unchanged-plugin"
            base.run("build-unchanged-plugin", ["logos", "build", "-o", str(plugin),
                                               "./cmd/sqlc-gen-spira"], timeout=900)
            refusals = observer.names(contracts["current_pipeline_refusals"])
            upstream = base.object_value(contracts["sqlc_upstream"])
            if set(upstream) != {"relation-column-list", "relation-column-list-explicit"}:
                raise RuntimeError("frozen upstream identities differ")
            pipeline: list[dict[str, object]] = []
            report["pipeline"] = pipeline
            for case in cases:
                name = base.text(case["name"])
                diagnostic = (base.text(upstream[name]) if name in upstream else
                              "sqlc-gen-spira: ContinueItem: writes versioned table items without version"
                              if name in refusals else "")
                pipeline.append(observer.observe_pipeline(container, case, plugin, "unchanged", diagnostic,
                                                          upstream=name in upstream))
                (base.OUTPUT / "pipeline.json").write_text(json.dumps(pipeline, indent=2) + "\n", encoding="utf-8")
            partition = {kind: sum(row["pipeline_contract"] == kind for row in pipeline)
                         for kind in ("generated", "plugin-refusal", "upstream")}
            if partition != contracts["current_pipeline_partition"]:
                raise RuntimeError("complete actual pipeline partition differs")
            report["pipeline_partition"] = partition
        report["tests_after"] = proof.source_snapshot(manifest, "tests")
        if report["tests_before"] != report["tests_after"]:
            raise RuntimeError("unchanged production/test tree moved")
        reference = base.SOURCE / "cmd/sqlc-gen-spira/version_reference.lgo"
        reference.write_bytes((base.PROOF / "reference-before-r41.lgo").read_bytes())
        base.run("stage-prior-reference", ["git", "add", "--", "cmd/sqlc-gen-spira/version_reference.lgo"])
        report["counterfactual_before"] = proof.source_snapshot(manifest, "counterfactual")
        base.run("counterfactual-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
        report["counterfactual_roles"] = suite(
            "prior-reference-relation-roles", "TestPostgresRelationReferenceRoles",
            names, observer.names(contracts["counterfactual_failed"]))
        report["counterfactual_after"] = proof.source_snapshot(manifest, "counterfactual")
        if report["counterfactual_before"] != report["counterfactual_after"]:
            raise RuntimeError("counterfactual source/test tree moved")
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
            print("SPIRA_R42_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
