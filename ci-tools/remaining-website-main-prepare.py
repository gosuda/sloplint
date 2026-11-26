from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
EVIDENCE = WORKSPACE / "evidence"
HEAD = "f2082ac85d2c0e12e92a9b8a1989cc4aee36c910"
DOCS = (
    "root/blog/docker-in-docker-inside-gvisor-with-gvx.md",
    "root/blog/fast-docker-environments-for-agent-rl-rollouts.md",
    "root/blog/run-docker-without-root-or-userns-gvx.md",
    "root/blog/gvx-cross-platform-linux-containers.md",
)


def run(args: list[str], name: str) -> None:
    with (EVIDENCE / f"{name}.log").open("w", encoding="utf-8") as output:
        result = subprocess.run(args, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, timeout=600, check=False)
    print(f"PREP_RESULT {name}: {result.returncode}", flush=True)
    if result.returncode:
        print((EVIDENCE / f"{name}.log").read_text()[-10000:], flush=True)
        raise RuntimeError(f"{name} failed with {result.returncode}")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def main() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    assert git("rev-parse", "HEAD") == HEAD
    originals = {relative: (SOURCE / relative).read_text() for relative in DOCS}
    for relative in DOCS[:3]:
        path = SOURCE / relative
        lines = path.read_text().splitlines()
        changed = 0
        for index in range(1, len(lines) - 1):
            if lines[index] == "" and lines[index - 1].startswith(">") and lines[index + 1].startswith(">"):
                lines[index] = ">"
                changed += 1
        assert changed <= 1, (relative, changed)
        path.write_text("\n".join(lines) + "\n")
    run(["npm", "ci"], "install")
    run(["npx", "--no-install", "prettier", "--write", *DOCS], "format")
    changed_paths = git("diff", "--name-only").splitlines()
    assert all(path in DOCS for path in changed_paths), changed_paths
    for relative, original in originals.items():
        def normalized(value: str) -> str:
            return "".join("".join(line.split()) for line in value.splitlines() if line.strip() != ">")
        assert normalized(original) == normalized((SOURCE / relative).read_text()), relative
    assert git("ls-files", "--others", "--exclude-standard") == ""
    files = {path: (SOURCE / path).read_text() for path in changed_paths}
    payload = json.dumps({"head": HEAD, "files": files, "sha256": {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in changed_paths}}, ensure_ascii=True)
    (EVIDENCE / "website-main-prepared.json").write_text(payload + "\n")
    (EVIDENCE / "website-main-prepared.patch").write_text(subprocess.check_output(["git", "diff", "--binary"], cwd=SOURCE, text=True))
    chunks = [payload[index:index + 1000] for index in range(0, len(payload), 1000)]
    for index, chunk in enumerate(chunks):
        print("PAYLOAD_CHUNK " + json.dumps({"label": "website-main", "index": index, "total": len(chunks), "data": chunk}), flush=True)


if __name__ == "__main__":
    main()
