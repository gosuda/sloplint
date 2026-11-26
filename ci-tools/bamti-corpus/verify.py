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

identity()
instrument = PROOF / "instrument.patch"
assert hashlib.sha256(instrument.read_bytes()).hexdigest() == MANIFEST["instrument_sha256"]
patch = PROOF / "candidate.patch"
assert hashlib.sha256(patch.read_bytes()).hexdigest() == MANIFEST["patch_sha256"]
frozen = (PROOF / "saturation-test.rs").read_bytes()
assert hashlib.sha256(frozen).hexdigest() == MANIFEST["saturation_test_sha256"]
counter = (PROOF / "detailed-profiler.rs").read_bytes()
assert hashlib.sha256(counter).hexdigest() == MANIFEST["detailed_profiler_sha256"]
alias_test = (PROOF / "alias-completion-test.rs").read_bytes()
assert hashlib.sha256(alias_test).hexdigest() == MANIFEST["alias_completion_test_sha256"]
tests = SOURCE / "crates/bamts-compiler/src/program/tests.rs"
saved_tests = tests.read_bytes()
assert hashlib.sha256(saved_tests).hexdigest() == MANIFEST["original_tests_sha256"]
relation = SOURCE / "crates/bamts-compiler/src/checker/relations.rs"
saved_relation = relation.read_bytes()
run("fetch-exact-baseline", ["git", "fetch", "--no-tags", "--depth=1", "origin", str(MANIFEST["baseline_head"])], 60)
baseline = subprocess.check_output(["git", "show", str(MANIFEST["baseline_head"]) + ":crates/bamts-compiler/src/checker/relations.rs"], cwd=SOURCE)
assert hashlib.sha256(baseline).hexdigest() == MANIFEST["baseline_relations_sha256"]
try:
    relation.write_bytes(baseline)
    run("apply-rejected-candidate", ["git", "apply", str(patch)], 30)
    candidate = relation.read_text()
    assert hashlib.sha256(relation.read_bytes()).hexdigest() == MANIFEST["candidate_relations_sha256"]
    run("apply-instrumentation", ["git", "apply", str(instrument)], 30)
    assert hashlib.sha256(tests.read_bytes()).hexdigest() == MANIFEST["instrumented_tests_sha256"]
    marker = "    /// Regression for nested contextual hits:"
    assert candidate.count(marker) == 1
    candidate = candidate.replace(marker, frozen.decode() + alias_test.decode() + marker)
    field_marker = "    #[cfg(test)]\n    computed_relations: Cell<usize>,"
    field_extra = (
        "    #[cfg(test)]\n"
        "    diagnostic_visits: RefCell<HashMap<RelationKey, usize>>,\n"
        "    #[cfg(test)]\n"
        "    diagnostic_outcomes: RefCell<HashMap<RelationKey, (bool, Vec<RelationKey>, bool)>>,\n"
        "    #[cfg(test)]\n"
        "    diagnostic_alias_assumptions: RefCell<HashSet<RelationKey>>,\n"
    )
    assert candidate.count(field_marker) == 1
    candidate = candidate.replace(field_marker, field_extra + field_marker)
    init_marker = "            #[cfg(test)]\n            computed_relations: Cell::new(0),"
    init_extra = (
        "            #[cfg(test)]\n"
        "            diagnostic_visits: RefCell::new(HashMap::new()),\n"
        "            #[cfg(test)]\n"
        "            diagnostic_outcomes: RefCell::new(HashMap::new()),\n"
        "            #[cfg(test)]\n"
        "            diagnostic_alias_assumptions: RefCell::new(HashSet::new()),\n"
    )
    assert candidate.count(init_marker) == 1
    candidate = candidate.replace(init_marker, init_extra + init_marker)
    alias_marker = "        if let Some(assumption) = alias_assumption {\n"
    assert candidate.count(alias_marker) == 1
    candidate = candidate.replace(alias_marker, alias_marker + "            #[cfg(test)]\n            self.diagnostic_alias_assumptions.borrow_mut().insert(assumption);\n")
    completion_marker = "        // A clean hit has no dependency bookkeeping"
    assert candidate.count(completion_marker) == 1
    candidate = candidate.replace(completion_marker, "        #[cfg(test)]\n        self.diagnostic_outcomes.borrow_mut().insert(relation, (result, frame.assumptions.iter().copied().collect(), frame.env_sensitive));\n" + completion_marker)
    count_marker = "        let (from, to) = (self.table.get(source), self.table.get(target));"
    assert candidate.count(count_marker) == 1
    candidate = candidate.replace(count_marker, counter.decode() + count_marker)
    cache_limit = "const RELATION_CACHE_CAPACITY: usize = 4096;"
    assert candidate.count(cache_limit) == 1
    test_args = ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib"]
    for name, content in [
        ("unbounded-diagnostic", candidate.replace(cache_limit, "const RELATION_CACHE_CAPACITY: usize = usize::MAX;")),
    ]:
        relation.write_text(content)
        print(f"DIAGNOSTIC_VARIANT {name} sha256=" + hashlib.sha256(relation.read_bytes()).hexdigest(), flush=True)
        run(f"{name}-compile", [*test_args, "--no-run"], 600)
        run(f"{name}-alias-completion", [*test_args, "checker::relations::tests::completed_alias_approximations_do_not_prove_changed_arguments", "--", "--exact", "--nocapture"], 180, required=False)
        run(f"{name}-saturation", [*test_args, "checker::relations::tests::shared_active_cycle_scales_after_context_cache_saturation", "--", "--exact", "--nocapture"], 180, required=False)
        run(f"{name}-corpus", [*test_args, "program::tests::lowers_and_verifies_every_manifest_pinned_corpus_program", "--", "--exact", "--nocapture", "--test-threads=1"], 180, required=False)
finally:
    relation.write_bytes(saved_relation)
    tests.write_bytes(saved_tests)
identity()
print("CAPACITY_DIAGNOSTIC_COMPLETE " + json.dumps(RESULTS, sort_keys=True), flush=True)
