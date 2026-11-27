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
        "SQLCEXPERIMENT": "",
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


