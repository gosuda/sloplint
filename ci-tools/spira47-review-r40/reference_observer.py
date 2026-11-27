"""Observe exact native SQL, real plugin outcomes and nested compiler arguments."""
from __future__ import annotations

import json
import os
import re
import shlex
import shutil
import sys
from pathlib import Path

import baseline_support as base
import native_support as native


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


def observe_pipeline(container: str, case: dict[str, object], plugin: Path,
                     lane: str, contract: str) -> dict[str, object]:
    name, sql = base.text(case["name"]), base.text(case["sql"])
    if re.fullmatch(r"[a-z0-9-]+", name) is None:
        raise ValueError("invalid frozen case name")
    messages = {
        "version-refusal": "sqlc-gen-spira: ContinueItem: writes versioned table items without version",
        "unnamed-column": "sqlc-gen-spira: ContinueItem: column 0 needs an alias",
    }
    if contract != "generated" and contract not in messages:
        raise ValueError("unknown frozen pipeline contract")
    expected_exit = 0 if contract == "generated" else 1
    work = base.BUILD / "reference-pipeline" / lane / name
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
    output = base.run(lane + "-sqlc-" + name,
                      ["go", "-C", str(base.SOURCE / "tools"), "tool", "sqlc", "generate",
                       "-f", str(work / "sqlc.yaml")], expected=expected_exit, timeout=300)
    generated = sorted((work / "generated").glob("*.lgo"))
    expected_files = ["models.sql.lgo", "queries.sql.lgo"] if expected_exit == 0 else []
    if [path.name for path in generated] != expected_files:
        raise RuntimeError("actual generated file set differs: " + name)
    if expected_exit != 0 and messages[contract] not in output:
        raise RuntimeError("pipeline rejected for a different reason: " + name)
    captured = base.object_value(json.loads(request.read_bytes()))
    queries = captured["queries"]
    if not isinstance(queries, list) or len(queries) != 1:
        raise ValueError("expected one actual plugin query")
    query = base.object_value(queries[0])
    if query["name"] != "ContinueItem" or query["cmd"] != ":exec" or query["filename"] != "queries.sql":
        raise RuntimeError("actual plugin query identity differs")
    if contract == "unnamed-column":
        columns = query["columns"]
        if not isinstance(columns, list) or len(columns) != 1 or base.object_value(columns[0])["name"] != "":
            raise RuntimeError("unnamed output-column metadata did not reproduce")
    processed = base.text(query["text"])
    row: dict[str, object] = {
        "name": name, "lane": lane, "sql": sql, "sql_sha256": base.digest(sql.encode()),
        "raw_exit": base.RECEIPTS[-1].raw_exit,
        "reference_contract_allows": case["allow"], "pipeline_contract": contract,
        "observed_generator_accepts": expected_exit == 0,
        "request_sha256": base.digest(request.read_bytes()),
        "processed_sql": processed, "processed_sql_sha256": base.digest(processed.encode()),
        "query_metadata": query, "log_sha256": base.digest(output.encode()),
        "files": [{"path": path.name, "sha256": base.digest(path.read_bytes()),
                   "content": path.read_text(encoding="utf-8")} for path in generated],
        "processed_native": observe_native(container, case, processed, lane + "-processed"),
    }
    (base.OUTPUT / (lane + "-" + name + "-request.json")).write_bytes(request.read_bytes())
    print("VERSION_REFERENCE_PIPELINE " + json.dumps(row), flush=True)
    return row


def install_argv_observer() -> tuple[Path, Path, str]:
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
    launcher = observer_dir / "launch-test"
    launcher.write_text("#!" + sys.executable + "\n"
                        + (base.PROOF / "launch_test.py").read_text(encoding="utf-8"),
                        encoding="utf-8")
    launcher.chmod(0o755)
    old_path = os.environ["PATH"]
    os.environ["SPIRA_ARGV_OBSERVER_DIR"] = str(observer_dir)
    os.environ["SPIRA_TEST_PROCESS_ARGV"] = str(base.OUTPUT / "test-process-argv.jsonl")
    os.environ["SPIRA_REAL_LOGOS"] = str(real)
    os.environ["SPIRA_LOGOS_ARGV"] = str(base.OUTPUT / "logos-argv.jsonl")
    os.environ["PATH"] = str(observer_dir) + os.pathsep + old_path
    return real, launcher, old_path


def nested_race_records() -> list[dict[str, object]]:
    rows = [base.object_value(json.loads(line))
            for line in (base.OUTPUT / "logos-argv.jsonl").read_bytes().splitlines()]
    for row in rows:
        print("CURRENT_OBSERVER_RAW_ARGV " + json.dumps(row), flush=True)
    selected = [row for row in rows if row["module"] == "module example.com/spira-sqlc-runtime"
                and isinstance(row["argv"], list) and row["argv"][:1] == ["test"]]
    if len(selected) != 2 or len({base.text(row["cwd"]) for row in selected}) != 2:
        raise RuntimeError("did not observe the two distinct nested generated-package test processes")
    if any(row["argv"] != ["test", "-race", "-count=1", "-v", "."] for row in selected):
        raise RuntimeError("candidate nested test arguments differ")
    launchers = [base.object_value(json.loads(line))
                 for line in (base.OUTPUT / "test-process-argv.jsonl").read_bytes().splitlines()]
    if len(launchers) != 2:
        raise RuntimeError("did not observe both actual outer test binaries")
    for row in launchers:
        print("OUTER_TEST_PROCESS_ARGV " + json.dumps(row), flush=True)
        if row["incoming_first_path"] != str(Path(os.environ["GOROOT"]) / "bin"):
            raise RuntimeError("native test PATH did not match the pinned runner prepend")
    for row in selected:
        print("NESTED_QUERY_TEST_ARGV " + json.dumps(row), flush=True)
    return selected


