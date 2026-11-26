from __future__ import annotations

from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import cast

import yaml

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "probe/ci-tools"
EVIDENCE = WORKSPACE / "evidence"
PATHS = [".github/workflows/ci.yml", ".github/workflows/pages.yml", ".github/workflows/verify.yml", ".depot/workflows/ci.yml"]
OLD_GUARD = 'test -z "$(go env GOCACHEPROG)"'
NEW_ASSIGN = 'effective_gocacheprog="$(go env GOCACHEPROG)"'
NEW_GUARD = 'test -z "$effective_gocacheprog"'
EXPORT = "export GOENV=off GOCACHEPROG=''"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def load_workflows() -> dict[str, dict[str, object]]:
    return {path: cast(dict[str, object], yaml.safe_load((SOURCE / path).read_text())) for path in PATHS}


def scripts(documents: dict[str, dict[str, object]]) -> dict[str, str]:
    result: dict[str, str] = {}
    for path, document in documents.items():
        jobs = cast(dict[str, dict[str, object]], document["jobs"])
        for job_name, job in jobs.items():
            for index, step in enumerate(cast(list[dict[str, object]], job["steps"])):
                if isinstance(step.get("run"), str):
                    key = f"{path}:{job_name}:{index}:{step.get('name', '')}"
                    result[key] = cast(str, step["run"])
    return result


def normalize(documents: dict[str, dict[str, object]]) -> dict[str, dict[str, object]]:
    result = deepcopy(documents)
    for document in result.values():
        jobs = cast(dict[str, dict[str, object]], document["jobs"])
        for job in jobs.values():
            for step in cast(list[dict[str, object]], job["steps"]):
                script = step.get("run")
                if isinstance(script, str):
                    step["run"] = "\n".join(line for line in script.splitlines() if line.strip() not in (EXPORT, OLD_GUARD, NEW_ASSIGN, NEW_GUARD)).strip()
    return result


def simulate(key: str, script: str, phase: str, go_status: int, go_stdout: str) -> dict[str, object]:
    identity = hashlib.sha256(f"{key}:{phase}".encode()).hexdigest()[:16]
    root = EVIDENCE / identity
    root.mkdir()
    go_log = root / "go.txt"
    gate_log = root / "just.txt"
    program = root / "step.sh"
    prefix = """go() {
    printf '%s\\n' "$*" >> "$ELEGANT_GO_LOG"
    printf '%s' "$ELEGANT_GO_STDOUT"
    return "$ELEGANT_GO_STATUS"
}
just() {
    printf '%s\\n' "$*" >> "$ELEGANT_GATE_LOG"
}
"""
    program.write_text(prefix + script + "\n")
    environment = dict(os.environ)
    environment.update(ELEGANT_GO_LOG=str(go_log), ELEGANT_GATE_LOG=str(gate_log), ELEGANT_GO_STDOUT=go_stdout, ELEGANT_GO_STATUS=str(go_status))
    result = subprocess.run(["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", str(program)], cwd=SOURCE, env=environment, text=True, capture_output=True, check=False, timeout=30)
    record: dict[str, object] = {"step": key, "phase": phase, "exit": result.returncode, "go_called": go_log.exists(), "gate_called": gate_log.exists(), "gate_commands": gate_log.read_text().splitlines() if gate_log.exists() else [], "stdout": result.stdout, "stderr": result.stderr}
    with (EVIDENCE / "controls.jsonl").open("a") as output:
        output.write(json.dumps(record) + "\n")
    print("CONTROL " + json.dumps(record), flush=True)
    return record


def main() -> None:
    EVIDENCE.mkdir()
    manifest = cast(dict[str, object], json.loads((PROOF / "remaining-elegant-guard-candidates.json").read_text()))
    cases = cast(dict[str, dict[str, str]], manifest["cases"])
    candidate = cases[sys.argv[1]]
    assert git("rev-parse", "HEAD") == candidate["head"]
    assert git("status", "--porcelain") == ""
    if candidate.get("initial_patch"):
        initial = PROOF / candidate["initial_patch"]
        assert hashlib.sha256(initial.read_bytes()).hexdigest() == candidate["initial_patch_sha256"]
        git("apply", "--index", "--whitespace=error", str(initial))
    assert git("write-tree") == candidate["old_tree"]
    before = load_workflows()
    previous = scripts(before)
    patch = PROOF / str(manifest["patch"])
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == candidate["tree"]
    git("diff", "--cached", "--check")
    after = load_workflows()
    current = scripts(after)
    assert normalize(before) == normalize(after), "workflow behavior changed beyond the reviewed preflight correction"
    guarded = {key: body for key, body in current.items() if NEW_ASSIGN in body}
    binary = {key: body for key, body in current.items() if OLD_GUARD in previous[key] and NEW_ASSIGN not in body}
    assert len(guarded) == 16 and len(binary) == 7, (len(guarded), len(binary))
    for key, body in guarded.items():
        assert OLD_GUARD not in body
        for phase, status, stdout, expected, gate in [
            ("candidate-empty", 0, "", 0, True),
            ("candidate-query-failure", 42, "", 42, False),
            ("candidate-nonempty", 0, "unexpected-cache-helper", 1, False),
        ]:
            result = simulate(key, body, phase, status, stdout)
            assert result["exit"] == expected and result["gate_called"] == gate and result["go_called"], result
        old = simulate(key, previous[key], "old-query-failure-is-masked", 42, "")
        assert old["exit"] == 0 and old["gate_called"] and old["go_called"], old
    for key, body in binary.items():
        assert "go env" not in body and EXPORT not in body
        result = simulate(key, body, "candidate-binary-independence", 42, "unexpected-cache-helper")
        assert result["exit"] == 0 and result["gate_called"] and not result["go_called"], result
        old = simulate(key, previous[key], "old-binary-preflight-blocks", 0, "unexpected-cache-helper")
        assert old["exit"] == 1 and not old["gate_called"] and old["go_called"], old
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    config = EVIDENCE / "actionlint.yaml"
    config.write_text("self-hosted-runner:\n  labels: [depot-ubuntu-24.04, depot-ubuntu-24.04-8, depot-windows-2025]\n")
    lint = subprocess.run(["go", "run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.7", "-config-file", str(config), "-shellcheck=", "-pyflakes=", *PATHS], cwd=SOURCE, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=300)
    (EVIDENCE / "actionlint.log").write_text(lint.stdout)
    print(f"RESULT actionlint: {lint.returncode}", flush=True)
    assert lint.returncode == 0, lint.stdout
    assert git("diff", "--exit-code") == ""
    assert git("write-tree") == candidate["tree"]
    assert git("ls-files", "--others", "--exclude-standard") == ""
    files = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in PATHS}
    summary = {"candidate": candidate, "tree": git("write-tree"), "files": files, "guarded_steps": len(guarded), "binary_only_steps": len(binary), "controls": 78, "actionlint": lint.returncode, "worktree_clean": True}
    (EVIDENCE / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("GUARD_PROOF " + json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
