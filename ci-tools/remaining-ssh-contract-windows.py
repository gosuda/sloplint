from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROBE = WORKSPACE / "probe"
EVIDENCE = WORKSPACE / "evidence"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def run(command: list[str], name: str, timeout: int = 900) -> int:
    path = EVIDENCE / f"{name}.log"
    with path.open("w", encoding="utf-8") as output:
        result = subprocess.run(command, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, timeout=timeout, check=False)
    print(f"RESULT {name}: {result.returncode}", flush=True)
    if result.returncode:
        print(path.read_text(encoding="utf-8", errors="replace")[-10000:], flush=True)
    return result.returncode


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    candidate = json.loads((PROBE / "ci-tools/remaining-ssh-contract-candidate.json").read_text())
    assert git("rev-parse", "HEAD") == candidate["head"]
    patch = PROBE / "ci-tools" / candidate["patch"]
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == candidate["patch_sha256"]
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == candidate["tree"]
    changed = git("diff", "--cached", "--name-only").splitlines()
    files = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in changed}
    assert files == {entry["path"]: entry["sha256"] for entry in candidate["files"]}
    (EVIDENCE / "identity.json").write_text(json.dumps({"candidate": candidate, "files": files}, indent=2) + "\n")
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    results = {"generator": run([sys.executable, str(PROBE / "ci-tools/remaining-ssh-generator.py")], "generator")}
    results["generated-package"] = run(["go", "test", "./proto"], "generated-package")
    results["worktree-clean"] = run(["git", "diff", "--exit-code"], "worktree-clean", 30)
    results["candidate-tree"] = 0 if git("write-tree") == candidate["tree"] else 1
    (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print("GATE_RESULTS " + json.dumps(results, sort_keys=True), flush=True)
    return 0 if all(code == 0 for code in results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
