"""Freeze PostgreSQL and actual-generator outcomes without changing production."""
from __future__ import annotations

import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Final

SOURCE: Final = Path(sys.argv[1]).absolute()
PROOF: Final = Path(sys.argv[2]).absolute()
OUTPUT: Final = Path(sys.argv[3]).absolute()
OUTPUT.mkdir(parents=True, exist_ok=True)
BUILD: Final = OUTPUT.parent / "spira47-baseline-build"
BUILD.mkdir(parents=True, exist_ok=True)
TEST_PATH: Final = "cmd/sqlc-gen-spira/continued_escape_test.lgo"
DATA_PATH: Final = "cmd/sqlc-gen-spira/testdata/continued-strings.json"


@dataclass(frozen=True)
class Receipt:
    name: str
    command: tuple[str, ...]
    raw_exit: int
    expected: int
    timed_out: bool
    log_sha256: str


RECEIPTS: Final[list[Receipt]] = []


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def object_value(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or any(not isinstance(key, str) for key in value):
        raise ValueError("expected a JSON object")
    return value


def text(value: object) -> str:
    if not isinstance(value, str):
        raise ValueError("expected text")
    return value


def environment() -> dict[str, str]:
    env = os.environ.copy()
    env.update({
        "GOENV": "off", "GOCACHEPROG": "", "GOTOOLCHAIN": "local",
        "GOCACHE": str(BUILD / "cache"), "GOMAXPROCS": "4", "GOFLAGS": "-p=4",
    })
    return env


def run(name: str, command: list[str], expected: int = 0, timeout: int = 600) -> str:
    print("SPIRA_COMMAND_START " + json.dumps({"name": name, "command": command}), flush=True)
    log_path = OUTPUT / f"{name}.log"
    timed_out = False
    with log_path.open("wb") as stream:
        process = subprocess.Popen(command, cwd=SOURCE, env=environment(),
                                   stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + timeout
        while True:
            try:
                raw_exit = process.wait(timeout=max(0.001, min(30, deadline - time.monotonic())))
                break
            except subprocess.TimeoutExpired:
                if time.monotonic() < deadline:
                    print("SPIRA_COMMAND_PROGRESS " + json.dumps({
                        "name": name, "log_bytes": log_path.stat().st_size,
                    }), flush=True)
                    continue
                timed_out = True
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    raw_exit = process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    raw_exit = process.wait()
                break
    data = log_path.read_bytes()
    receipt = Receipt(name, tuple(command), raw_exit, expected, timed_out, digest(data))
    RECEIPTS.append(receipt)
    print("SPIRA_COMMAND_RESULT " + json.dumps(asdict(receipt)), flush=True)
    output = data.decode("utf-8", errors="strict")
    print(output, end="" if output.endswith("\n") else "\n", flush=True)
    if raw_exit != expected or timed_out:
        raise RuntimeError(f"{name}: actual exit {raw_exit}, expected {expected}, timeout={timed_out}")
    return output


def index_identity(manifest: dict[str, object], label: str) -> str:
    rows: list[dict[str, str]] = json.loads((PROOF / "source-index.json").read_bytes())
    actual: list[dict[str, str]] = []
    tracked = subprocess.run(["git", "ls-files", "-s", "-z"], cwd=SOURCE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout
    for entry in tracked.split(b"\0"):
        if not entry:
            continue
        header, raw_path = entry.split(b"\t", 1)
        mode, blob, stage = header.decode("ascii").split()
        if stage != "0":
            raise RuntimeError("source index has unmerged entries")
        actual.append({"path": raw_path.decode("utf-8"), "mode": mode, "sha": blob})
    if sorted(actual, key=lambda row: row["path"]) != sorted(rows, key=lambda row: row["path"]):
        raise RuntimeError("full source index differs")
    content_rows: list[dict[str, str]] = []
    for row in rows:
        path = SOURCE / row["path"]
        data = path.read_bytes()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if blob != row["sha"] or bool(path.stat().st_mode & 0o111) != (row["mode"] == "100755"):
            raise RuntimeError(f"source content/mode differs: {row['path']}")
        content_rows.append({**row, "sha256": digest(data)})
    if len(content_rows) != manifest["blobs"]:
        raise RuntimeError("source blob count differs")
    result = digest(json.dumps(content_rows, sort_keys=True, separators=(",", ":")).encode())
    print("SPIRA_SOURCE_IDENTITY " + json.dumps({"label": label, "blobs": len(rows),
                                                "index_sha256": result}), flush=True)
    return result


def native_cases(manifest: dict[str, object]) -> list[dict[str, object]]:
    run_id = os.environ["GITHUB_RUN_ID"]
    attempt = os.environ["GITHUB_RUN_ATTEMPT"]
    if not run_id.isdigit() or not attempt.isdigit():
        raise ValueError("invalid native run identity")
    container = f"spira47-continued-{run_id}-{attempt}"
    image = text(manifest["postgres_image"])
    raw_cases: object = json.loads((PROOF / "continued-strings.json").read_bytes())
    if not isinstance(raw_cases, list) or len(raw_cases) != 14:
        raise ValueError("wrong frozen native case count")
    observations: list[dict[str, object]] = []
    run("postgres-image", ["docker", "pull", image], timeout=1200)
    run("postgres-image-identity", ["docker", "image", "inspect", image])
    try:
        run("postgres-start", ["docker", "run", "--detach", "--network", "none", "--name", container,
                               "--env", "POSTGRES_HOST_AUTH_METHOD=trust", image])
        ready = False
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            probe = subprocess.run(["docker", "exec", container, "pg_isready", "-U", "postgres"],
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
            print("POSTGRES_READY " + json.dumps({"exit": probe.returncode,
                                                   "output": probe.stdout.decode()}), flush=True)
            if probe.returncode == 0:
                ready = True
                break
            time.sleep(2)
        if not ready:
            raise RuntimeError("native PostgreSQL did not become ready")
        psql = ["docker", "exec", container, "psql", "-XAtq", "-v", "ON_ERROR_STOP=1",
                "-U", "postgres", "-d", "postgres", "-c"]
        version = run("postgres-version", psql + ["SHOW server_version_num;"]).strip()
        if version != text(manifest["postgres_version_num"]):
            raise RuntimeError(f"native PostgreSQL version differs: {version}")
        for raw in raw_cases:
            case = object_value(raw)
            name, sql = text(case["name"]), text(case["sql"])
            if not re.fullmatch("[a-z-]+", name):
                raise ValueError("invalid case name")
            query = ("SET standard_conforming_strings = on;\n"
                     "CREATE TEMP TABLE items (id bigint PRIMARY KEY, name text, version integer);\n"
                     "INSERT INTO items VALUES (1, 'before', 0);\n" + sql +
                     "\nSELECT json_build_object('name', name, 'version', version) FROM items;")
            output = run(f"native-{name}", psql + [query]).strip()
            observed = object_value(json.loads(output))
            expected = {"name": text(case["native_name"]), "version": case["native_version"]}
            record: dict[str, object] = {
                "id": name, "sql_sha256": digest(sql.encode()), "server_version_num": version,
                "observed": observed, "expected": expected, "matches": observed == expected,
            }
            observations.append(record)
            print("POSTGRES_CONTINUATION_CASE " + json.dumps(record), flush=True)
        if any(row["matches"] is not True for row in observations):
            raise RuntimeError("native PostgreSQL differs from the frozen continuation contract")
        return observations
    finally:
        run("postgres-cleanup", ["docker", "rm", "-f", "-v", container])


def main() -> None:
    manifest = object_value(json.loads((PROOF / "manifest.json").read_bytes()))
    hashes = object_value(manifest["proof_sha256"])
    for name, value in hashes.items():
        if Path(name).name != name or digest((PROOF / name).read_bytes()) != text(value):
            raise RuntimeError(f"frozen proof digest differs: {name}")
    source = object_value(manifest["source"])
    for suffix, wanted in (("HEAD", source["head"]), ("HEAD^{tree}", source["tree"])):
        observed = run("source-" + ("head" if suffix == "HEAD" else "tree"),
                       ["git", "rev-parse", suffix]).strip()
        if observed != wanted:
            raise RuntimeError("wrong unchanged source checkout")
    report: dict[str, object] = {"schema": 1, "manifest": manifest, "source_restored": False,
                                "baseline_verified": False, "baseline_gates_passed": False, "publication_eligible": False}
    created: list[Path] = []
    try:
        report["initial_index_sha256"] = index_identity(source, "before")
        version = run("logos-version", ["logos", "version"])
        if not re.search(r"^logos version go1\.27(?:\.4)?(?:\s|$)", version, flags=re.MULTILINE):
            raise RuntimeError("wrong LoGOs toolchain")
        if run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external Go cache remained active")
        report["native"] = native_cases(manifest)
        for target, payload in ((TEST_PATH, "continued_escape_test.lgo"), (DATA_PATH, "continued-strings.json")):
            if (SOURCE / target).exists():
                raise RuntimeError(f"frozen temporary path already exists: {target}")
            (SOURCE / target).write_bytes((PROOF / payload).read_bytes())
            created.append(SOURCE / target)
        run("generator-compile", ["logos", "test", "-run", "^$", "-count=1", "./cmd/sqlc-gen-spira"])
        log = run("generator-unchanged-red", ["logos", "test", "-count=1", "-v",
                  "-run", "^TestPostgresContinuedEscapeStrings$", "./cmd/sqlc-gen-spira"], expected=1)
        started = set(re.findall(r"^=== RUN   TestPostgresContinuedEscapeStrings/([a-z-]+)$",
                                 log, flags=re.MULTILINE))
        cases: list[dict[str, object]] = json.loads((PROOF / "continued-strings.json").read_bytes())
        if started != {text(case["name"]) for case in cases}:
            raise RuntimeError("unchanged generator did not execute every frozen case")
        failed = set(re.findall(r"^\s*--- FAIL: TestPostgresContinuedEscapeStrings/([a-z-]+) ",
                                log, flags=re.MULTILINE))
        passed = set(re.findall(r"^\s*--- PASS: TestPostgresContinuedEscapeStrings/([a-z-]+) ",
                                log, flags=re.MULTILINE))
        if "escape-lf" not in failed or "Generate rejected native-valid versioned SQL" not in log:
            raise RuntimeError("unchanged source did not reproduce the reported generator failure")
        if not {"escape-single", "standard-continuation", "independent-standard",
                "independent-escape", "comment-only-version", "standard-final-backslash"} <= passed:
            raise RuntimeError("an adjacent generator control failed")
        report["generator_failed"] = sorted(failed)
        report["generator_passed"] = sorted(passed)
        run("existing-generator-controls", ["logos", "test", "-race", "-count=1",
            "-run", "^TestVersionedWrite", "./cmd/sqlc-gen-spira"], timeout=900)
        run("existing-migration-controls", ["logos", "test", "-race", "-count=1",
            "-run", "^(TestQueuedApplyHonorsContext|TestReleaseFreesTheTurnOnlyOnce|TestMigrationTurnRejectsEndedContextOnFreePool|TestApplyPartialPrefixLeavesSchema)$",
            "./migrate"], timeout=900)
        report["baseline_gates_passed"] = True
    finally:
        try:
            run("restore-tracked-source", ["git", "restore", "--source", text(source["head"]),
                                          "--staged", "--worktree", "--", "."])
            for target in created:
                target.unlink(missing_ok=True)
            report["restored_index_sha256"] = index_identity(source, "restored")
            run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            report["source_restored"] = True
        finally:
            report["baseline_verified"] = bool(report["baseline_gates_passed"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in RECEIPTS]
            (OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_BASELINE_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
