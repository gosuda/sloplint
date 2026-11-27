"""Native PostgreSQL observations for exact original and compiled query bytes."""
from __future__ import annotations

import json
import os
import subprocess
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

import baseline_support as base

PROOF = base.PROOF


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
                              "sql_sha256": base.digest(sql.encode()), "observed": observed,
                              "expected": expected, "matches": observed == expected}
    print("TYPED_NATIVE_CASE " + json.dumps(row), flush=True)
    return row



@contextmanager
def postgres(manifest: dict[str, object]) -> Iterator[tuple[str, str]]:
    run_id, attempt = os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"]
    if not run_id.isdigit() or not attempt.isdigit():
        raise ValueError("invalid native run identity")
    container = f"spira47-dollar-{run_id}-{attempt}"
    image = base.text(manifest["postgres_image"])
    base.run("postgres-image", ["docker", "pull", image], timeout=1200)
    base.run("postgres-image-identity", ["docker", "image", "inspect", image])
    started = False
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
                raise RuntimeError("native PostgreSQL did not become ready")
            time.sleep(2)
        version = psql(container, "postgres-version", "SHOW server_version_num;").strip()
        if version != manifest["postgres_version_num"]:
            raise RuntimeError("wrong PostgreSQL server version")
        psql(container, "postgres-schema", (PROOF / "schema.sql").read_bytes().decode("utf-8"))
        yield container, version
    finally:
        if started:
            base.run("postgres-cleanup", ["docker", "rm", "-f", "-v", container])
