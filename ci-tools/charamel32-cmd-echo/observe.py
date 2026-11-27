"""Freeze native Cmd echo and nested-payload observations on the exact published source."""
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
    return {"head": MANIFEST["head"], "tree": MANIFEST["tree"],
            "source_sha256": MANIFEST["source_sha256"], "source_clean": True,
            "index_sha256": digest(subprocess.check_output(
                ["git", "-C", str(SOURCE), "ls-files", "-s"]))}


def main() -> int:
    assert sys.platform == "win32"
    OUT.mkdir(parents=True, exist_ok=True)
    before = identity()
    comspec = os.environ["COMSPEC"]
    records: list[dict[str, Any]] = []
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="charamel32-echo-") as temporary:
        cwd = Path(temporary)
        for case in MANIFEST["cases"]:
            capture = cwd / "capture.txt"
            capture.unlink(missing_ok=True)
            started = time.monotonic()
            timed_out = False
            try:
                result = subprocess.run(
                    f'"{comspec}" /d /c {case["command"]}', executable=comspec,
                    cwd=cwd, env=dict(os.environ), stdin=subprocess.DEVNULL,
                    capture_output=True, timeout=10, check=False)
                exit_code: int | None = result.returncode
                stdout = result.stdout
                stderr = result.stderr
            except subprocess.TimeoutExpired as error:
                exit_code = None
                stdout = error.stdout or b""
                stderr = error.stderr or b""
                timed_out = True
            text_out = stdout.decode("utf-8", errors="backslashreplace").replace(str(cwd), "{cwd}")
            text_err = stderr.decode("utf-8", errors="backslashreplace").replace(str(cwd), "{cwd}")
            required = case["required"]
            passed = not timed_out and all(
                {"stdout": text_out, "stderr": text_err, "actual_exit": exit_code}[key] == value
                for key, value in required.items())
            saved = capture.read_bytes() if capture.exists() else None
            row = {"id": case["id"], "command": case["command"],
                   "actual_exit": exit_code, "timeout": timed_out,
                   "seconds": time.monotonic() - started, "stdout": text_out, "stderr": text_err,
                   "stdout_sha256": digest(stdout), "stderr_sha256": digest(stderr),
                   "capture": None if saved is None else saved.decode("utf-8", errors="backslashreplace"),
                   "capture_sha256": None if saved is None else digest(saved),
                   "required": required, "required_pass": passed}
            records.append(row)
            if not passed:
                failures.append(case["id"])
            print("NATIVE_ECHO_CONTEXT " + json.dumps(row, ensure_ascii=True), flush=True)
    by_id = {row["id"]: row for row in records}
    pairs: list[dict[str, Any]] = []
    for original, spaced in MANIFEST["distinguishing_pairs"]:
        different = by_id[original]["stdout"] != by_id[spaced]["stdout"]
        pair = {"original": original, "spaced": spaced, "different_stdout": different}
        pairs.append(pair)
        if not different:
            failures.append(original + ":lost-discriminator")
    report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
              "identity": identity(), "before": before, "records": records, "pairs": pairs,
              "failures": failures, "record_count": len(records),
              "scope": "Native observation; only explicit required fields and three output distinctions are acceptance requirements."}
    (OUT / "native-echo.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("NATIVE_ECHO_RESULT " + json.dumps(report, ensure_ascii=True), flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
