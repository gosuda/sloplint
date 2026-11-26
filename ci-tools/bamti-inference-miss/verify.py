from __future__ import annotations

import difflib
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
from typing import cast

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "probe" / "ci-tools" / "bamti-inference-miss"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "bamti-inference-miss-evidence"
EVIDENCE.mkdir(exist_ok=True)
INPUTS = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
RELATION = SOURCE / "crates/bamts-compiler/src/checker/relations.rs"
EXITS: dict[str, int] = {}

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()

def run(name: str, args: list[str], expected: int = 0, required: str = "", limit: int = 600) -> str:
    print(f"RUN {name}: {args!r}", flush=True)
    process = subprocess.Popen(args, cwd=SOURCE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
    timed_out = False
    try:
        output, _ = process.communicate(timeout=limit)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        output, _ = process.communicate()
    (EVIDENCE / f"{name}.log").write_text(output)
    EXITS[name] = process.returncode
    (EVIDENCE / "results.json").write_text(json.dumps({"inputs": INPUTS, "exits": EXITS}, indent=2, sort_keys=True) + "\n")
    print(f"EXIT {name}: {process.returncode} timed_out={timed_out}\n{output[-20000:]}", flush=True)
    assert not timed_out, f"{name}: exceeded {limit} seconds; actual exit {process.returncode}"
    assert process.returncode == expected, f"{name}: expected {expected}, got {process.returncode}"
    assert not required or required in output, f"{name}: missing {required!r}"
    return output

def identity() -> None:
    frozen = cast(dict[str, str], json.loads((EVIDENCE / "source-identity.json").read_text()))
    assert git("rev-parse", "HEAD") == INPUTS["head"]
    assert git("write-tree") == frozen["tree"]
    assert hashlib.sha256(RELATION.read_bytes()).hexdigest() == frozen["relations_sha256"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=SOURCE, check=True)
    print("BAMTI_SOURCE_IDENTITY " + json.dumps(frozen, sort_keys=True), flush=True)

if sys.argv[1] == "prepare":
    assert git("rev-parse", "HEAD") == INPUTS["head"]
    assert git("write-tree") == INPUTS["base_tree"]
    assert hashlib.sha256(RELATION.read_bytes()).hexdigest() == INPUTS["original_relations_sha256"]
    candidate = (PROOF / "candidate-relations.rs").read_bytes()
    assert hashlib.sha256(candidate).hexdigest() == INPUTS["input_relations_sha256"]
    RELATION.write_bytes(candidate)
    run("format-input", ["cargo", "fmt", "--all"])
    formatted = RELATION.read_bytes()
    formatting = "".join(difflib.unified_diff(candidate.decode().splitlines(keepends=True), formatted.decode().splitlines(keepends=True), fromfile="a/crates/bamts-compiler/src/checker/relations.rs", tofile="b/crates/bamts-compiler/src/checker/relations.rs"))
    (EVIDENCE / "formatting.patch").write_text(formatting)
    print("FORMATTING_PATCH_BEGIN\n" + formatting + "FORMATTING_PATCH_END", flush=True)
    subprocess.run(["git", "add", "crates/bamts-compiler/src/checker/relations.rs"], cwd=SOURCE, check=True)
    frozen = {"head": git("rev-parse", "HEAD"), "tree": git("write-tree"), "relations_sha256": hashlib.sha256(formatted).hexdigest()}
    (EVIDENCE / "source-identity.json").write_text(json.dumps(frozen, indent=2, sort_keys=True) + "\n")
    (EVIDENCE / "candidate.patch").write_text(subprocess.check_output(["git", "diff", "--cached", "--binary"], cwd=SOURCE, text=True))
    identity()
elif sys.argv[1] == "verify":
    identity()
    assert json.loads((SOURCE / "node_modules/typescript/package.json").read_text())["version"] == "7.0.2"
    oracle = EVIDENCE / "oracle"
    oracle.mkdir(exist_ok=True)
    for leaf, expected in [("number", 1), ("string", 0)]:
        source = [
            "type A = S12;",
            "type B = T12;",
            "type S0 = { back: A; infer: <T>(value: {}) => string; value: string };",
            f"type T0 = {{ back: B; infer: (value: {{}}) => string; value: {leaf} }};",
        ]
        for depth in range(1, 13):
            source += [
                f"type S{depth} = {{ child: S{depth - 1} }};",
                f'type T{depth} = {{ child: T{depth - 1}; tag?: "first" }} | {{ child: T{depth - 1}; tag?: "second" }};',
            ]
        source += ["declare const value: A;", "const result: B = value;"]
        witness = oracle / f"inference-miss-{leaf}.ts"
        witness.write_text("\n".join(source) + "\n")
        run(f"typescript-inference-{leaf}", ["node", "node_modules/typescript/bin/tsc", "--ignoreConfig", "--noEmit", "--strict", "--skipLibCheck", "--pretty", "false", str(witness)], expected, "error TS2322:" if expected else "", limit=30)
    candidate = RELATION.read_bytes()
    baseline = subprocess.check_output(["git", "show", "HEAD:crates/bamts-compiler/src/checker/relations.rs"], cwd=SOURCE).decode()
    marker = "#[cfg(test)]\nmod tests {"
    assert baseline.count(marker) == 1 and candidate.decode().count(marker) == 1
    frozen_tests = candidate.decode().split(marker, 1)[1]
    args = ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib"]
    try:
        RELATION.write_text(baseline.split(marker, 1)[0] + marker + frozen_tests)
        run("baseline-compile", [*args, "--no-run"])
        run("baseline-inference-miss", [*args, "checker::relations::tests::inference_misses_do_not_recompute_shared_recursive_refutations", "--", "--exact", "--nocapture"], 101, "inference miss recomputed", limit=180)
    finally:
        RELATION.write_bytes(candidate)
    identity()
    run("candidate-relations", [*args, "checker::relations::tests::", "--", "--nocapture"], required="test result: ok.")
    run("candidate-recursive-union", [*args, "recursive_union_", "--", "--nocapture"], required="2 passed; 0 failed")
    run("candidate-corpus", [*args, "program::tests::lowers_and_verifies_every_manifest_pinned_corpus_program", "--", "--exact", "--nocapture", "--test-threads=1"], required="1 passed; 0 failed", limit=180)
    run("fmt", ["cargo", "fmt", "--all", "--check"])
    run("check", ["cargo", "check", "--workspace", "--locked"])
    run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
    run("fetch", ["cargo", "fetch", "--locked"])
    run("authority", ["cargo", "run", "--locked", "-p", "bamts-verification", "--", "source", "fetch", "typescript-primary-tests", "--dest", "target/authority/typescript-7.0.2-tests"])
    run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"], limit=1200)
    run("node-contracts", ["cargo", "test", "--locked", "-p", "bamts-verification", "--lib", "--", "--ignored", "--exact", "corpus::tests::verify_node_version_accepts_pinned_and_rejects_missing", "corpus::tests::oracle_runs_a_real_corpus_case", "facets::tests::forged_unpinned_js_outcomes_are_not_a_public_proof_path"])
    run("npm", ["node", "--test", "--test-skip-pattern=staged cli-|clean consumer", *[str(path) for path in sorted((SOURCE / "npm/test").glob("*.test.mjs"))]])
    identity()
    print("BAMTI_GATES_COMPLETE " + json.dumps(EXITS, sort_keys=True), flush=True)
else:
    raise ValueError(sys.argv[1])
