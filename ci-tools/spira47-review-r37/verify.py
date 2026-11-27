"""Freeze typed-literal and real sqlc preprocessing behavior on unchanged source."""
from __future__ import annotations

import hashlib
import json
import os
import re
import shlex
import subprocess
import time
from dataclasses import asdict
from pathlib import Path

import baseline_support as base

SOURCE = base.SOURCE
PROOF = base.PROOF
OUTPUT = base.OUTPUT
BUILD = base.BUILD
TEST = "cmd/sqlc-gen-spira/typed_literal_test.lgo"
DATA = "cmd/sqlc-gen-spira/testdata/typed-literals.json"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def cases_from(name: str, count: int) -> list[dict[str, object]]:
    value: object = json.loads((PROOF / name).read_bytes())
    if not isinstance(value, list) or len(value) != count:
        raise ValueError("Frozen case count differs")
    cases = [base.object_value(row) for row in value]
    if len({base.text(row["name"]) for row in cases}) != count:
        raise ValueError("Duplicate frozen case")
    return cases


def psql(container: str, name: str, sql: str) -> str:
    return base.run(name, ["docker", "exec", container, "psql", "-XAtq",
                          "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", "postgres", "-c", sql])


def observe_native(container: str, case: dict[str, object], sql: str, lane: str) -> dict[str, object]:
    name = base.text(case["name"])
    statement = ("BEGIN; SET LOCAL standard_conforming_strings = on;\n"
                 "TRUNCATE items; INSERT INTO items VALUES (1, 'before', 0);\n" + sql +
                 "\n;\nSELECT json_build_object('name', name, 'version', version) FROM items;\nROLLBACK;")
    raw = psql(container, f"{lane}-{name}", statement).strip()
    observed = base.object_value(json.loads(raw))
    expected = {"name": case["native_name"], "version": case["native_version"]}
    row: dict[str, object] = {"id": name, "lane": lane, "sql": sql,
                              "sql_sha256": digest(sql.encode()), "observed": observed,
                              "expected": expected, "matches": observed == expected}
    print("TYPED_NATIVE_CASE " + json.dumps(row), flush=True)
    return row


def observe_sqlc(container: str, case: dict[str, object], plugin: Path) -> dict[str, object]:
    name, sql = base.text(case["name"]), base.text(case["sql"])
    if not re.fullmatch("[a-z-]+", name):
        raise ValueError("Unsafe frozen case ID")
    work = BUILD / "sqlc-cases" / name
    work.mkdir(parents=True, exist_ok=False)
    request = work / "request.json"
    wrapper = work / "capture-plugin.sh"
    wrapper.write_text("#!/usr/bin/env bash\nset -euo pipefail\n"
                       + "tee " + shlex.quote(str(request)) + " | "
                       + shlex.quote(str(plugin)) + ' "$@"\n', encoding="utf-8")
    wrapper.chmod(0o755)
    (work / "schema.sql").write_bytes((PROOF / "schema.sql").read_bytes())
    (work / "queries.sql").write_text("-- name: ContinueItem :exec\n" + sql + "\n", encoding="utf-8")
    config = ("version: \"2\"\nplugins:\n  - name: spira\n    process:\n"
              "      cmd: " + json.dumps(str(wrapper)) + "\n      format: json\n"
              "sql:\n  - engine: postgresql\n    schema: schema.sql\n    queries: queries.sql\n"
              "    codegen:\n      - plugin: spira\n        out: generated\n"
              "        options:\n          package: generated\n          tables:\n"
              "            items: {type: Item, key: id, key_type: ItemID, version: version}\n")
    (work / "sqlc.yaml").write_text(config, encoding="utf-8")
    command = ["go", "-C", str(SOURCE / "tools"), "tool", "sqlc", "generate",
               "-f", str(work / "sqlc.yaml")]
    env = base.environment()
    env["GOWORK"] = "off"
    log_path = OUTPUT / f"sqlc-{name}.log"
    print("SQLC_OBSERVATION_START " + json.dumps({"id": name, "command": command}), flush=True)
    with log_path.open("wb") as stream:
        result = subprocess.run(command, cwd=SOURCE, env=env, stdout=stream,
                                stderr=subprocess.STDOUT, timeout=300, check=False)
    output = log_path.read_text(encoding="utf-8")
    row: dict[str, object] = {
        "id": name, "command": command, "raw_exit": result.returncode,
        "input_sql": sql, "input_sha256": digest(sql.encode()),
        "log_sha256": digest(log_path.read_bytes()), "log": output,
        "captured_request": request.exists(),
    }
    if result.returncode not in (0, 1):
        raise RuntimeError(f"sqlc {name} exited unexpectedly: {result.returncode}")
    if request.exists():
        data = request.read_bytes()
        captured = base.object_value(json.loads(data))
        queries = captured.get("queries")
        if not isinstance(queries, list) or len(queries) != 1:
            raise ValueError("Expected exactly one real sqlc query")
        query = base.object_value(queries[0])
        processed = base.text(query["text"])
        row["sqlc_version"] = captured.get("sqlc_version")
        row["request_sha256"] = digest(data)
        row["processed_sql"] = processed
        row["processed_sha256"] = digest(processed.encode())
        row["processed_native"] = observe_native(container, case, processed, "sqlc-native")
        (OUTPUT / f"sqlc-{name}-request.json").write_bytes(data)
    generated = sorted((work / "generated").glob("*.lgo"))
    row["generated_files"] = [
        {"path": path.name, "sha256": digest(path.read_bytes()), "content": path.read_text(encoding="utf-8")}
        for path in generated
    ]
    print("SQLC_OBSERVATION_RESULT " + json.dumps(row), flush=True)
    return row


def native_and_pipeline(manifest: dict[str, object]) -> dict[str, object]:
    cases = cases_from("native-cases.json", 26)
    run_id, attempt = os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"]
    if not run_id.isdigit() or not attempt.isdigit():
        raise ValueError("Invalid workflow run identity")
    container = f"spira47-typed-{run_id}-{attempt}"
    image = base.text(manifest["postgres_image"])
    base.run("postgres-image", ["docker", "pull", image], timeout=1200)
    base.run("postgres-image-identity", ["docker", "image", "inspect", image])
    started = False
    native: list[dict[str, object]] = []
    pipeline: list[dict[str, object]] = []
    try:
        base.run("postgres-start", ["docker", "run", "--detach", "--network", "none",
                                  "--name", container, "--env", "POSTGRES_HOST_AUTH_METHOD=trust", image])
        started = True
        deadline = time.monotonic() + 120
        while True:
            result = subprocess.run(["docker", "exec", container, "pg_isready", "-U", "postgres"],
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
            print("POSTGRES_READY " + json.dumps({"raw_exit": result.returncode,
                                                  "output": result.stdout.decode()}), flush=True)
            if result.returncode == 0:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError("Native PostgreSQL did not become ready")
            time.sleep(2)
        version = psql(container, "postgres-version", "SHOW server_version_num;").strip()
        if version != manifest["postgres_version_num"]:
            raise RuntimeError("Wrong PostgreSQL server version")
        psql(container, "postgres-schema", (PROOF / "schema.sql").read_text(encoding="utf-8"))
        for case in cases:
            native.append(observe_native(container, case, base.text(case["sql"]), "original-native"))
        if any(row["matches"] is not True for row in native):
            raise RuntimeError("A frozen native literal expectation differs")
        plugin = BUILD / "sqlc-gen-spira"
        base.run("build-real-plugin", ["logos", "build", "-o", str(plugin), "./cmd/sqlc-gen-spira"], timeout=900)
        base.run("sqlc-modules", ["go", "-C", str(SOURCE / "tools"), "mod", "download"], timeout=900)
        module = json.loads(base.run("sqlc-module", ["go", "-C", str(SOURCE / "tools"), "list",
                                                   "-m", "-json", "github.com/sqlc-dev/sqlc"]))
        if module["Version"] != "v1.31.2-0.20261023030206-3ad8b20fb02b":
            raise RuntimeError("Source-pinned sqlc version differs")
        base.run("sqlc-version", ["go", "-C", str(SOURCE / "tools"), "tool", "sqlc", "version"], timeout=900)
        for case in cases:
            row = observe_sqlc(container, case, plugin)
            pipeline.append(row)
            (OUTPUT / "pipeline-observations.json").write_text(json.dumps(pipeline, indent=2) + "\n", encoding="utf-8")
        if any(row["captured_request"] is not True for row in pipeline):
            raise RuntimeError("sqlc rejected an input before the actual plugin; inspect archived observations")
        if any(base.object_value(row["processed_native"])["matches"] is not True for row in pipeline):
            raise RuntimeError("Actual sqlc preprocessing changed native query semantics")
        return {"version": version, "original": native, "pipeline": pipeline}
    finally:
        (OUTPUT / "native-observations.json").write_text(json.dumps(native, indent=2) + "\n", encoding="utf-8")
        if started:
            base.run("postgres-cleanup", ["docker", "rm", "-f", "-v", container])


def main() -> None:
    manifest = base.object_value(json.loads((PROOF / "manifest.json").read_bytes()))
    for name, expected in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or digest((PROOF / name).read_bytes()) != expected:
            raise RuntimeError(f"Frozen proof bytes differ: {name}")
    source = base.object_value(manifest["source"])
    report: dict[str, object] = {"schema": 1, "manifest": manifest, "baseline_verified": False,
                                "source_restored": False, "publication_eligible": False}
    created: list[Path] = []
    try:
        for suffix, expected in (("HEAD", source["head"]), ("HEAD^{tree}", source["tree"])):
            actual = base.run("source-" + ("head" if suffix == "HEAD" else "tree"),
                              ["git", "rev-parse", suffix]).strip()
            if actual != expected:
                raise RuntimeError("Wrong immutable source")
        report["initial_index_sha256"] = base.index_identity(source, "before")
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("External Go cache is active")
        for path, payload in ((TEST, "typed_literal_test.lgo"), (DATA, "typed-literals.json")):
            target = SOURCE / path
            if target.exists():
                raise RuntimeError("Frozen temporary test path already exists")
            target.write_bytes((PROOF / payload).read_bytes())
            created.append(target)
        base.run("generator-compile", ["logos", "test", "-race", "-run", "^$", "-count=1",
                                      "./cmd/sqlc-gen-spira"], timeout=900)
        log = base.run("typed-literal-unchanged-red", ["logos", "test", "-race", "-count=1", "-v",
                       "-run", "^TestPostgresTypedLiteralBoundaries$", "./cmd/sqlc-gen-spira"],
                       expected=1, timeout=900)
        cases = cases_from("typed-literals.json", 12)
        expected_fails = {base.text(row["name"]) for row in cases[:8]}
        failed = set(re.findall(r"^\s*--- FAIL: TestPostgresTypedLiteralBoundaries/([a-z-]+) ", log, re.MULTILINE))
        passed = set(re.findall(r"^\s*--- PASS: TestPostgresTypedLiteralBoundaries/([a-z-]+) ", log, re.MULTILINE))
        if failed != expected_fails or passed != {base.text(row["name"]) for row in cases[8:]}:
            raise RuntimeError("Unchanged generator did not reproduce the frozen typed-literal boundary/control split")
        report["generator_failed"] = sorted(failed)
        report["generator_passed"] = sorted(passed)
        base.run("retained-generator-controls", ["logos", "test", "-race", "-count=1", "-v",
                 "-run", "^(TestPostgresContinuedEscapeStrings|TestVersionedWrite)", "./cmd/sqlc-gen-spira"], timeout=900)
        report["native_and_pipeline"] = native_and_pipeline(manifest)
        report["baseline_verified"] = True
    finally:
        try:
            base.run("restore-tracked-source", ["git", "restore", "--source", base.text(source["head"]),
                                               "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored_index_sha256"] = base.index_identity(source, "restored")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            report["source_restored"] = True
        finally:
            report["baseline_verified"] = bool(report["baseline_verified"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("TYPED_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
