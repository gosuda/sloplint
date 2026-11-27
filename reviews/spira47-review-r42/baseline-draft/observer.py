"""Observe exact native statements and the real SQLc catalog/query handoff."""
from __future__ import annotations

import json
import re
import shlex
from pathlib import Path

import baseline_support as base
import native_support as native


def rows_from(name: str, count: int) -> list[dict[str, object]]:
    raw: object = json.loads((base.PROOF / name).read_bytes())
    if not isinstance(raw, list) or len(raw) != count:
        raise ValueError("frozen case count differs: " + name)
    rows = [base.object_value(row) for row in raw]
    if len({base.text(row["name"]) for row in rows}) != count:
        raise ValueError("duplicate frozen case")
    return rows


def names(value: object) -> set[str]:
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        raise ValueError("expected a frozen string array")
    result = {base.text(item) for item in value}
    if len(result) != len(value):
        raise ValueError("duplicate frozen name")
    return result


def columns(case: dict[str, object]) -> list[str]:
    value = case.get("columns", ["version"])
    if not isinstance(value, list) or not value or any(not isinstance(item, str) for item in value):
        raise ValueError("invalid catalog columns")
    return [base.text(item) for item in value]


def quote_identifier(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def table_schema(case: dict[str, object], temporary: bool) -> str:
    names = columns(case)
    prefix = "CREATE TEMP TABLE" if temporary else "CREATE TABLE"
    parts = ["id bigint PRIMARY KEY", "name text NOT NULL"]
    parts.extend(quote_identifier(name) + " integer NOT NULL DEFAULT 0" for name in names)
    return prefix + " items (" + ", ".join(parts) + ");"


def observe_native(container: str, case: dict[str, object], sql: str, lane: str) -> dict[str, object]:
    name = base.text(case["name"])
    names = columns(case)
    projections = ", ".join(quote_identifier(item) for item in names)
    values = ", ".join(["1", "'before'"] + ["0"] * len(names))
    script = ("BEGIN; SET LOCAL standard_conforming_strings = on;\n"
              + table_schema(case, False) + "\nINSERT INTO items VALUES (" + values + ");\n"
              + sql + "\n;\nSELECT json_build_object('name', name, 'versions', ARRAY["
              + projections + "]) FROM items;\nROLLBACK;")
    output = native.psql(container, lane + "-" + name, script)
    lines = output.strip().splitlines()
    if not lines:
        raise RuntimeError("native query omitted its final state")
    actual = base.object_value(json.loads(lines[-1]))
    expected = {"name": case["native_name"],
                "versions": case.get("native_versions", [case.get("native_version")])}
    row: dict[str, object] = {
        "name": name, "lane": lane, "sql": sql, "sql_sha256": base.digest(sql.encode()),
        "raw_output": output, "observed": actual, "expected": expected, "matches": actual == expected,
    }
    print("SPIRA_R42_NATIVE " + json.dumps(row), flush=True)
    if actual != expected:
        raise RuntimeError("native state differs: " + name)
    return row


def observe_pipeline(container: str, case: dict[str, object], plugin: Path,
                     lane: str, diagnostic: str, upstream: bool = False) -> dict[str, object]:
    name, sql = base.text(case["name"]), base.text(case["sql"])
    if re.fullmatch("[a-z0-9-]+", name) is None:
        raise ValueError("invalid case identity")
    work = base.BUILD / "catalog-cte-pipeline" / lane / name
    work.mkdir(parents=True, exist_ok=False)
    request = work / "request.json"
    wrapper = work / "capture-plugin.sh"
    wrapper.write_text("#!/usr/bin/env bash\nset -euo pipefail\ntee "
                       + shlex.quote(str(request)) + " | " + shlex.quote(str(plugin))
                       + ' "$@"\n', encoding="utf-8")
    wrapper.chmod(0o755)
    schema = ("CREATE DOMAIN version AS text;\nCREATE COLLATION version FROM \"C\";\n"
              + table_schema(case, False) + "\n")
    (work / "schema.sql").write_text(schema, encoding="utf-8")
    (work / "queries.sql").write_text("-- name: ContinueItem :exec\n" + sql + "\n", encoding="utf-8")
    option = base.text(case.get("option", "version"))
    config = ("version: \"2\"\nplugins:\n  - name: spira\n    process:\n"
              "      cmd: " + json.dumps(str(wrapper)) + "\n      format: json\n"
              "sql:\n  - engine: postgresql\n    schema: schema.sql\n    queries: queries.sql\n"
              "    codegen:\n      - plugin: spira\n        out: generated\n"
              "        options:\n          package: generated\n          tables:\n"
              "            items: {type: Item, key: id, key_type: ItemID, version: "
              + json.dumps(option) + "}\n")
    (work / "sqlc.yaml").write_text(config, encoding="utf-8")
    expected_exit = 1 if diagnostic else 0
    output = base.run(lane + "-sqlc-" + name,
                      ["go", "-C", str(base.SOURCE / "tools"), "tool", "sqlc", "generate",
                       "-f", str(work / "sqlc.yaml")], expected=expected_exit, timeout=300)
    generated = sorted((work / "generated").glob("*.lgo"))
    wanted = [] if diagnostic else ["models.sql.lgo", "queries.sql.lgo"]
    if [path.name for path in generated] != wanted:
        raise RuntimeError("actual generated file set differs: " + name)
    if diagnostic and diagnostic not in output:
        raise RuntimeError("pipeline refused for a different reason: " + name)
    if upstream:
        if name != "nested-materialized-cte-reference":
            raise ValueError("unfrozen upstream case")
        if request.exists() or generated:
            raise RuntimeError("upstream refusal unexpectedly invoked the plugin or produced files")
        expected_output = '# package spira\nqueries.sql:1:1: relation "b" does not exist\n'
        if output != expected_output or diagnostic != expected_output.strip().splitlines()[-1]:
            raise RuntimeError("upstream diagnostic differs from the frozen observation")
        row: dict[str, object] = {
            "name": name, "lane": lane, "sql": sql, "sql_sha256": base.digest(sql.encode()),
            "option": option, "columns": columns(case), "expected_diagnostic": diagnostic,
            "raw_exit": base.RECEIPTS[-1].raw_exit, "pipeline_contract": "upstream",
            "raw_output": output, "log_sha256": base.digest(output.encode()),
            "request_present": False, "files": [], "processed_native": None,
        }
        print("SPIRA_R42_PIPELINE " + json.dumps(row), flush=True)
        return row
    captured = base.object_value(json.loads(request.read_bytes()))
    queries = captured["queries"]
    if not isinstance(queries, list) or len(queries) != 1:
        raise RuntimeError("actual SQLc request does not contain exactly one query")
    query = base.object_value(queries[0])
    if query["name"] != "ContinueItem" or query["cmd"] != ":exec" or query["filename"] != "queries.sql":
        raise RuntimeError("actual SQLc query identity differs")
    processed = base.text(query["text"])
    catalog_bytes = json.dumps(captured["catalog"], sort_keys=True, separators=(",", ":")).encode()
    row: dict[str, object] = {
        "name": name, "lane": lane, "sql": sql, "sql_sha256": base.digest(sql.encode()),
        "option": option, "columns": columns(case), "expected_diagnostic": diagnostic,
        "raw_exit": base.RECEIPTS[-1].raw_exit, "request_sha256": base.digest(request.read_bytes()),
        "request_present": True,
        "pipeline_contract": "plugin-refusal" if diagnostic else "generated",
        "processed_sql": processed, "processed_sql_sha256": base.digest(processed.encode()),
        "catalog_sha256": base.digest(catalog_bytes), "catalog_canonical_bytes": len(catalog_bytes),
        "catalog_encoding": "UTF-8 JSON, sorted keys, compact separators", "query_metadata": query,
        "log_sha256": base.digest(output.encode()),
        "files": [{"path": path.name, "sha256": base.digest(path.read_bytes()),
                   "content": path.read_text(encoding="utf-8")} for path in generated],
        "processed_native": observe_native(container, case, processed, lane + "-processed"),
    }
    (base.OUTPUT / (lane + "-" + name + "-request.json")).write_bytes(request.read_bytes())
    print("SPIRA_R42_PIPELINE " + json.dumps(row), flush=True)
    return row
