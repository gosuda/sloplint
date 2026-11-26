from __future__ import annotations

import hashlib
import json
import os
import signal
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
    log = EVIDENCE / f"{name}.log"
    with log.open("w", encoding="utf-8") as output:
        process = subprocess.Popen(command, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            code = 124
            output.write(f"\nExceeded {timeout} seconds.\n")
    print(f"RESULT {name}: {code}", flush=True)
    if code:
        print(log.read_text(encoding="utf-8", errors="replace")[-10000:], flush=True)
    return code


def main() -> int:
    label = sys.argv[1]
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    records: object = json.loads((PROBE / "ci-tools/remaining-candidates.json").read_text())
    if not isinstance(records, list):
        raise TypeError("candidate manifest must be a list")
    matches = [record for record in records if isinstance(record, dict) and record.get("label") == label]
    if len(matches) != 1:
        raise ValueError(f"unknown or duplicate candidate {label}")
    candidate = matches[0]
    for key in ("head", "tree", "patch", "patch_sha256"):
        if not isinstance(candidate.get(key), str):
            raise TypeError(f"candidate {key} must be a string")
    patch = PROBE / "ci-tools" / candidate["patch"]
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == candidate["patch_sha256"]
    assert git("rev-parse", "HEAD") == candidate["head"]
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == candidate["tree"]
    git("diff", "--cached", "--check")
    files = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in git("diff", "--cached", "--name-only").splitlines()}
    identity = {"candidate": candidate, "files": files}
    (EVIDENCE / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print("IDENTITY " + json.dumps(identity, sort_keys=True), flush=True)
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    (EVIDENCE / "go-env.txt").write_text(subprocess.check_output(["go", "env", "GOENV", "GOCACHEPROG", "GOVERSION"], cwd=SOURCE, text=True))
    config = EVIDENCE / "actionlint.yaml"
    config.write_text("self-hosted-runner:\n  labels: [depot-ubuntu-24.04]\n")
    commands: tuple[tuple[str, list[str]], ...] = (
        ("workflow-lint", ["go", "run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.7", "-config-file", str(config), "-shellcheck=", "-pyflakes=", ".github/workflows/pr-gate.yml", ".github/workflows/update.yml"]),
        ("install", ["npm", "ci"]),
        ("format", ["npm", "run", "format:check"]),
        ("markdown-lint", ["npm", "run", "lint:md"]),
        ("go-tests", ["go", "test", "-count=1", "-v", "./..."]),
        ("translation-limit", ["env", "LLM_INIT=false", "go", "run", ".", "check_retranslate", "4"]),
    )
    results = {name: run(command, name) for name, command in commands}
    if candidate["number"] == 312:
        results["templ-regeneration"] = run(["go", "run", "github.com/a-h/templ/cmd/templ@v0.3.1118", "generate"], "templ-regeneration")
    results["worktree-clean"] = run(["git", "diff", "--exit-code"], "worktree-clean", 30)
    results["candidate-tree"] = 0 if git("write-tree") == candidate["tree"] else 1
    (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print("GATE_RESULTS " + json.dumps(results, sort_keys=True), flush=True)
    return 0 if all(code == 0 for code in results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
