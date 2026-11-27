"""Record real logos subprocess argv without replacing its compiler or output."""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def main() -> None:
    real = Path(os.environ["SPIRA_REAL_LOGOS"]).resolve(strict=True)
    record_path = Path(os.environ["SPIRA_LOGOS_ARGV"])
    module_path = Path.cwd() / "go.mod"
    module = ""
    if module_path.is_file():
        module = next((line for line in module_path.read_text(encoding="utf-8").splitlines()
                       if line.startswith("module ")), "")
    record = {"argv": sys.argv[1:], "cwd": str(Path.cwd()), "module": module, "pid": os.getpid()}
    data = (json.dumps(record, separators=(",", ":")) + "\n").encode()
    if len(data) > 4096:
        raise RuntimeError("argv evidence exceeds the frozen record bound")
    descriptor = os.open(record_path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    try:
        if os.write(descriptor, data) != len(data):
            raise RuntimeError("argv evidence write was incomplete")
    finally:
        os.close(descriptor)
    os.execve(real, [str(real), *sys.argv[1:]], os.environ.copy())


if __name__ == "__main__":
    main()
