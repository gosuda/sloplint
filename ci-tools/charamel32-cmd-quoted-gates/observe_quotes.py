"""Observe native Cmd quoted filenames using only owned files and processes."""
from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from typing import Any


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


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
    for name, content in case["input_files"].items():
        relative = Path(name)
        assert len(relative.parts) == 1 and name not in (".", "..", "observe-child.ps1")
        assert not relative.is_absolute() and relative.name == name
        (cwd / name).write_bytes(content.encode("ascii"))
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
        "required_pass": (not timed_out
                          and (case["require_exit"] is None
                               or (process.returncode == case["require_exit"] and child_ok))
                          and (case["required_stdout"] is None
                               or stdout == case["required_stdout"].encode("ascii"))),
        "assertion_scope": case["assertion_scope"],
    }


