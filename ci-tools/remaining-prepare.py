from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
EVIDENCE = WORKSPACE / "evidence"


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=SOURCE, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=check)


def publish(label: str, value: dict[str, object]) -> None:
    text = json.dumps(value, ensure_ascii=True)
    (EVIDENCE / f"{label}.json").write_text(text + "\n", encoding="utf-8")
    chunks = [text[index:index + 12000] for index in range(0, len(text), 12000)]
    for index, chunk in enumerate(chunks):
        message = json.dumps({"index": index, "total": len(chunks), "data": chunk})
        print(f"::notice title=remaining-prep-{label}-{index}::" + message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A"), flush=True)


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    mode = sys.argv[1]
    if mode == "oracle":
        assert git("rev-parse", "HEAD").stdout.strip() == "3d6bc75bf7f4c00f9a587727c1d8748894524520"
        package = SOURCE / "tools/upstream-oracle/package.json"
        before = package.read_text()
        assert before.count('"@dagrejs/dagre": "3.1.4"') == 1
        package.write_text(before.replace('"@dagrejs/dagre": "3.1.4"', '"@dagrejs/dagre": "3.1.3"'))
        command = ["pnpm", "--dir", "tools/upstream-oracle", "install", "--lockfile-only", "--ignore-scripts"]
        with (EVIDENCE / "oracle-lock.log").open("w") as log:
            result = subprocess.run(command, cwd=SOURCE, stdout=log, stderr=subprocess.STDOUT, check=False, timeout=600)
        if result.returncode:
            print((EVIDENCE / "oracle-lock.log").read_text()[-10000:], flush=True)
            return result.returncode
        paths = ("tools/upstream-oracle/package.json", "tools/upstream-oracle/pnpm-lock.yaml")
        files = {path: (SOURCE / path).read_text() for path in paths}
        publish("oracle", {"head": git("rev-parse", "HEAD").stdout.strip(), "files": files, "sha256": {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in paths}})
        return 0
    if mode == "ssh":
        head = "1dd8ea93aad5a9a1543db75d05c3609ac0dcdc3e"
        base = "179d41e6e32c79fe2619b1f718d560edb2a8c33a"
        assert git("rev-parse", "HEAD").stdout.strip() == head
        git("fetch", "--no-tags", "https://github.com/gosuda/ssh-chat.git", base)
        git("config", "user.name", "CI Probe")
        git("config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
        merge = git("-c", "merge.conflictstyle=diff3", "merge", "--no-commit", "--no-ff", base, check=False)
        (EVIDENCE / "merge.log").write_text(merge.stdout)
        conflicts = git("diff", "--name-only", "--diff-filter=U").stdout.splitlines()
        files = {path: (SOURCE / path).read_text() if (SOURCE / path).is_file() else None for path in conflicts}
        publish("ssh", {"head": head, "base": base, "exit": merge.returncode, "merge_log": merge.stdout, "status": git("status", "--porcelain=v1").stdout, "conflicts": files})
        return 0 if merge.returncode in (0, 1) else merge.returncode
    raise ValueError(f"unknown preparation {mode}")


if __name__ == "__main__":
    raise SystemExit(main())
