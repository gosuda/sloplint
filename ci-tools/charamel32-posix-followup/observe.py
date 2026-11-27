"""Freeze literal fallback and GNU env executable boundaries on the current public source."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
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
    OUT.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, Any] = json.loads((ROOT / "observations.json").read_text("utf-8"))
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("rev-parse", "HEAD^{tree}") == manifest["tree"]
    assert git("status", "--porcelain") == ""
    for path, expected in manifest["source_sha256"].items():
        assert digest((SOURCE / path).read_bytes()) == expected, path
    records: list[dict[str, Any]] = []
    failures: list[str] = []

    def observe(
        identity: str, args: list[str], cwd: Path, environment: dict[str, str],
        expected_lines: list[str], expected_exit: int = 0,
    ) -> None:
        started = time.monotonic()
        result = subprocess.run(
            args, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
            capture_output=True, timeout=10, check=False,
        )
        stdout = result.stdout.decode("utf-8", errors="backslashreplace")
        stderr = result.stderr.decode("utf-8", errors="backslashreplace")
        accepted = result.returncode == expected_exit and stdout.splitlines() == expected_lines
        row = {"id": identity, "argv": args, "actual_exit": result.returncode,
               "stdout": stdout, "stderr": stderr, "seconds": time.monotonic() - started,
               "stdout_sha256": digest(result.stdout), "stderr_sha256": digest(result.stderr),
               "expected_lines": expected_lines, "expected_exit": expected_exit,
               "accepted": accepted}
        records.append(row)
        print("NATIVE_FOLLOWUP " + json.dumps(row), flush=True)
        if not accepted:
            failures.append(identity)

    with tempfile.TemporaryDirectory(prefix="charamel32-followup-") as temporary:
        cwd = Path(temporary)
        for name, label in (("-", "DASH"), ("-i", "DASH_I"), ("brew.exe", "BREW")):
            stub = cwd / name
            stub.write_text(
                "#!/bin/sh\nprintf 'EXEC:" + label + "\\n'\nprintf '<%s>\\n' \"$@\"\n",
                encoding="ascii",
            )
            stub.chmod(0o755)
        environment = dict(os.environ)
        environment["PATH"] = str(cwd) + os.pathsep + environment["PATH"]
        helper = 'probe() { printf "%s\\n" "$#"; printf "<%s>\\n" "$@"; }; '
        setup = "unset pattern; set -- alpha beta; CHAR32_PREFIX_A=one; CHAR32_PREFIX_B=two; "
        for shell in ("/bin/bash", "/bin/dash"):
            for case in manifest["arity"]:
                observe(Path(shell).name + "_" + case["id"],
                        [shell, "-c", helper + setup + "probe " + case["expression"]],
                        cwd, environment, case["lines"])
        observe("bash_real_indirect_at",
                ["/bin/bash", "--noprofile", "--norc", "-c",
                 helper + setup + 'probe "' + chr(36) + '{!CHAR32_PREFIX@}"'],
                cwd, environment, ["2", "<CHAR32_PREFIX_A>", "<CHAR32_PREFIX_B>"])
        executable = shutil.which("env")
        if executable is None:
            raise RuntimeError("GNU env is required")
        for case in manifest["env"]:
            observe(case["id"], [executable, *case["args"]], cwd, environment, case["lines"])
    assert git("status", "--porcelain") == ""
    report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
              "source": manifest["head"], "tree": manifest["tree"], "source_clean": True,
              "driver_sha256": digest(Path(__file__).read_bytes()),
              "manifest_sha256": digest((ROOT / "observations.json").read_bytes()),
              "records": records, "failures": failures}
    (OUT / "native-followup.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("NATIVE_FOLLOWUP_RESULT " + json.dumps(report), flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
