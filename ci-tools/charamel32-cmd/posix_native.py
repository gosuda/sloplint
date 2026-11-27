"""Freeze benign native shell argument boundaries on unchanged Charamel."""
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


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, Any] = json.loads((ROOT / "posix-baseline.json").read_text("utf-8"))
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("rev-parse", "HEAD^{tree}") == manifest["tree"]
    for path, expected in manifest["source_sha256"].items():
        assert hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() == expected, path
    records: list[dict[str, Any]] = []
    failures: list[str] = []

    def observe(
        identity: str, args: list[str], cwd: Path,
        expected_exit: int | None = 0, expected_lines: list[str] | None = None,
    ) -> dict[str, Any]:
        started = time.monotonic()
        result = subprocess.run(
            args, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True,
            timeout=10, check=False,
        )
        stdout = result.stdout.decode("utf-8", errors="backslashreplace")
        stderr = result.stderr.decode("utf-8", errors="backslashreplace")
        accepted = (
            (expected_exit is None or result.returncode == expected_exit)
            and (expected_lines is None or stdout.splitlines() == expected_lines)
        )
        record: dict[str, Any] = {
            "id": identity, "argv": args, "actual_exit": result.returncode,
            "stdout": stdout, "stderr": stderr, "seconds": time.monotonic() - started,
            "expected_exit": expected_exit, "expected_lines": expected_lines,
            "accepted": accepted,
        }
        records.append(record)
        print("NATIVE_POSIX " + json.dumps(record, ensure_ascii=True), flush=True)
        if not accepted:
            failures.append(identity)
        return record

    with tempfile.TemporaryDirectory(prefix="charamel32-posix-") as temporary:
        cwd = Path(temporary)
        stub = cwd / "curl.exe"
        stub.write_text("#!/bin/sh\nprintf 'BENIGN_STUB\\n'\n", encoding="ascii")
        stub.chmod(0o755)
        helper = 'probe() { printf "%s\\n" "$#"; printf "<%s>\\n" "$@"; }; '
        setup = "set -- alpha beta; value=scalar; CHAR32_PREFIX_A=one; CHAR32_PREFIX_B=two; "
        for case in manifest["arity"]:
            observe(
                case["id"], ["/bin/bash", "--noprofile", "--norc", "-c",
                             helper + setup + "probe " + case["expression"]],
                cwd, 0, case["lines"],
            )
        observe(
            "dash_braced_at",
            ["/bin/dash", "-c", helper + 'set -- alpha beta; probe "' + chr(36) + '{@}"'],
            cwd, 0, ["2", "<alpha>", "<beta>"],
        )
        for shell in ("/bin/bash", "/bin/dash"):
            for switches in ("-pv", "-pV", "-vp", "-p -v", "-pp"):
                code = (
                    f"command {switches} /bin/echo CMD_EXEC"
                    if switches == "-pp" else f"command {switches} /bin/sh"
                )
                row = observe(f"lookup_{Path(shell).name}_{switches}", [shell, "-c", code], cwd)
                if switches == "-pp" and row["stdout"] != "CMD_EXEC\n":
                    failures.append(row["id"] + "_execution")
                if switches != "-pp" and "/bin/sh" not in row["stdout"]:
                    failures.append(row["id"] + "_lookup")
            observe(
                f"lookup_{Path(shell).name}_suffix", [shell, "-c", 'command -pv "$1"', "_", str(stub)],
                cwd, 0, [str(stub)],
            )
            observe(
                f"lookup_{Path(shell).name}_invalid", [shell, "-c", "command -v -z /bin/sh"],
                cwd, None,
            )
        env = shutil.which("env")
        if env is None:
            raise RuntimeError("GNU env is required")
        observe("env_version", [env, "--version"], cwd)
        for identity, options in (
            ("env_initial_dash", ["-"]),
            ("env_dash_after_i", ["-i", "-"]),
            ("env_dash_after_u", ["-u", "UNUSED", "-"]),
            ("env_dash_after_end", ["-i", "--", "-"]),
        ):
            observe(identity, [env, *options, str(stub)], cwd, 0, ["BENIGN_STUB"])
        observe("env_dash_ends_options", [env, "-", "-i", str(stub)], cwd, None)
        observe(
            "env_braced_fields",
            ["/bin/bash", "--noprofile", "--norc", "-c",
             'set -- alpha "$1"; env A="' + chr(36) + '{@}"', "_", str(stub)],
            cwd, 0, ["BENIGN_STUB"],
        )
        (cwd / "README").write_text("owned\n", encoding="ascii")
        (cwd / "README.md").write_text("owned\n", encoding="ascii")
        (cwd / "target").mkdir()
        (cwd / "target" / "README.md").write_text("owned\n", encoding="ascii")
        fd = shutil.which("fd") or shutil.which("fdfind")
        if fd is None:
            raise RuntimeError("fd is required")
        observe("fd_version", [fd, "--version"], cwd)
        for identity, separate, attached in manifest["fd_pairs"]:
            left = observe(identity + "_reference", [fd, *separate, "README"], cwd)
            right = observe(identity + "_attached", [fd, *attached, "README"], cwd)
            if sorted(left["stdout"].splitlines()) != sorted(right["stdout"].splitlines()):
                failures.append(identity + "_equivalence")
        for identity, args in (
            ("fd_attached_exec", ["-tf", "README", "-x", "/bin/echo", "FD_EXEC"]),
            ("fd_attached_batch", ["--type=f", "README", "--exec-batch", "/bin/echo", "FD_BATCH"]),
        ):
            row = observe(identity, [fd, *args], cwd)
            marker = "FD_BATCH" if identity.endswith("batch") else "FD_EXEC"
            if marker not in row["stdout"]:
                failures.append(identity + "_action")
    assert git("status", "--porcelain", "--untracked-files=no") == ""
    report = {
        "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "source": manifest["head"], "tree": manifest["tree"], "source_clean": True,
        "driver_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "manifest_sha256": hashlib.sha256((ROOT / "posix-baseline.json").read_bytes()).hexdigest(),
        "records": records, "failures": failures,
    }
    (OUT / "posix-baseline-result.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("NATIVE_POSIX_RESULT " + json.dumps(report, ensure_ascii=True), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
