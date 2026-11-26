from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

task = sys.argv[1]
mode = sys.argv[2]
root = Path.cwd()
probe = Path(__file__).resolve().parent
manifest = json.loads((probe / "manifest.json").read_text())[task]
evidence = root.parent / "evidence" / f"{task}-{mode}-{os.environ.get('PROOF_TARGET', 'native')}"
evidence.mkdir(parents=True, exist_ok=True)
results: dict[str, int] = {}

def output(command: list[str]) -> str:
    return subprocess.check_output(command, text=True, cwd=root).strip()

def run(label: str, command: list[str], expected: int = 0, needle: str = "") -> str:
    path = evidence / f"{label}.log"
    print(f"RUN {label}: {command}", flush=True)
    with path.open("w") as handle:
        process = subprocess.run(command, cwd=root, stdout=handle, stderr=subprocess.STDOUT, text=True, check=False)
    results[label] = process.returncode
    text = path.read_text()
    print(f"EXIT {label}: {process.returncode}", flush=True)
    print(text[-18000:], flush=True)
    valid = process.returncode == 0 if expected == 0 else process.returncode != 0
    if not valid or (needle and needle not in text):
        raise AssertionError(f"{label}: exit={process.returncode}, expected={expected}, required={needle!r}")
    (evidence / "exits.json").write_text(json.dumps(results, sort_keys=True, indent=2) + "\n")
    return text

def identity(apply: bool) -> None:
    assert output(["git", "rev-parse", "HEAD"]) == manifest["head"]
    patch = probe / manifest["patch"]
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    if apply:
        subprocess.run(["git", "apply", "--index", str(patch)], cwd=root, check=True)
    assert output(["git", "write-tree"]) == manifest["tree"]
    for name, digest in manifest["files"].items():
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name
    subprocess.run(["git", "diff", "--exit-code"], cwd=root, check=True)
    record = {"candidate": manifest, "mode": mode, "target": os.environ.get("PROOF_TARGET", "native")}
    (evidence / "identity.json").write_text(json.dumps(record, sort_keys=True, indent=2) + "\n")
    print("IDENTITY " + json.dumps(record, sort_keys=True), flush=True)

def controls() -> None:
    if task == "dalgona":
        production = root / "internal/dal/tool/search/index.go"
        test_path = root / "internal/dal/tool/search/index_stop_test.go"
        anchor = "const minKillExitGrace = 30 * time.Second"
        replacement = "const minKillExitGrace = 10 * time.Second"
        command = ["go", "test", "-race", "-count=1", "-v", "-run", "^TestStopBuilder", "./internal/dal/tool/search"]
        needle = "cleanup lasted 10s, want the 30-second floor"
    else:
        production = root / "crates/sloplint/src/main.rs"
        test_path = root / "crates/sloplint/tests/optin_writes.rs"
        anchor = "        summary.baselined,\n"
        replacement = "        summary.baselined * 10,\n"
        command = ["cargo", "test", "--locked", "-p", "sloplint", "--test", "optin_writes", "--target", os.environ["PROOF_TARGET"], "default_agent_output_is_stable_for_threshold_and_ratchet_runs", "--", "--exact", "--nocapture"]
        needle = "ratchet mode must report exactly one baselined=1 field"
    original = production.read_bytes()
    candidate = test_path.read_bytes()
    source = original.decode()
    assert source.count(anchor) == 1
    old_test = subprocess.check_output(["git", "show", f"HEAD:{test_path.relative_to(root)}"], cwd=root)
    try:
        production.write_text(source.replace(anchor, replacement))
        test_path.write_bytes(old_test)
        run("old-oracle-mutant", command)
        test_path.write_bytes(candidate)
        run("new-oracle-mutant", command, expected=1, needle=needle)
    finally:
        production.write_bytes(original)
        test_path.write_bytes(candidate)
    green = run("restored-control", command)
    if task == "dalgona":
        assert "--- PASS: TestStopBuilderKeepsUnconfirmedChildAfterCleanupBudget" in green
        assert "\nPASS\n" in green and "--- SKIP:" not in green
    else:
        assert "test default_agent_output_is_stable_for_threshold_and_ratchet_runs ... ok" in green
        assert "1 passed; 0 failed" in green
    identity(False)
    print("MUTATION_PROOF " + json.dumps({"task": task, "exits": results}, sort_keys=True), flush=True)

identity(mode == "prepare")
if mode == "prepare":
    raise SystemExit(0)
if task == "dalgona":
    controls()
    run("web", ["just", "web"])
    run("verify", ["just", "verify"])
elif mode == "test":
    target = os.environ["PROOF_TARGET"]
    run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--target", target, "--", "-D", "warnings"])
    run("tests", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast", "--target", target])
    if target == "x86_64-unknown-linux-gnu":
        controls()
    run("release", ["cargo", "build", "--release", "--locked", "--target", target])
    if target == "x86_64-unknown-linux-gnu":
        run("self-lint", [f"target/{target}/release/sloplint", "check", "crates", "--format", "agent", "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
identity(False)
print("GATES_COMPLETE " + json.dumps({"task": task, "mode": mode, "tree": manifest["tree"], "exits": results}, sort_keys=True), flush=True)
