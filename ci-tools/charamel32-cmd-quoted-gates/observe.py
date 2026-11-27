"""Observe owned native Cmd processes without changing Charamel source."""
from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time
from typing import Any

ROOT = Path(__file__).resolve().parent
WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
OUT = WORKSPACE / "evidence"
MANIFEST: dict[str, Any] = json.loads((ROOT / "observations.json").read_text("utf-8"))


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def identity() -> dict[str, Any]:
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("rev-parse", "HEAD^{tree}") == MANIFEST["tree"]
    assert git("status", "--porcelain") == ""
    for path, expected in MANIFEST["source_sha256"].items():
        assert digest((SOURCE / path).read_bytes()) == expected, path
    return {
        "head": MANIFEST["head"],
        "tree": MANIFEST["tree"],
        "source_sha256": MANIFEST["source_sha256"],
        "source_clean": True,
        "index_sha256": digest(subprocess.check_output(["git", "-C", str(SOURCE), "ls-files", "-s"])),
    }


def captured(data: bytes) -> dict[str, Any]:
    return {
        "bytes": len(data),
        "sha256": digest(data),
        "base64": base64.b64encode(data).decode("ascii"),
        "text": data.decode("utf-8", errors="backslashreplace"),
    }


def child_records(streams: list[bytes]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for stream in streams:
        for line in stream.decode("utf-8-sig", errors="replace").splitlines():
            try:
                record = json.loads(line)
            except (ValueError, TypeError):
                continue
            if isinstance(record, dict) and "parent_command_line" in record:
                records.append(record)
    return records


def observe(case: dict[str, Any], parent: Path, helper: bytes) -> dict[str, Any]:
    cwd = parent / str(case["id"])
    cwd.mkdir()
    (cwd / "observe-child.ps1").write_bytes(helper)
    comspec = os.environ["COMSPEC"]
    command_line = f'"{comspec}" /d /c {case["command"]}'
    started = time.monotonic()
    process = subprocess.Popen(
        command_line,
        executable=comspec,
        cwd=cwd,
        env=dict(os.environ),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    timed_out = False
    cleanup: dict[str, Any] | None = None
    try:
        stdout, stderr = process.communicate(timeout=30)
    except subprocess.TimeoutExpired:
        timed_out = True
        if process.poll() is None:
            stopped = subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=10,
                check=False,
            )
            cleanup = {
                "actual_exit": stopped.returncode,
                "stdout": captured(stopped.stdout),
                "stderr": captured(stopped.stderr),
            }
        stdout, stderr = process.communicate(timeout=10)
    files: dict[str, dict[str, Any]] = {}
    streams = [stdout]
    for path in sorted(cwd.iterdir()):
        if path.is_file() and path.name != "observe-child.ps1":
            data = path.read_bytes()
            files[path.name] = captured(data)
            streams.append(data)
    children = child_records(streams)
    child_ok = case["kind"] != "child" or (
        len(children) == 1
        and str(children[0]["parent_name"]).lower() == "cmd.exe"
        and "observe-child.ps1" in str(children[0]["parent_command_line"])
    )
    return {
        "id": case["id"],
        "kind": case["kind"],
        "command": case["command"],
        "outer_command_line": command_line,
        "outer_pid": process.pid,
        "actual_exit": process.returncode,
        "seconds": time.monotonic() - started,
        "timeout": timed_out,
        "stdout": captured(stdout),
        "stderr": captured(stderr),
        "files": files,
        "owned_children": children,
        "cleanup": cleanup,
        "required_pass": not timed_out and process.returncode == case["require_exit"] and child_ok,
    }


def main() -> int:
    assert sys.platform == "win32"
    OUT.mkdir(parents=True, exist_ok=True)
    helper = (ROOT / "observe-child.ps1").read_bytes()
    assert digest(helper) == MANIFEST["observer_sha256"]
    before = identity()
    records: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="charamel32-redirection-") as temporary:
        for case in MANIFEST["cases"]:
            row = observe(case, Path(temporary), helper)
            records.append(row)
            print("NATIVE_REDIRECTION_CONTEXT " + json.dumps(row, ensure_ascii=True), flush=True)
    after = identity()
    assert after == before
    failures = [row["id"] for row in records if not row["required_pass"]]
    report = {
        "producer": os.environ["GITHUB_SHA"],
        "run": os.environ["GITHUB_RUN_ID"],
        "before": before,
        "after": after,
        "system": {"python": sys.version, "windows": platform.win32_ver(), "comspec": os.environ["COMSPEC"]},
        "inputs": {name: digest((ROOT / name).read_bytes()) for name in ("observe.py", "observe-child.ps1", "observations.json")},
        "records": records,
        "record_count": len(records),
        "failures": failures,
        "scope": MANIFEST["scope"],
    }
    (OUT / "native-redirection.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("NATIVE_REDIRECTION_RESULT " + json.dumps(report, ensure_ascii=True), flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
