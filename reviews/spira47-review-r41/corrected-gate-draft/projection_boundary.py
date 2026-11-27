"""Freeze two SELECT clause boundaries before applying the corrected production source."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import baseline_support as base
import native_support as native
import observer
import proof_support as proof


def source_rows(name: str, count: int) -> list[dict[str, object]]:
    raw: object = json.loads((base.PROOF / name).read_bytes())
    if not isinstance(raw, list) or len(raw) != count:
        raise ValueError("wrong boundary source set")
    rows = [base.object_value(row) for row in raw]
    if len({base.text(row["path"]) for row in rows}) != count:
        raise ValueError("duplicate boundary source path")
    for row in rows:
        relative = Path(base.text(row["path"]))
        if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
            raise ValueError("invalid boundary source path")
        data = base.text(row["content"]).encode()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if base.digest(data) != row["sha256"] or blob != row["blob"]:
            raise RuntimeError("boundary source identity differs: " + str(relative))
    return rows


def apply(rows: list[dict[str, object]], created: set[Path], lane: str) -> None:
    for row in rows:
        path = base.SOURCE / base.text(row["path"])
        if not path.exists():
            created.add(path)
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-boundary-" + lane, ["git", "add", "--"] + [base.text(row["path"]) for row in rows])


def expected_red(lane: str, names: set[str], failed: set[str]) -> dict[str, list[str]]:
    test = "TestPostgresCTEProjectionClauseBoundaries"
    output = base.run("boundary-" + lane + "-red", [
        "logos", "test", "-race", "-count=1", "-v", "-run", "^" + test + "$",
        "./cmd/sqlc-gen-spira",
    ], expected=1, timeout=900)
    actual = proof.test_outcomes(output, test, names)
    if set(actual["failed"]) != failed or set(actual["passed"]) != names - failed:
        raise RuntimeError("boundary RED/control partition differs: " + lane)
    return actual


def pipeline(container: str, cases: list[dict[str, object]], lane: str,
             refusals: set[str]) -> list[dict[str, object]]:
    plugin = base.BUILD / ("boundary-" + lane + "-plugin")
    base.run("build-boundary-" + lane + "-plugin", ["logos", "build", "-o", str(plugin),
                                                  "./cmd/sqlc-gen-spira"], timeout=900)
    rows: list[dict[str, object]] = []
    for case in cases:
        diagnostic = ("sqlc-gen-spira: ContinueItem: writes versioned table items without version"
                      if base.text(case["name"]) in refusals else "")
        rows.append(observer.observe_pipeline(container, case, plugin, "boundary-" + lane, diagnostic))
    return rows


def compare_handoff(previous: dict[str, object], current: dict[str, object]) -> None:
    for key in ("name", "sql", "sql_sha256", "option", "columns", "processed_sql",
                "processed_sql_sha256", "catalog_sha256", "catalog_canonical_bytes", "query_metadata"):
        if previous[key] != current[key]:
            raise RuntimeError("boundary SQLc handoff differs: " + base.text(current["name"]) + ": " + key)
    old_native = base.object_value(previous["processed_native"])
    new_native = base.object_value(current["processed_native"])
    for key in ("sql", "sql_sha256", "observed", "expected", "matches", "raw_output"):
        if old_native[key] != new_native[key]:
            raise RuntimeError("boundary processed native state differs")


def observe(manifest: dict[str, object], report: dict[str, object], created: set[Path]
            ) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    cases = observer.rows_from("version-projection-clauses.json", 2)
    names = {base.text(case["name"]) for case in cases}
    tests = source_rows("boundary-tests.json", 11)
    proposed = source_rows("boundary-proposed.json", 15)
    proposed_paths = {base.text(row["path"]): row for row in proposed}
    for row in tests:
        if proposed_paths[base.text(row["path"])] != row:
            raise RuntimeError("rejected proposal changed a boundary test")
    with native.postgres(manifest) as (container, version):
        report["boundary_postgres_version_num"] = version
        native_rows: list[dict[str, object]] = []
        report["boundary_native_original"] = native_rows
        for case in cases:
            native_rows.append(observer.observe_native(container, case, base.text(case["sql"]),
                                                       "boundary-original"))
        apply(tests, created, "unchanged")
        report["boundary_tests_before"] = proof.source_snapshot(manifest, "boundary_tests")
        base.run("boundary-unchanged-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
        report["boundary_unchanged_red"] = expected_red(
            "unchanged", names, {"cte-select-not-distinct-from-reference"})
        unchanged = pipeline(container, cases, "unchanged", names)
        report["boundary_unchanged_pipeline"] = unchanged
        report["boundary_tests_after"] = proof.source_snapshot(manifest, "boundary_tests")
        if report["boundary_tests_before"] != report["boundary_tests_after"]:
            raise RuntimeError("unchanged boundary source moved")
        apply(proposed, created, "rejected-proposal")
        report["boundary_proposed_before"] = proof.source_snapshot(manifest, "boundary_proposed")
        base.run("boundary-proposed-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                              "./cmd/sqlc-gen-spira"], timeout=1200)
        report["boundary_proposed_red"] = expected_red(
            "proposed", names, {"cte-select-distinct-from-alias"})
        proposed_pipeline = pipeline(container, cases, "proposed", set())
        report["boundary_proposed_pipeline"] = proposed_pipeline
        for previous, current in zip(unchanged, proposed_pipeline, strict=True):
            compare_handoff(previous, current)
        report["boundary_proposed_after"] = proof.source_snapshot(manifest, "boundary_proposed")
        if report["boundary_proposed_before"] != report["boundary_proposed_after"]:
            raise RuntimeError("rejected boundary source moved")
    source = base.object_value(manifest["source"])
    base.run("boundary-restore-source", ["git", "restore", "--source", base.text(source["head"]),
                                        "--staged", "--worktree", "--", "."])
    for path in created:
        path.unlink(missing_ok=True)
    report["boundary_restored"] = proof.source_snapshot(manifest, "source")
    base.run("boundary-restoration-diff", ["git", "diff", "--exit-code", "HEAD"])
    if base.run("boundary-restoration-untracked", ["git", "ls-files", "--others",
                                                  "--exclude-standard"]).strip():
        raise RuntimeError("unexpected untracked boundary source")
    if report["boundary_restored"] != report["before"]:
        raise RuntimeError("boundary source restoration differs")
    report["boundary_observations_complete"] = True
    return native_rows, unchanged
