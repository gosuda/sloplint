"""Observe the introduced SELECT-alias boundary on unchanged and frozen proposed source."""
from __future__ import annotations

import hashlib
import json
import os
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import native_support as native
import observer
import proof_support as proof
from verify_baseline import suite


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


def compare_handoff(previous: dict[str, object], current: dict[str, object]) -> None:
    for key in ("name", "sql", "sql_sha256", "option", "columns", "processed_sql",
                "processed_sql_sha256", "catalog_sha256", "catalog_canonical_bytes", "query_metadata"):
        if previous[key] != current[key]:
            raise RuntimeError("actual SQLc handoff differs: " + base.text(current["name"]) + ": " + key)
    prior_native = base.object_value(previous["processed_native"])
    current_native = base.object_value(current["processed_native"])
    for key in ("sql", "sql_sha256", "observed", "expected", "matches", "raw_output"):
        if prior_native[key] != current_native[key]:
            raise RuntimeError("processed native state differs")


def pipeline(container: str, cases: list[dict[str, object]], lane: str,
             refusals: set[str]) -> list[dict[str, object]]:
    plugin = base.BUILD / (lane + "-plugin")
    base.run("build-" + lane + "-plugin", ["logos", "build", "-o", str(plugin),
                                          "./cmd/sqlc-gen-spira"], timeout=900)
    rows: list[dict[str, object]] = []
    for case in cases:
        diagnostic = ("sqlc-gen-spira: ContinueItem: writes versioned table items without version"
                      if base.text(case["name"]) in refusals else "")
        rows.append(observer.observe_pipeline(container, case, plugin, lane, diagnostic))
        (base.OUTPUT / (lane + "-pipeline.json")).write_text(json.dumps(rows, indent=2) + "\n",
                                                           encoding="utf-8")
    return rows


def main() -> None:
    manifest = base.object_value(json.loads((base.PROOF / "manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof hash differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r41.yml"
    if base.digest(active.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    contracts = base.object_value(manifest["contracts"])
    report: dict[str, object] = {
        "schema": 1, "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "manifest": manifest, "manifest_sha256": base.digest((base.PROOF / "manifest.json").read_bytes()),
        "observations_complete": False, "source_restored": False,
        "baseline_verified": False, "source_gates_passed": False, "publication_eligible": False,
    }
    created: set[Path] = set()
    os.environ["GOWORK"] = "off"
    try:
        producer = base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip()
        if producer != os.environ["GITHUB_SHA"]:
            raise RuntimeError("producer identity differs")
        if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
            raise RuntimeError("unchanged source differs")
        report["before"] = proof.source_snapshot(manifest, "source")
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache remained active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run("sqlc-module", ["go", "-C", str(base.SOURCE / "tools"),
                                                     "list", "-m", "-json", "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned SQLc differs")
        cases = observer.rows_from("version-projections.json", 10)
        names = {base.text(case["name"]) for case in cases}
        tests = authored("authored-tests.json", 9)
        proposed = authored("authored-proposed.json", 13)
        proposed_by_path = {base.text(row["path"]): row for row in proposed}
        for row in tests:
            if proposed_by_path[base.text(row["path"])] != row:
                raise RuntimeError("proposal changed a frozen test")
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            native_rows: list[dict[str, object]] = []
            report["native_original"] = native_rows
            for case in cases:
                native_rows.append(observer.observe_native(container, case, base.text(case["sql"]), "original"))
            apply(tests, created, "unchanged-tests")
            report["tests_before"] = proof.source_snapshot(manifest, "tests")
            base.run("unchanged-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
            report["unchanged_projection"] = suite(
                "unchanged-projection-controls", "TestPostgresCTEProjectionRoles",
                names, observer.names(contracts["unchanged_failed"]))
            unchanged_pipeline = pipeline(container, cases, "unchanged", names)
            report["unchanged_pipeline"] = unchanged_pipeline
            report["tests_after"] = proof.source_snapshot(manifest, "tests")
            if report["tests_before"] != report["tests_after"]:
                raise RuntimeError("unchanged production/test source moved")
            apply(proposed, created, "frozen-proposal")
            report["proposed_before"] = proof.source_snapshot(manifest, "proposed")
            base.run("proposed-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                              "./cmd/sqlc-gen-spira"], timeout=1200)
            report["proposed_projection"] = suite(
                "proposed-projection-regression", "TestPostgresCTEProjectionRoles",
                names, observer.names(contracts["proposed_failed"]))
            proposed_pipeline = pipeline(
                container, cases, "proposed", observer.names(contracts["proposed_pipeline_refusals"]))
            report["proposed_pipeline"] = proposed_pipeline
            for lane, rows in (("unchanged", unchanged_pipeline), ("proposed", proposed_pipeline)):
                partition = {kind: sum(row["pipeline_contract"] == kind for row in rows)
                             for kind in ("generated", "plugin-refusal")}
                if partition != contracts[lane + "_pipeline_partition"]:
                    raise RuntimeError("actual pipeline partition differs: " + lane)
                report[lane + "_pipeline_partition"] = partition
            for previous, current in zip(unchanged_pipeline, proposed_pipeline, strict=True):
                compare_handoff(base.object_value(previous), base.object_value(current))
            report["proposed_after"] = proof.source_snapshot(manifest, "proposed")
            if report["proposed_before"] != report["proposed_after"]:
                raise RuntimeError("frozen proposed source moved")
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
                raise RuntimeError("unexpected untracked restored source")
            report["source_restored"] = True
        finally:
            report["baseline_verified"] = bool(report["observations_complete"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R41_SELECT_ALIAS_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
