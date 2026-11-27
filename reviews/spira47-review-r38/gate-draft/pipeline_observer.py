"""Capture actual pinned-sqlc requests without assuming successful generation."""
from __future__ import annotations

import json
import re
import shlex
from pathlib import Path

import baseline_support as base
import native_support as native


def observe_case(container: str, case: dict[str, object], plugin: Path,
                 passed: set[str]) -> dict[str, object]:
    name, sql = base.text(case["name"]), base.text(case["sql"])
    if not re.fullmatch("[a-z0-9-]+", name):
        raise ValueError("unsafe frozen case name")
    work = base.BUILD / "dollar-pipeline" / name
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
    # The direct unchanged-source result predicts the plugin result. This is not a correctness oracle.
    direct_accepts = (name in passed) == (case["allow"] is True)
    expected_exit = 0 if direct_accepts else 1
    command = ["go", "-C", str(base.SOURCE / "tools"), "tool", "sqlc", "generate",
               "-f", str(work / "sqlc.yaml")]
    output = base.run("dollar-sqlc-" + name, command, expected=expected_exit, timeout=300)
    generated = sorted((work / "generated").glob("*.lgo"))
    row: dict[str, object] = {
        "id": name, "input_sql": sql, "input_sha256": base.digest(sql.encode()),
        "raw_exit": base.RECEIPTS[-1].raw_exit, "predicted_exit": expected_exit,
        "prediction_is_correctness_oracle": False,
        "captured_request": request.exists(), "log": output,
        "log_sha256": base.digest(output.encode()),
        "generated_files": [{"path": path.name, "content": path.read_bytes().decode("utf-8"),
                             "sha256": base.digest(path.read_bytes())} for path in generated],
    }
    if not request.exists():
        raise RuntimeError("new dollar case was rejected before plugin invocation: " + name)
    captured = base.object_value(json.loads(request.read_bytes()))
    queries = captured["queries"]
    if not isinstance(queries, list) or len(queries) != 1:
        raise ValueError("expected one actual plugin query")
    processed = base.text(base.object_value(queries[0])["text"])
    row.update({"request_sha256": base.digest(request.read_bytes()), "sqlc_version": captured.get("sqlc_version"),
                "processed_sql": processed, "processed_sha256": base.digest(processed.encode()),
                "processed_native": native.observe_native(container, case, processed, "dollar-processed")})
    (base.OUTPUT / (name + "-request.json")).write_bytes(request.read_bytes())
    if direct_accepts:
        if [path.name for path in generated] != ["models.sql.lgo", "queries.sql.lgo"]:
            raise RuntimeError("wrong generated file set")
        row["outcome"] = "generated" if case["allow"] is True else "false-acceptance"
    else:
        if generated or "ContinueItem: writes versioned table items without version" not in output:
            raise RuntimeError("unexpected plugin refusal")
        row["outcome"] = "version-refusal" if case["allow"] is False else "false-refusal"
    print("DOLLAR_SQLC_OBSERVATION " + json.dumps(row), flush=True)
    return row
