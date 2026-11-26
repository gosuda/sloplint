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


def run(command: list[str], name: str, timeout: int = 600) -> int:
    path = EVIDENCE / f"{name}.log"
    with path.open("w", encoding="utf-8") as output:
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
    print(f"RESULT {name}: {code}", flush=True)
    if code:
        print(path.read_text(encoding="utf-8", errors="replace")[-12000:], flush=True)
    return code


def control(path: str, edits: list[list[str]], test: str, marker: str, name: str, package: str) -> dict[str, int]:
    source = SOURCE / path
    original = source.read_bytes()
    text = original.decode()
    for before, after in edits:
        assert text.count(before) == 1, (name, before)
        text = text.replace(before, after)
    try:
        source.write_text(text)
        code = run(["go", "test", "-count=1", "-timeout", "120s", "-run", test, package], name)
        log = (EVIDENCE / f"{name}.log").read_bytes()
        expected = code != 0 and code != 124 and marker.encode("utf-8") in log
        return {"observed_exit": code, "expected_rejection": 0 if expected else 1}
    finally:
        source.write_bytes(original)
        assert hashlib.sha256(source.read_bytes()).hexdigest() == hashlib.sha256(original).hexdigest()


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    candidate = json.loads((PROBE / "ci-tools/remaining-ssh-empty-candidate.json").read_text())
    assert git("rev-parse", "HEAD") == candidate["head"]
    patch = PROBE / "ci-tools" / candidate["patch"]
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == candidate["patch_sha256"]
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == candidate["tree"]
    git("diff", "--cached", "--check")
    changed = git("diff", "--cached", "--name-only").splitlines()
    files = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() if (SOURCE / path).is_file() else None for path in changed}
    expected = {entry["path"]: entry["sha256"] for entry in candidate["files"]}
    assert files == expected
    (EVIDENCE / "identity.json").write_text(json.dumps({"candidate": candidate, "files": files}, indent=2) + "\n")
    print("IDENTITY " + json.dumps({"head": candidate["head"], "base": candidate["base_sha"], "tree": candidate["tree"], "patch_sha256": candidate["patch_sha256"]}), flush=True)
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    (EVIDENCE / "go-env.txt").write_text(subprocess.check_output(["go", "env", "GOENV", "GOCACHEPROG", "GOVERSION"], cwd=SOURCE, text=True))
    gofiles = git("ls-files", "*.go").splitlines()
    results = {"gofmt": run(["gofmt", "-l", *gofiles], "gofmt", 60)}
    if (EVIDENCE / "gofmt.log").read_text().strip():
        results["gofmt"] = 1
    results["generator"] = run([sys.executable, str(PROBE / "ci-tools/remaining-ssh-generator.py")], "generator")
    results["module-consistency"] = run(["go", "mod", "tidy", "-diff"], "module-consistency")
    results["vet"] = run(["go", "vet", "./..."], "vet")
    results["race-tests"] = run(["go", "test", "-race", "-count=1", "-timeout", "120s", "-v", "./..."], "race-tests")
    binary = EVIDENCE / "ssh-chat-server"
    results["build"] = run(["go", "build", "-trimpath", "-o", str(binary), "."], "build")
    results["cli-help"] = run([str(binary), "--help"], "cli-help", 15) if results["build"] == 0 else 1
    cases = json.loads((PROBE / "ci-tools/remaining-ssh-empty-controls.json").read_text())
    controls = {case["name"]: control(case["path"], case["edits"], case["test"], case["marker"], case["name"] + "-negative-control", case["package"]) for case in cases}
    for name, observed in controls.items():
        results[f"{name}-control"] = observed["expected_rejection"]
    results["worktree-clean"] = run(["git", "diff", "--exit-code"], "worktree-clean", 30)
    results["candidate-tree"] = 0 if git("write-tree") == candidate["tree"] else 1
    (EVIDENCE / "controls.json").write_text(json.dumps(controls, indent=2) + "\n")
    (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print("CONTROL_RESULTS " + json.dumps(controls, sort_keys=True), flush=True)
    print("GATE_RESULTS " + json.dumps(results, sort_keys=True), flush=True)
    return 0 if all(code == 0 for code in results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
