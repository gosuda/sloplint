from __future__ import annotations

from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import cast

import yaml

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "probe/ci-tools"
EVIDENCE = WORKSPACE / "evidence"
PATHS = [".github/workflows/pr-gate.yml", ".github/workflows/update.yml"]
OLD_GUARD = 'test -z "$(go env GOCACHEPROG)"'
NEW_ASSIGN = 'effective_gocacheprog="$(go env GOCACHEPROG)"'
NEW_GUARD = 'test -z "$effective_gocacheprog"'
EXPORT = "export GOENV=off GOCACHEPROG=''"


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def workflows(root: Path) -> dict[str, dict[str, object]]:
    return {path: cast(dict[str, object], yaml.safe_load((root / path).read_text())) for path in PATHS}


def scripts(documents: dict[str, dict[str, object]]) -> dict[str, str]:
    result: dict[str, str] = {}
    for path, document in documents.items():
        for job_name, job in cast(dict[str, dict[str, object]], document["jobs"]).items():
            assert job["runs-on"] in ("depot-ubuntu-24.04", "depot-ubuntu-24.04-4", "depot-ubuntu-24.04-8")
            for index, step in enumerate(cast(list[dict[str, object]], job["steps"])):
                body = step.get("run")
                if isinstance(body, str):
                    result[f"{path}:{job_name}:{index}"] = body
    return result


def normalize(documents: dict[str, dict[str, object]]) -> dict[str, dict[str, object]]:
    result = deepcopy(documents)
    for document in result.values():
        for job in cast(dict[str, dict[str, object]], document["jobs"]).values():
            for step in cast(list[dict[str, object]], job["steps"]):
                body = step.get("run")
                if isinstance(body, str):
                    step["run"] = "\n".join(line for line in body.splitlines() if line.strip() not in (EXPORT, OLD_GUARD, NEW_ASSIGN, NEW_GUARD)).strip()
    return result


def simulate(label: str, body: str, phase: str, query_status: int, query_stdout: str) -> dict[str, object]:
    identity = hashlib.sha256(f"{label}:{phase}".encode()).hexdigest()[:16]
    root = EVIDENCE / "controls" / identity
    root.mkdir(parents=True)
    query_log = root / "query.txt"
    gate_log = root / "gate.txt"
    program = root / "step.sh"
    prefix = """go() {
    if test "$1" = env; then
        printf '%s\\n' "$*" >> "$WEBSITE_QUERY_LOG"
        printf '%s' "$WEBSITE_QUERY_STDOUT"
        return "$WEBSITE_QUERY_STATUS"
    fi
    printf 'go %s\\n' "$*" >> "$WEBSITE_GATE_LOG"
}
"""
    program.write_text(prefix + body + "\n")
    build = root / "build.sh"
    build.write_text('#!/usr/bin/env bash\nprintf "./build.sh\\n" >> "$WEBSITE_GATE_LOG"\n')
    build.chmod(0o755)
    environment = dict(os.environ)
    environment.update(WEBSITE_QUERY_LOG=str(query_log), WEBSITE_GATE_LOG=str(gate_log), WEBSITE_QUERY_STDOUT=query_stdout, WEBSITE_QUERY_STATUS=str(query_status))
    result = subprocess.run(["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", str(program)], cwd=root, env=environment, text=True, capture_output=True, check=False, timeout=30)
    record: dict[str, object] = {"step": label, "phase": phase, "exit": result.returncode, "query_called": query_log.exists(), "gate_called": gate_log.exists(), "gate_commands": gate_log.read_text().splitlines() if gate_log.exists() else [], "stdout": result.stdout, "stderr": result.stderr}
    with (EVIDENCE / "controls.jsonl").open("a") as output:
        output.write(json.dumps(record) + "\n")
    print("CONTROL " + json.dumps(record), flush=True)
    return record


def main() -> None:
    EVIDENCE.mkdir()
    manifest = cast(dict[str, object], json.loads((PROOF / "remaining-website-guard-candidates.json").read_text()))
    candidates = cast(list[dict[str, str | int]], manifest["candidates"])
    patch = PROOF / str(manifest["patch"])
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    assert git(SOURCE, "rev-parse", "HEAD") == candidates[0]["head"]
    assert git(SOURCE, "status", "--porcelain") == ""
    git(SOURCE, "fetch", "--depth=1", "origin", *(str(candidate["head"]) for candidate in candidates))
    roots: list[Path] = []
    records: list[dict[str, object]] = []
    for candidate in candidates:
        number = int(candidate["number"])
        root = WORKSPACE / f"candidate-{number}"
        git(SOURCE, "worktree", "add", "--detach", str(root), str(candidate["head"]))
        roots.append(root)
        assert git(root, "rev-parse", "HEAD^{tree}") == candidate["old_tree"]
        assert git(root, "status", "--porcelain") == ""
        for path, digest in cast(dict[str, str], manifest["old_files"]).items():
            assert hashlib.sha256((root / path).read_bytes()).hexdigest() == digest
        before = workflows(root)
        previous = scripts(before)
        git(root, "apply", "--index", "--whitespace=error", str(patch))
        assert git(root, "write-tree") == candidate["tree"]
        git(root, "diff", "--cached", "--check")
        assert set(git(root, "diff", "--cached", "--name-only").splitlines()) == set(PATHS)
        for path, digest in cast(dict[str, str], manifest["files"]).items():
            assert hashlib.sha256((root / path).read_bytes()).hexdigest() == digest
        after = workflows(root)
        current = scripts(after)
        assert normalize(before) == normalize(after), number
        guarded = {key: body for key, body in current.items() if NEW_ASSIGN in body}
        assert len(guarded) == 3
        for key, body in guarded.items():
            label = f"PR{number}:{key}"
            for phase, status, stdout, expected, gate in [
                ("candidate-empty", 0, "", 0, True),
                ("candidate-query-failure", 42, "", 42, False),
                ("candidate-nonempty", 0, "unexpected-cache-helper", 1, False),
            ]:
                result = simulate(label, body, phase, status, stdout)
                assert result["exit"] == expected and result["gate_called"] == gate and result["query_called"], result
            old = simulate(label, previous[key], "old-query-failure-is-masked", 42, "")
            assert old["exit"] == 0 and old["gate_called"] and old["query_called"], old
        records.append({"candidate": candidate, "tree": git(root, "write-tree"), "controls": 12, "other_workflow_fields_and_commands_unchanged": True})
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=roots[0], text=True).strip() == ""
    config = EVIDENCE / "actionlint.yaml"
    config.write_text("self-hosted-runner:\n  labels: [depot-ubuntu-24.04, depot-ubuntu-24.04-4, depot-ubuntu-24.04-8]\n")
    lint = subprocess.run(["go", "run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.7", "-config-file", str(config), "-shellcheck=", "-pyflakes=", *PATHS], cwd=roots[0], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=300)
    (EVIDENCE / "actionlint.log").write_text(lint.stdout)
    print(f"RESULT actionlint: {lint.returncode}", flush=True)
    assert lint.returncode == 0, lint.stdout
    for root, candidate in zip(roots, candidates, strict=True):
        assert git(root, "diff", "--exit-code") == ""
        assert git(root, "write-tree") == candidate["tree"]
        assert git(root, "ls-files", "--others", "--exclude-standard") == ""
    summary = {"candidates": records, "workflow_files": manifest["files"], "controls": 96, "actionlint": lint.returncode, "all_worktrees_clean": True}
    (EVIDENCE / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("WEBSITE_GUARD_PROOF " + json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
