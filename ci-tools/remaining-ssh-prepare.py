from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROBE = WORKSPACE / "probe"
EVIDENCE = WORKSPACE / "evidence"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def main() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((PROBE / "ci-tools/remaining-ssh-draft.json").read_text())
    assert git("rev-parse", "HEAD") == manifest["head"]
    patch = PROBE / "ci-tools" / manifest["patch"]
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    git("apply", "--index", str(patch))
    assert git("write-tree") == manifest["tree"]
    (EVIDENCE / "draft-identity.json").write_text(json.dumps(manifest, indent=2) + "\n")
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    paths = git("ls-files", "*.go").splitlines()
    subprocess.run(["gofmt", "-w", *paths], cwd=SOURCE, check=True, timeout=60)
    with (EVIDENCE / "go-mod-tidy.log").open("w") as output:
        result = subprocess.run(["go", "mod", "tidy"], cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, check=False, timeout=600)
    print(f"TIDY_RESULT {result.returncode}", flush=True)
    if result.returncode:
        print((EVIDENCE / "go-mod-tidy.log").read_text()[-10000:], flush=True)
        raise RuntimeError("go mod tidy failed")
    git("add", "--update")
    assert git("ls-files", "--others", "--exclude-standard") == ""
    git("diff", "--cached", "--check")
    changed = git("diff", "--cached", "--name-only").splitlines()
    files = {path: (SOURCE / path).read_text() if (SOURCE / path).is_file() else None for path in changed}
    value = {"head": manifest["head"], "base": manifest["base_sha"], "draft_tree": manifest["tree"], "tree": git("write-tree"), "files": files, "sha256": {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in changed if (SOURCE / path).is_file()}}
    payload = json.dumps(value, ensure_ascii=True)
    (EVIDENCE / "ssh-prepared.json").write_text(payload + "\n")
    (EVIDENCE / "ssh-prepared.patch").write_text(subprocess.check_output(["git", "diff", "--cached", "--binary"], cwd=SOURCE, text=True))
    chunks = [payload[index:index + 1000] for index in range(0, len(payload), 1000)]
    for index, chunk in enumerate(chunks):
        print("PAYLOAD_CHUNK " + json.dumps({"label": "ssh-prepared", "index": index, "total": len(chunks), "data": chunk}), flush=True)


if __name__ == "__main__":
    main()
