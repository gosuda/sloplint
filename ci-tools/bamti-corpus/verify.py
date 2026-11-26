from __future__ import annotations

import hashlib
import json
import os
import signal
from pathlib import Path
import subprocess
from typing import cast

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "probe" / "ci-tools" / "bamti-corpus"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "bamti-corpus-evidence"
EVIDENCE.mkdir()
MANIFEST = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
RESULTS: dict[str, dict[str, int | bool]] = {}

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()

def run(name: str, args: list[str], limit: int, required: bool = True) -> str:
    print(f"RUN {name}: {args!r}", flush=True)
    process = subprocess.Popen(args, cwd=SOURCE, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    timed_out = False
    try:
        output, _ = process.communicate(timeout=limit)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        output, _ = process.communicate()
    (EVIDENCE / f"{name}.log").write_text(output)
    RESULTS[name] = {"exit": process.returncode, "timed_out": timed_out, "limit_seconds": limit}
    (EVIDENCE / "results.json").write_text(json.dumps({"manifest": MANIFEST, "results": RESULTS}, indent=2) + "\n")
    print(f"RESULT {name}: {json.dumps(RESULTS[name])}\n{output[-20000:]}", flush=True)
    if required:
        assert process.returncode == 0 and not timed_out, name
    return output

def identity() -> None:
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("write-tree") == MANIFEST["tree"]
    for path, digest in cast(dict[str, str], MANIFEST["files"]).items():
        assert hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() == digest, path
    subprocess.run(["git", "diff", "--exit-code"], check=True)
    print("CORPUS_SOURCE_IDENTITY " + json.dumps(MANIFEST, sort_keys=True), flush=True)

assert git("rev-parse", "HEAD") == MANIFEST["head"]
candidate_patch = PROOF / "candidate.patch"
assert hashlib.sha256(candidate_patch.read_bytes()).hexdigest() == MANIFEST["patch_sha256"]
run("apply-candidate", ["git", "apply", "--index", str(candidate_patch)], 30)
identity()
instrument = PROOF / "instrument.patch"
assert hashlib.sha256(instrument.read_bytes()).hexdigest() == MANIFEST["instrument_sha256"]
tests = SOURCE / "crates/bamts-compiler/src/program/tests.rs"
saved_tests = tests.read_bytes()
assert hashlib.sha256(saved_tests).hexdigest() == MANIFEST["original_tests_sha256"]
relation = SOURCE / "crates/bamts-compiler/src/checker/relations.rs"
saved_relation = relation.read_bytes()
baseline = subprocess.check_output(["git", "show", "HEAD:crates/bamts-compiler/src/checker/relations.rs"])
run("apply-instrumentation", ["git", "apply", str(instrument)], 30)
assert hashlib.sha256(tests.read_bytes()).hexdigest() == MANIFEST["instrumented_tests_sha256"]
test_args = ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib", "program::tests::lowers_and_verifies_every_manifest_pinned_corpus_program"]
try:
    for name, content in [("baseline", baseline), ("candidate", saved_relation)]:
        relation.write_bytes(content)
        run(f"{name}-compile", [*test_args, "--no-run"], 600)
        run(f"{name}-corpus", [*test_args, "--", "--exact", "--nocapture", "--test-threads=1"], 180, required=False)
finally:
    relation.write_bytes(saved_relation)
    tests.write_bytes(saved_tests)
identity()
print("CORPUS_DIAGNOSTIC_COMPLETE " + json.dumps(RESULTS, sort_keys=True), flush=True)
