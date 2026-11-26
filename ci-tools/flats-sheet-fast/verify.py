from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time
from typing import cast

proof = Path(__file__).resolve().parent
root = Path(os.environ["GITHUB_WORKSPACE"]) / "source"
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-sheet-fast"
evidence.mkdir()
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", PYTHONDONTWRITEBYTECODE="1")
for key, name in (("GOCACHE", "go-build"), ("GOMODCACHE", "go-modules")):
    cache = evidence / name
    assert not cache.exists()
    environment[key] = str(cache)

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()

def identity() -> None:
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("write-tree") == manifest["tree"]
    assert git("status", "--porcelain", "--untracked-files=all") == ""
    for item in cast(list[dict[str, str]], manifest["files"]):
        assert hashlib.sha256((root / item["path"]).read_bytes()).hexdigest() == item["sha256"]

@dataclass(frozen=True)
class Command:
    name: str
    script: str

python = shlex.quote(sys.executable)
commands = (
    Command("go-format", 'files=$(gofmt -l internal/app/backupcmd_test.go internal/sheet/first_commit_test.go)\nif [ -n "$files" ]; then printf "%s\\n" "$files"; exit 1; fi'),
    Command("ci-helpers", python + " -B -m unittest discover -s scripts -p 'test_ci_*.py'"),
    Command("image-helpers", python + " -B -m unittest discover -s scripts -p test_image_commits.py"),
    Command("approval-helpers", python + " -B scripts/test_approval_gate.py"),
    Command("actionlint", "go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 -config-file " + shlex.quote(str(proof / "actionlint.yaml")) + " -shellcheck= -pyflakes= .github/workflows/ci.yml .github/workflows/notebook-memory-race.yml"),
)
for name, digest in cast(dict[str, str], manifest["proof_sha256"]).items():
    assert hashlib.sha256((proof / name).read_bytes()).hexdigest() == digest
identity()
results: list[dict[str, object]] = []
try:
    for command in commands:
        started = time.monotonic()
        prefix = 'export GOENV=off GOCACHEPROG=""; test -z "$(go env GOCACHEPROG)"\n'
        result = subprocess.run(["bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", prefix + command.script],
                                cwd=root, env=environment, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=600, check=False)
        log = evidence / (command.name + ".log")
        log.write_text(result.stdout)
        results.append({"name": command.name, "exit": result.returncode, "seconds": time.monotonic() - started,
                        "command": command.script, "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
        print(f"FAST_GATE_EXIT {command.name} {result.returncode}\n{result.stdout}", flush=True)
        (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results,
                                                         "cpu_count": os.cpu_count()}, indent=2) + "\n")
finally:
    identity()
assert all(result["exit"] == 0 for result in results)
print("SHEET_FAST_CORRECTION_PASS", flush=True)
