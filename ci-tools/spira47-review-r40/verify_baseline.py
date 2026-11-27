"""Observe the frozen version-reference and nested-race defects on unchanged production."""
from __future__ import annotations

import hashlib
import json
import os
import re
import shlex
import shutil
import sys
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import native_support as native
import proof_support as proof


def rows_from(name: str, count: int) -> list[dict[str, object]]:
    value: object = json.loads((base.PROOF / name).read_bytes())
    if not isinstance(value, list) or len(value) != count:
        raise ValueError("frozen row count differs: " + name)
    rows = [base.object_value(row) for row in value]
    if len({base.text(row["name"]) for row in rows}) != count:
        raise ValueError("duplicate frozen case name")
    return rows


def observe_native(container: str, case: dict[str, object], sql: str, lane: str) -> dict[str, object]:
    name = base.text(case["name"])
    script = ("BEGIN; SET LOCAL standard_conforming_strings = on;\n"
              "TRUNCATE items; INSERT INTO items (id, name, version) VALUES (1, 'before', 0);\n"
              + sql + "\n;\nSELECT json_build_object('name', name, 'version', version) FROM items;\nROLLBACK;")
    output = native.psql(container, lane + "-" + name, script)
    lines = output.strip().splitlines()
    if not lines:
        raise RuntimeError("native query omitted its final state")
    observed = base.object_value(json.loads(lines[-1]))
    expected = {"name": case["native_name"], "version": case["native_version"]}
    row: dict[str, object] = {"name": name, "lane": lane, "sql": sql,
                             "sql_sha256": base.digest(sql.encode()), "raw_output": output,
                             "observed": observed, "expected": expected, "matches": observed == expected}
    print("VERSION_REFERENCE_NATIVE " + json.dumps(row), flush=True)
    if observed != expected:
        raise RuntimeError("native outcome differs: " + name)
    return row


def observe_pipeline(container: str, case: dict[str, object], plugin: Path) -> dict[str, object]:
    name, sql = base.text(case["name"]), base.text(case["sql"])
    if re.fullmatch(r"[a-z0-9-]+", name) is None:
        raise ValueError("invalid frozen case name")
    work = base.BUILD / "reference-pipeline" / name
    work.mkdir(parents=True, exist_ok=False)
    request = work / "request.json"
    wrapper = work / "capture-plugin.sh"
    wrapper.write_text("#!/usr/bin/env bash\nset -euo pipefail\ntee "
                       + shlex.quote(str(request)) + " | " + shlex.quote(str(plugin))
                       + ' "$@"\n', encoding="utf-8")
    wrapper.chmod(0o755)
    (work / "schema.sql").write_bytes((base.PROOF / "schema.sql").read_bytes())
    (work / "queries.sql").write_bytes(("-- name: ContinueItem :exec\n" + sql + "\n").encode())
    config = ("version: \"2\"\nplugins:\n  - name: spira\n    process:\n"
              "      cmd: " + json.dumps(str(wrapper)) + "\n      format: json\n"
              "sql:\n  - engine: postgresql\n    schema: schema.sql\n    queries: queries.sql\n"
              "    codegen:\n      - plugin: spira\n        out: generated\n"
              "        options:\n          package: generated\n          tables:\n"
              "            items: {type: Item, key: id, key_type: ItemID, version: version}\n")
    (work / "sqlc.yaml").write_text(config, encoding="utf-8")
    output = base.run("reference-sqlc-" + name,
                      ["go", "-C", str(base.SOURCE / "tools"), "tool", "sqlc", "generate",
                       "-f", str(work / "sqlc.yaml")], expected=0, timeout=300)
    generated = sorted((work / "generated").glob("*.lgo"))
    if [path.name for path in generated] != ["models.sql.lgo", "queries.sql.lgo"]:
        raise RuntimeError("baseline generated file set differs: " + name)
    captured = base.object_value(json.loads(request.read_bytes()))
    queries = captured["queries"]
    if not isinstance(queries, list) or len(queries) != 1:
        raise ValueError("expected one actual plugin query")
    query = base.object_value(queries[0])
    processed = base.text(query["text"])
    row: dict[str, object] = {
        "name": name, "sql": sql, "sql_sha256": base.digest(sql.encode()),
        "raw_exit": base.RECEIPTS[-1].raw_exit,
        "expected_generator_accepts": case["allow"], "observed_generator_accepts": True,
        "correct": case["allow"] is True, "request_sha256": base.digest(request.read_bytes()),
        "processed_sql": processed, "processed_sql_sha256": base.digest(processed.encode()),
        "query_metadata": query, "log_sha256": base.digest(output.encode()),
        "files": [{"path": path.name, "sha256": base.digest(path.read_bytes()),
                   "content": path.read_text(encoding="utf-8")} for path in generated],
        "processed_native": observe_native(container, case, processed, "processed"),
    }
    (base.OUTPUT / (name + "-request.json")).write_bytes(request.read_bytes())
    print("VERSION_REFERENCE_PIPELINE " + json.dumps(row), flush=True)
    return row


def install_argv_observer() -> tuple[Path, str]:
    found = shutil.which("logos")
    if found is None:
        raise RuntimeError("real logos compiler is missing")
    real = Path(found).resolve(strict=True)
    observer_dir = base.BUILD / "argv-observer"
    observer_dir.mkdir(exist_ok=False)
    wrapper = observer_dir / "logos"
    wrapper.write_text("#!" + sys.executable + "\n"
                       + (base.PROOF / "capture_logos.py").read_text(encoding="utf-8"),
                       encoding="utf-8")
    wrapper.chmod(0o755)
    old_path = os.environ["PATH"]
    os.environ["SPIRA_REAL_LOGOS"] = str(real)
    os.environ["SPIRA_LOGOS_ARGV"] = str(base.OUTPUT / "logos-argv.jsonl")
    os.environ["PATH"] = str(observer_dir) + os.pathsep + old_path
    return real, old_path


def nested_race_records() -> list[dict[str, object]]:
    rows = [base.object_value(json.loads(line))
            for line in (base.OUTPUT / "logos-argv.jsonl").read_bytes().splitlines()]
    selected = [row for row in rows if row["module"] == "module example.com/spira-sqlc-runtime"
                and isinstance(row["argv"], list) and row["argv"][:1] == ["test"]]
    if len(selected) != 2 or len({base.text(row["cwd"]) for row in selected}) != 2:
        raise RuntimeError("did not observe the two distinct nested generated-package test processes")
    if any(row["argv"] != ["test", "-count=1", "-v", "."] for row in selected):
        raise RuntimeError("unchanged nested test arguments differ")
    for row in selected:
        print("NESTED_QUERY_TEST_ARGV " + json.dumps(row), flush=True)
    return selected


def main() -> None:
    manifest = base.object_value(json.loads((base.PROOF / "manifest.json").read_bytes()))
    for name, wanted in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("proof digest differs: " + name)
    active = base.PROOF.parent.parent / ".github/workflows/spira47-review-r40.yml"
    if base.digest(active.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    producer = base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip()
    if producer != os.environ["GITHUB_SHA"]:
        raise RuntimeError("producer checkout differs from the workflow source")
    os.environ["GOWORK"] = "off"
    if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
        raise RuntimeError("wrong unchanged source")
    report: dict[str, object] = {"schema": 1, "producer": os.environ["GITHUB_SHA"],
                                "run": os.environ["GITHUB_RUN_ID"], "manifest": manifest,
                                "baseline_verified": False, "observations_complete": False,
                                "source_restored": False, "publication_eligible": False}
    created: set[Path] = set()
    original_path = os.environ["PATH"]
    try:
        report["before"] = proof.source_snapshot(manifest, "source")
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run("sqlc-module", ["go", "-C", str(base.SOURCE / "tools"),
                                                     "list", "-m", "-json", "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("source-pinned sqlc differs")
        cases = rows_from("version-references.json", 20)
        with native.postgres(manifest) as (container, version):
            report["postgres_version_num"] = version
            report["native_original"] = [observe_native(container, case, base.text(case["sql"]), "original")
                                         for case in cases]
            authored: object = json.loads((base.PROOF / "authored-tests.json").read_bytes())
            if not isinstance(authored, list) or len(authored) != 3:
                raise ValueError("wrong authored test set")
            for raw in authored:
                row = base.object_value(raw)
                relative = Path(base.text(row["path"]))
                if relative.is_absolute() or ".." in relative.parts or row["mode"] != "100644":
                    raise ValueError("invalid test path")
                data = base.text(row["content"]).encode()
                blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
                if base.digest(data) != row["sha256"] or blob != row["blob"]:
                    raise RuntimeError("authored test digest differs")
                path = base.SOURCE / relative
                if not path.exists():
                    created.add(path)
                path.write_bytes(data)
            base.run("stage-frozen-tests", ["git", "add", "--"] +
                     [base.text(base.object_value(row)["path"]) for row in authored])
            report["tests_before"] = proof.source_snapshot(manifest, "tests")
            base.run("reference-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
            output = base.run("version-reference-unchanged-red",
                ["logos", "test", "-race", "-count=1", "-v", "-run", "^TestPostgresVersionReferenceRoles$",
                 "./cmd/sqlc-gen-spira"], expected=1, timeout=900)
            outcomes = proof.test_outcomes(output, "TestPostgresVersionReferenceRoles",
                                           {base.text(case["name"]) for case in cases})
            failed = {base.text(case["name"]) for case in cases if case["allow"] is False}
            if set(outcomes["failed"]) != failed or len(outcomes["passed"]) != 8:
                raise RuntimeError("baseline reference RED/control partition differs")
            report["generator"] = outcomes
            plugin = base.BUILD / "unchanged-plugin"
            base.run("build-unchanged-plugin", ["logos", "build", "-o", str(plugin),
                                               "./cmd/sqlc-gen-spira"], timeout=900)
            pipeline = []
            for case in cases:
                pipeline.append(observe_pipeline(container, case, plugin))
                (base.OUTPUT / "pipeline-observations.json").write_text(json.dumps(pipeline, indent=2) + "\n",
                                                                       encoding="utf-8")
            report["pipeline"] = pipeline
        real, original_path = install_argv_observer()
        report["real_logos"] = {"path": str(real), "sha256": base.digest(real.read_bytes())}
        for name, test in (("retained-direct-runtime", "TestGeneratedQueryConstants"),
                           ("retained-pipeline-runtime", "TestSQLcPostgresLiteralPipeline")):
            output = base.run(name, ["logos", "test", "-race", "-count=1", "-v", "-run", "^" + test + "$",
                                     "./cmd/sqlc-gen-spira"], timeout=1200)
            if re.search(r"^\s*--- PASS: " + re.escape(test) + r" ", output, re.MULTILINE) is None:
                raise RuntimeError("retained runtime suite did not pass: " + test)
        report["nested_test_argv"] = nested_race_records()
        os.environ["PATH"] = original_path
        base.run("retained-generator-controls", ["logos", "test", "-race", "-count=1", "-v", "-run",
                 "^(TestPostgresDollarQuotedBoundaries|TestPostgresTypedLiteralBoundaries|TestPostgresContinuedEscapeStrings|TestVersionedWrite)",
                 "./cmd/sqlc-gen-spira"], timeout=1200)
        report["tests_after"] = proof.source_snapshot(manifest, "tests")
        if report["tests_before"] != report["tests_after"]:
            raise RuntimeError("unchanged production/test snapshot moved")
        report["observations_complete"] = True
    finally:
        os.environ["PATH"] = original_path
        try:
            base.run("restore-original-source", ["git", "restore", "--source", base.text(source["head"]),
                                                "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored"] = proof.source_snapshot(manifest, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            if base.run("untracked-restoration", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
                raise RuntimeError("unexpected untracked source remains")
            report["source_restored"] = True
        finally:
            report["baseline_verified"] = bool(report["observations_complete"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R40_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
