from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time
from typing import cast

workspace = Path(os.environ["GITHUB_WORKSPACE"])
source = workspace / "source"
proof = workspace / "proof/ci-tools/flats-retention"
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-retention"
evidence.mkdir()
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
authored = cast(list[dict[str, str]], json.loads((proof / "candidate-elements.json").read_text()))
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", GOTOOLCHAIN="local", GOMAXPROCS="2",
                   GOFLAGS="-p=2", NOTEBOOK_LARGE_MEMORY_TESTS="0")
results: list[dict[str, object]] = []
reader = source / "internal/notebook/reader_entry_test.go"
injection = source / "internal/notebook/retention_proof_test.go"

def output(args: list[str], env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=source, env=environment if env is None else env, text=True).strip()

def run(name: str, args: list[str], expected: int | None = 0, cwd: Path = source,
        env: dict[str, str] | None = None) -> str:
    started = time.monotonic()
    result = subprocess.run(args, cwd=cwd, env=environment if env is None else env,
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            timeout=2400, check=False)
    (evidence / f"{name}.log").write_text(result.stdout)
    results.append({"name": name, "args": args, "cwd": str(cwd), "expected": expected,
                    "exit": result.returncode, "seconds": time.monotonic() - started,
                    "sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")
    print(f"RETENTION_GATE {name} exit={result.returncode}\n{result.stdout[-18000:]}", flush=True)
    if expected is None:
        assert result.returncode in (0, 1), (name, result.returncode)
    else:
        assert result.returncode == expected, (name, result.returncode, expected)
    return result.stdout

def original_identity() -> None:
    assert output(["git", "rev-parse", "HEAD"]) == manifest["head"]
    assert output(["git", "write-tree"]) == manifest["tree"]
    assert output(["git", "status", "--porcelain", "--untracked-files=all"]) == ""
    for path, digest in cast(dict[str, str], manifest["source_sha256"]).items():
        assert hashlib.sha256((source / path).read_bytes()).hexdigest() == digest, path

def build_fault_binary(name: str) -> Path:
    saved = reader.read_bytes()
    text = saved.decode()
    start = text.index("func measureReaderLiveHeap")
    stop = text.index("\nfunc ", start + 5)
    section = text[start:stop]
    call = "body, err := in.Read(t.Context(), query, false)"
    assert section.count(call) == 1
    section = section.replace(call, "body, err := retentionProbeRead(t, in, query)")
    path = evidence / f"{name}.test"
    try:
        reader.write_text(text[:start] + section + text[stop:])
        injection.write_bytes((proof / "retention_proof_test.go.txt").read_bytes())
        run(f"{name}-fault-format", ["gofmt", "-w", str(reader), str(injection)])
        run(f"{name}-fault-build", ["go", "test", "-c", "-o", str(path), "./internal/notebook"])
    finally:
        reader.write_bytes(saved)
        injection.unlink(missing_ok=True)
    return path

for path, digest in cast(dict[str, str], manifest["proof_sha256"]).items():
    assert hashlib.sha256((proof / path).read_bytes()).hexdigest() == digest, path
original_identity()
assert os.cpu_count() == 2, os.cpu_count()
print("RETENTION_ENVIRONMENT " + json.dumps({
    "cpus": os.cpu_count(), "platform": platform.platform(),
    "mem_total": next(line for line in Path("/proc/meminfo").read_text().splitlines() if line.startswith("MemTotal:")),
    "source": manifest["head"], "tree": manifest["tree"],
}), flush=True)
run("load", ["uptime"])
run("go-version", ["go", "version"])
assert output(["go", "env", "GOCACHEPROG"]) == ""
run("baseline-clean-option-schema", ["go", "test", "-count=1", "-v", "-timeout=18m",
    "-run", "^TestMarkdownRetainedMemory$/^option_schema$", "./internal/notebook"])
original_identity()
original_binary = build_fault_binary("original")
original_identity()
for item in authored:
    (source / item["path"]).write_text(item["content"])
run("format-candidate", ["gofmt", "-w", *[item["path"] for item in authored if item["path"].endswith(".go")]])
paths = output(["git", "diff", "--name-only"]).splitlines()
assert set(paths) == {item["path"] for item in authored}, paths
index_env = dict(environment)
index_env["GIT_INDEX_FILE"] = str(evidence / "candidate.index")
output(["git", "read-tree", "HEAD"], index_env)
output(["git", "add", "--", *paths], index_env)
tree = output(["git", "write-tree"], index_env)
snapshot = {"base": manifest["head"], "tree": tree,
            "files": [{"path": path, "sha256": hashlib.sha256((source / path).read_bytes()).hexdigest()} for path in paths]}
(evidence / "candidate-snapshot.json").write_text(json.dumps(snapshot, indent=2) + "\n")
print("RETENTION_CANDIDATE_SNAPSHOT " + json.dumps(snapshot), flush=True)
for item in snapshot["files"]:
    print("RETENTION_CANDIDATE_FILE " + json.dumps({**item, "content": (source / item["path"]).read_text()}), flush=True)

def candidate_identity() -> None:
    assert output(["git", "rev-parse", "HEAD"]) == manifest["head"]
    assert output(["git", "write-tree"], index_env) == tree
    assert set(output(["git", "diff", "--name-only"]).splitlines()) == set(paths)
    assert output(["git", "ls-files", "--others", "--exclude-standard"]) == ""
    for item in snapshot["files"]:
        assert hashlib.sha256((source / item["path"]).read_bytes()).hexdigest() == item["sha256"], item["path"]

candidate_identity()
candidate_binary = build_fault_binary("candidate")
candidate_identity()
modes: list[tuple[str, int | None, int]] = [
    ("last", 0, 1), ("per_read", None, 1), ("first", 1, 1),
    ("middle", 1, 1), ("response", 1, 0),
]
for mode, before_exit, after_exit in modes:
    for label, binary, expected in (("original", original_binary, before_exit), ("candidate", candidate_binary, after_exit)):
        env = dict(environment)
        env["RETENTION_PROBE"] = mode
        log = run(f"{label}-{mode}", [str(binary), "-test.v", "-test.count=1", "-test.timeout=18m",
                  "-test.run=^TestMarkdownRetainedMemory$/^option_schema$/^1MiB_live$"],
                  expected, source / "internal/notebook", env)
        assert f"RETENTION_PROBE mode={mode} attempt=3" in log
        if expected == 1 or (expected is None and results[-1]["exit"] == 1):
            assert "--- FAIL: TestMarkdownRetainedMemory/option_schema/1MiB_live" in log
            assert "post-GC live heap rose" in log and "exceeds 1 MiB" in log
        else:
            assert "PASS" in log and "--- FAIL" not in log
candidate_identity()
run("candidate-clean-option-schema", ["go", "test", "-count=1", "-v", "-timeout=18m",
    "-run", "^TestMarkdownRetainedMemory$/^option_schema$", "./internal/notebook"])
run("vet", ["go", "vet", "./internal/notebook"])
run("notebook", ["go", "test", "-count=1", "-v", "-timeout=18m", "./internal/notebook"])
run("notebook-race-1", ["bash", "scripts/ci-race.sh", "notebook-1", "-count=1", "-v"])
run("notebook-race-2", ["bash", "scripts/ci-race.sh", "notebook-2", "-count=1", "-v"])
run("docs-audit", ["make", "docs-audit"])
run("docs-verify", ["make", "docs-verify"])
run("format-check", ["bash", "-c", "set -euo pipefail; test -z \"$(gofmt -l internal/notebook/markdown_read_memory_test.go internal/notebook/reader_entry_test.go)\""])
run("whitespace", ["git", "diff", "--check"])
candidate_identity()
print("RETENTION_IMPLEMENTATION_GATES_PASS " + tree, flush=True)
