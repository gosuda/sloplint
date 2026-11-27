"""Restore the argv observer after the toolchain's documented test PATH prepend."""
from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path


def main() -> None:
    if len(sys.argv) < 2:
        raise RuntimeError("test launcher omitted the real test binary")
    binary = Path(sys.argv[1]).resolve(strict=True)
    if not binary.is_file():
        raise RuntimeError("test launcher target is not a regular file")
    observer = Path(os.environ["SPIRA_ARGV_OBSERVER_DIR"]).resolve(strict=True)
    incoming = os.environ["PATH"]
    record = {
        "argv": sys.argv[1:], "cwd": str(Path.cwd()),
        "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "incoming_first_path": incoming.split(os.pathsep)[0],
        "observer_path": str(observer), "pid": os.getpid(),
    }
    data = (json.dumps(record, separators=(",", ":")) + "\n").encode()
    if len(data) > 4096:
        raise RuntimeError("test launcher evidence exceeds its record bound")
    descriptor = os.open(os.environ["SPIRA_TEST_PROCESS_ARGV"], os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    try:
        if os.write(descriptor, data) != len(data):
            raise RuntimeError("test launcher evidence write was incomplete")
    finally:
        os.close(descriptor)
    environment = os.environ.copy()
    environment["PATH"] = str(observer) + os.pathsep + incoming
    os.execve(binary, [str(binary), *sys.argv[2:]], environment)


if __name__ == "__main__":
    main()
