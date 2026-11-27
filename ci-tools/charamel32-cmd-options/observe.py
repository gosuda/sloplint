"""Observe a bounded native Cmd FOR/F option ambiguity without changing source."""
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
WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
OUT = WORKSPACE / "evidence"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def main() -> int:
    assert sys.platform == "win32"
    OUT.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, Any] = json.loads((ROOT / "observations.json").read_text("utf-8"))
    assert git("rev-parse", "HEAD") == manifest["source"]
    assert git("rev-parse", "HEAD^{tree}") == manifest["tree"]
    assert git("status", "--porcelain") == ""
    records: list[dict[str, Any]] = []
    failures: list[str] = []
    comspec = os.environ["COMSPEC"]
    version = subprocess.run(
        f'"{comspec}" /d /c ver', executable=comspec, stdin=subprocess.DEVNULL,
        capture_output=True, timeout=10, check=False,
    )
    with tempfile.TemporaryDirectory(prefix="charamel32-options-") as temporary:
        cwd = Path(temporary)
        for case in manifest["cases"]:
            started = time.monotonic()
            result = subprocess.run(
                f'"{comspec}" /d /c {case["command"]}', executable=comspec, cwd=cwd,
                stdin=subprocess.DEVNULL, capture_output=True, timeout=10, check=False,
            )
            stdout = result.stdout.decode("utf-8", errors="backslashreplace")
            stderr = result.stderr.decode("utf-8", errors="backslashreplace")
            lines = stdout.splitlines()
            expected = case["expected_lines"]
            accepted = expected is None or (result.returncode == 0 and lines == expected)
            row = {**case, "actual_exit": result.returncode, "stdout": stdout, "stderr": stderr,
                   "stdout_sha256": digest(result.stdout), "stderr_sha256": digest(result.stderr),
                   "seconds": time.monotonic() - started, "accepted_control": accepted}
            records.append(row)
            print("NATIVE_FOR_OPTIONS " + json.dumps(row), flush=True)
            if not accepted:
                failures.append(case["id"])
    assert git("status", "--porcelain") == ""
    report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
              "source": manifest["source"], "tree": manifest["tree"],
              "candidate_tree_under_review": manifest["candidate_tree"],
              "windows_version": version.stdout.decode("utf-8", errors="backslashreplace"),
              "driver_sha256": digest(Path(__file__).read_bytes()),
              "manifest_sha256": digest((ROOT / "observations.json").read_bytes()),
              "records": records, "control_failures": failures, "source_clean": True,
              "observational_rows_are_not_claimed_native_valid": True}
    (OUT / "native-for-options.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("NATIVE_FOR_OPTIONS_RESULT " + json.dumps(report), flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
