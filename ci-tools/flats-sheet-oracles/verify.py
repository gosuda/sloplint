from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import TypedDict, cast

class Source(TypedDict):
    name: str
    head: str
    tree: str

proof = Path(__file__).resolve().parent
workspace = Path(os.environ["GITHUB_WORKSPACE"])
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-sheet-oracles"
evidence.mkdir(exist_ok=True)
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", GOMAXPROCS="4", GOFLAGS="-p=4", PYTHONDONTWRITEBYTECODE="1")
environment["TMPDIR"] = "/tmp/fso"
Path(environment["TMPDIR"]).mkdir(exist_ok=True)
results: list[dict[str, object]] = []
for name, digest in cast(dict[str, str], manifest["proof_sha256"]).items():
    assert hashlib.sha256((proof / name).read_bytes()).hexdigest() == digest

def output(args: list[str], root: Path) -> str:
    return subprocess.check_output(args, cwd=root, env=environment, text=True).strip()

def identity(root: Path, source: Source) -> None:
    assert output(["git", "rev-parse", "HEAD"], root) == source["head"]
    assert output(["git", "write-tree"], root) == source["tree"]
    assert output(["git", "status", "--porcelain", "--untracked-files=all"], root) == ""
    assert output(["go", "env", "GOCACHEPROG"], root) == ""

def run(args: list[str], root: Path, name: str, timeout: int) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=root, env=environment, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=timeout, check=False)
    (evidence / (name + ".log")).write_text(result.stdout)
    print(f"GATE_EXIT {name} {result.returncode}\n{result.stdout[-18000:]}", flush=True)
    results.append({"name": name, "args": args, "exit": result.returncode,
                    "log_sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")
    return result

for source in cast(list[Source], manifest["sources"]):
    root = workspace / source["name"]
    identity(root, source)
    environment["GOCACHE"] = str(evidence / (source["name"] + "-go-cache"))
    assert not Path(environment["GOCACHE"]).exists()
    target = root / "internal/sheet/first_commit_test.go"
    original = target.read_bytes()
    selected = proof / (source["name"] + "-race.go")
    hashes = cast(dict[str, str], manifest["proof_sha256"])
    assert hashlib.sha256(selected.read_bytes()).hexdigest() == hashes[selected.name]
    try:
        target.write_bytes(selected.read_bytes())
        formatted = subprocess.run(["gofmt", "-w", str(target)], cwd=root, env=environment, check=False)
        assert formatted.returncode == 0
        result = run(["go", "test", "-race", "-count=1", "-timeout=25m",
                      "-run", "^TestFirstCommitKeepsAFormatUnderAReusedStyleID$", "-v", "./internal/sheet"],
                     root, source["name"] + "-maintenance-overlap", 1600)
        assert "REAL_WRITER_MAINTENANCE_COMMIT_REACHED" in result.stdout
        if source["name"] == "old":
            assert result.returncode == 1 and "WARNING: DATA RACE" in result.stdout
            assert "(*Instance).commit" in result.stdout and "(*Instance).maintain" in result.stdout
        else:
            assert result.returncode == 0 and "WARNING: DATA RACE" not in result.stdout
    finally:
        target.write_bytes(original)
        identity(root, source)
    if source["name"] == "candidate":
        assert run(["go", "test", "-race", "-count=1", "-timeout=25m",
                    "-run", "^TestFirstCommitKeepsAFormatUnderAReusedStyleID$", "-v", "./internal/sheet"],
                   root, "unchanged-first-commit", 1600).returncode == 0
        binary = evidence / "flats"
        environment["CGO_ENABLED"] = "0"
        assert run(["go", "build", "-buildvcs=true", "-o", str(binary), "./cmd/flats"],
                   root, "product-build", 1200).returncode == 0
        assert run(["uv", "run", "--no-project", "--python", "3.15", str(proof / "lifecycle-controls.py"),
                    str(root), str(binary), str(evidence / "lifecycle-controls")],
                   root, "lifecycle-controls", 900).returncode == 0
        assert run(["uv", "run", "--no-project", "--python", "3.15", "-m", "unittest", "discover",
                    "-s", "internal/lifecyclecheck", "-p", "test_*.py"],
                   root, "existing-lifecycle-tests", 300).returncode == 0
        assert run(["bash", "scripts/lifecycle-gate.sh", "--only", "approval-boundary-census"],
                   root, "actual-lifecycle-gate", 2400).returncode == 0
        identity(root, source)
print("SHEET_ORACLE_DEVELOPMENT_GATES_PASS", flush=True)
