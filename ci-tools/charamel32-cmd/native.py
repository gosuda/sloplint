"""Observe native cmd.exe behavior against immutable public Charamel inputs."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from typing import Any

ROOT = Path(__file__).resolve().parent
OUT = Path(os.environ["GITHUB_WORKSPACE"]) / "evidence"
SOURCE = Path(os.environ["GITHUB_WORKSPACE"]) / "source"


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    if sys.platform != "win32":
        raise RuntimeError("the oracle requires native Windows")
    OUT.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, Any] = json.loads((ROOT / "baseline.json").read_text("utf-8"))
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("rev-parse", "HEAD^{tree}") == manifest["tree"]
    for name, expected in manifest["source_sha256"].items():
        assert digest(SOURCE / name) == expected, name
    comspec = os.environ["COMSPEC"]
    version = subprocess.run(
        f'"{comspec}" /d /c ver', executable=comspec, stdin=subprocess.DEVNULL,
        capture_output=True, timeout=10, check=False,
    )
    records: list[dict[str, Any]] = []
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="charamel32-native-") as temporary:
        cwd = Path(temporary)
        (cwd / "do").mkdir()
        (cwd / "do" / "note.txt").write_text("owned\n", encoding="ascii")
        (cwd / "notes.txt").write_text("alpha one\nbeta two\n", encoding="ascii")
        (cwd / "notes space.txt").write_text("gamma three\n", encoding="ascii")
        env = dict(os.environ)
        env["FILES"] = "alpha"
        env["PROGRAM"] = "echo"
        for case in manifest["cases"]:
            started = time.monotonic()
            command_line = f'"{comspec}" /d /c {case["command"]}'
            result = subprocess.run(
                command_line, executable=comspec, cwd=cwd, env=env,
                stdin=subprocess.DEVNULL, capture_output=True, timeout=10, check=False,
            )
            stdout = result.stdout.decode("utf-8", errors="backslashreplace")
            stderr = result.stderr.decode("utf-8", errors="backslashreplace")
            actual_lines = [line.strip() for line in stdout.splitlines() if line.strip()]
            expected = case["stdout_lines"]
            accepted = expected is None or (result.returncode == 0 and actual_lines == expected)
            record = {
                "id": case["id"], "command": case["command"],
                "actual_exit": result.returncode, "seconds": time.monotonic() - started,
                "stdout": stdout, "stderr": stderr,
                "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
                "expected_stdout_lines": expected, "accepted": accepted,
            }
            records.append(record)
            print("NATIVE_CMD " + json.dumps(record, ensure_ascii=True), flush=True)
            if not accepted:
                failures.append(case["id"])
    assert git("status", "--porcelain", "--untracked-files=no") == ""
    report = {
        "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "source": manifest["head"], "tree": manifest["tree"],
        "windows_version": version.stdout.decode("utf-8", errors="backslashreplace"),
        "manifest_sha256": digest(ROOT / "baseline.json"),
        "driver_sha256": digest(Path(__file__)),
        "records": records, "failures": failures, "source_clean": True,
    }
    (OUT / "native-baseline.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("NATIVE_BASELINE_RESULT " + json.dumps(report, ensure_ascii=True), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
