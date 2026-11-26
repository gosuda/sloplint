from __future__ import annotations

import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
from typing import cast

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "probe" / "ci-tools" / "bamti-source-alias-r19"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "bamti-source-alias-r19-evidence"
EVIDENCE.mkdir(exist_ok=True)
INPUTS = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
CASES = cast(list[dict[str, object]], json.loads((PROOF / "cases.json").read_text()))
TEST = SOURCE / str(INPUTS["test_path"])
EXITS: dict[str, int] = {}

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()

def run(name: str, args: list[str], expected: int | None = 0, limit: int = 600) -> str:
    print(f"RUN {name}: {args!r}", flush=True)
    started = time.monotonic()
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
    print(f"EXIT {name}: {process.returncode} elapsed_seconds={time.monotonic() - started:.3f} timed_out={timed_out}\n{output[-20000:]}", flush=True)
    assert not timed_out, f"{name}: exceeded {limit} seconds; actual exit {process.returncode}"
    assert expected is None or process.returncode == expected, f"{name}: expected {expected}, got {process.returncode}"
    return output

def current_identity() -> dict[str, object]:
    index = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=SOURCE)
    entries = index.split(b"\0")
    count = 0
    for entry in entries:
        if not entry:
            continue
        metadata, relative = entry.split(b"\t", 1)
        mode, expected, stage = metadata.split()
        assert stage == b"0", f"unmerged index entry: {relative!r}"
        path = SOURCE / os.fsdecode(relative)
        if mode == b"120000":
            content = os.fsencode(os.readlink(path))
        else:
            assert mode in (b"100644", b"100755"), f"unsupported tracked mode: {mode!r}"
            content = path.read_bytes()
        actual = hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest()
        assert actual == expected.decode(), f"tracked blob mismatch: {relative!r}"
        count += 1
    subprocess.run(["git", "diff", "--exit-code"], cwd=SOURCE, check=True)
    return {
        "head": git("rev-parse", "HEAD"),
        "parent": git("rev-parse", "HEAD^"),
        "tree": git("write-tree"),
        "tracked_blobs": count,
        "index_sha256": hashlib.sha256(index).hexdigest(),
        "tests_sha256": hashlib.sha256(TEST.read_bytes()).hexdigest(),
    }

def identity() -> None:
    frozen = cast(dict[str, object], json.loads((EVIDENCE / "source-identity.json").read_text()))
    actual = current_identity()
    assert actual == frozen, f"source identity changed: {actual!r} != {frozen!r}"
    assert git("diff", "--cached", "--name-only") == INPUTS["test_path"]
    print("BAMTI_SOURCE_IDENTITY " + json.dumps(actual, sort_keys=True), flush=True)

def prepare() -> None:
    initial = current_identity()
    assert initial["head"] == INPUTS["head"]
    assert initial["parent"] == INPUTS["parent"]
    assert initial["tree"] == INPUTS["base_tree"]
    assert initial["tests_sha256"] == INPUTS["original_test_sha256"]
    assert git("rev-parse", f"HEAD:{INPUTS['test_path']}") == INPUTS["original_test_git_blob"]
    (EVIDENCE / "baseline-identity.json").write_text(json.dumps(initial, indent=2, sort_keys=True) + "\n")
    fragment = (PROOF / "frozen-test.rs").read_bytes()
    assert hashlib.sha256(fragment).hexdigest() == INPUTS["frozen_test_sha256"]
    original = TEST.read_text()
    anchor = str(INPUTS["anchor"])
    assert original.count(anchor) == 1
    candidate = original.replace(anchor, fragment.decode() + "\n" + anchor)
    assert hashlib.sha256(candidate.encode()).hexdigest() == INPUTS["input_candidate_sha256"]
    TEST.write_text(candidate)
    run("format-input", ["cargo", "fmt", "--all"])
    formatted = TEST.read_text()
    formatting = "".join(difflib.unified_diff(candidate.splitlines(keepends=True), formatted.splitlines(keepends=True), fromfile="a/" + str(INPUTS["test_path"]), tofile="b/" + str(INPUTS["test_path"])))
    (EVIDENCE / "formatting.patch").write_text(formatting)
    print("FORMATTING_PATCH_BEGIN\n" + formatting + "FORMATTING_PATCH_END", flush=True)
    assert git("diff", "--name-only") == INPUTS["test_path"]
    subprocess.run(["git", "add", str(INPUTS["test_path"])], cwd=SOURCE, check=True)
    frozen = current_identity()
    (EVIDENCE / "source-identity.json").write_text(json.dumps(frozen, indent=2, sort_keys=True) + "\n")
    (EVIDENCE / "candidate.patch").write_text(subprocess.check_output(["git", "diff", "--cached", "--binary"], cwd=SOURCE, text=True))
    (EVIDENCE / "candidate-tests.rs").write_bytes(TEST.read_bytes())
    identity()

def verify() -> None:
    identity()
    assert len(CASES) == INPUTS["case_count"] == 4
    assert json.loads((SOURCE / "node_modules/typescript/package.json").read_text())["version"] == INPUTS["typescript"]
    assert subprocess.check_output(["node", "--version"], text=True).strip() == "v" + str(INPUTS["node"])
    assert subprocess.check_output(["rustc", "--version"], text=True).startswith("rustc " + str(INPUTS["rust"]) + " ")
    oracle = EVIDENCE / "oracle"
    oracle.mkdir(exist_ok=True)
    oracle_results: list[dict[str, object]] = []
    for case in CASES:
        name = str(case["name"])
        source = str(case["source"])
        assert hashlib.sha256(source.encode()).hexdigest() == case["source_sha256"]
        witness = oracle / f"{name}.ts"
        witness.write_text(source)
        expected = int(str(case["typescript_exit"]))
        output = run(f"typescript-{name}", ["node", "node_modules/typescript/bin/tsc", "--ignoreConfig", "--noEmit", "--strict", "--skipLibCheck", "--pretty", "false", str(witness)], expected, limit=30)
        codes = re.findall(r"error TS(\d+):", output)
        assert codes == (["2322"] if expected else []), f"{name}: unexpected TypeScript diagnostics {codes!r}"
        oracle_results.append({"name": name, "source_sha256": case["source_sha256"], "exit": expected, "diagnostics": codes})
    (EVIDENCE / "typescript-results.json").write_text(json.dumps(oracle_results, indent=2, sort_keys=True) + "\n")
    args = ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib"]
    run("baseline-compile", [*args, "--no-run"])
    output = run("baseline-source-assignments", [*args, "checker::tests::transformed_alias_source_assignments_preserve_union_order", "--", "--exact", "--nocapture"], expected=None, limit=180)
    records = re.findall(r"BINDING_ALIAS_SOURCE_BEGIN (\S+)\n(.*?)BINDING_ALIAS_SOURCE_END", output, re.DOTALL)
    assert len(records) == len(CASES), f"expected four compiler observations, got {len(records)}"
    assert len(dict(records)) == len(records), "duplicate compiler case name"
    expected_sources = {str(case["name"]): str(case["source"]) for case in CASES}
    assert dict(records) == expected_sources, "compiler and TypeScript source bytes differ"
    for name, source in records:
        print(f"BINDING_ORACLE_SOURCE_MATCH {name} {hashlib.sha256(source.encode()).hexdigest()}", flush=True)
    identity()
    assert EXITS["baseline-source-assignments"] == 0, "unchanged implementation disagrees with the frozen source-assignment expectations"
    assert "1 passed; 0 failed" in output
    run("fmt", ["cargo", "fmt", "--all", "--check"])
    run("check", ["cargo", "check", "--workspace", "--locked"])
    run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
    run("fetch", ["cargo", "fetch", "--locked"])
    run("authority", ["cargo", "run", "--locked", "-p", "bamts-verification", "--", "source", "fetch", "typescript-primary-tests", "--dest", "target/authority/typescript-7.0.2-tests"])
    run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"], limit=1200)
    run("node-contracts", ["cargo", "test", "--locked", "-p", "bamts-verification", "--lib", "--", "--ignored", "--exact", "corpus::tests::verify_node_version_accepts_pinned_and_rejects_missing", "corpus::tests::oracle_runs_a_real_corpus_case", "facets::tests::forged_unpinned_js_outcomes_are_not_a_public_proof_path"])
    run("npm", ["node", "--test", "--test-skip-pattern=staged cli-|clean consumer", *[str(path) for path in sorted((SOURCE / "npm/test").glob("*.test.mjs"))]])
    run("whitespace", ["git", "diff", "--cached", "--check"])
    identity()
    print("BAMTI_SOURCE_ASSIGNMENTS_COMPLETE " + json.dumps(EXITS, sort_keys=True), flush=True)

if sys.argv[1] == "prepare":
    prepare()
elif sys.argv[1] == "verify":
    verify()
else:
    raise ValueError(sys.argv[1])
