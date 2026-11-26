from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import cast

source = Path.cwd()
proof = source.parent / "probe" / "ci-tools" / "bamti-cycle"
evidence = Path(os.environ["RUNNER_TEMP"]) / "bamti-cycle-evidence"
evidence.mkdir(exist_ok=True)
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
exits: dict[str, int] = {}

def command(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=source, text=True).strip()

def identity() -> None:
    assert command(["git", "rev-parse", "HEAD"]) == manifest["head"]
    assert command(["git", "write-tree"]) == manifest["tree"]
    files = cast(dict[str, str], manifest["files"])
    for name, expected in files.items():
        assert hashlib.sha256((source / name).read_bytes()).hexdigest() == expected, name
    subprocess.run(["git", "diff", "--exit-code"], cwd=source, check=True)
    print("BAMTI_IDENTITY " + json.dumps(manifest, sort_keys=True), flush=True)

def run(name: str, args: list[str], expected: int = 0, required: str = "") -> str:
    print(f"RUN {name}: {args!r}", flush=True)
    process = subprocess.run(args, cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, check=False)
    output = process.stdout
    (evidence / f"{name}.log").write_text(output)
    exits[name] = process.returncode
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "exits": exits}, indent=2, sort_keys=True) + "\n")
    print(f"EXIT {name}: {process.returncode}\n{output[-20000:]}", flush=True)
    assert process.returncode == expected, f"{name}: expected {expected}, got {process.returncode}"
    if required:
        assert required in output, f"{name}: missing {required!r}"
    return output

mode = sys.argv[1]
if mode == "prepare":
    patch = proof / "candidate.patch"
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    assert command(["git", "rev-parse", "HEAD"]) == manifest["head"]
    subprocess.run(["git", "apply", "--index", str(patch)], cwd=source, check=True)
    identity()
elif mode == "verify":
    identity()
    assert json.loads((source / "node_modules/typescript/package.json").read_text())["version"] == "7.0.2"
    oracle_dir = evidence / "oracle"
    oracle_dir.mkdir(exist_ok=True)
    for parameter, expected in [("number", 2), ("string", 0)]:
        branches = [f"{{ root: B<{parameter}> }}", f'{{ root: {{ a: B<{parameter}>["a"] }} }}']
        for reverse in [False, True]:
            order = list(reversed(branches)) if reverse else branches
            text = (
                "type A<T> = { a: { back: A<T>; leaf: string }; z: T };\n"
                "type B<T> = { a: { back: B<T>; leaf: Object }; z: T };\n"
                "declare const value: { root: A<string> };\n"
                f"const result: {' | '.join(order)} = value;\n"
            )
            witness = oracle_dir / f"{parameter}-{reverse}.ts"
            witness.write_text(text)
            output = run(
                f"typescript-{parameter}-{reverse}",
                ["node", "node_modules/typescript/bin/tsc", "--ignoreConfig", "--noEmit", "--strict", "--skipLibCheck", "--pretty", "false", str(witness)],
                expected,
                "error TS2322:" if expected else "",
            )
            if expected:
                assert output.count("error TS2322:") == 1
    relation = source / "crates/bamts-compiler/src/checker/relations.rs"
    candidate = relation.read_bytes()
    try:
        relation.write_bytes(subprocess.check_output(["git", "show", "HEAD:crates/bamts-compiler/src/checker/relations.rs"], cwd=source))
        run("baseline-recursive", ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib", "recursive_union_", "--", "--nocapture"], 101, "recursive branch must not reuse a disproven cycle")
    finally:
        relation.write_bytes(candidate)
    identity()
    run("candidate-recursive", ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib", "recursive_union_", "--", "--nocapture"], required="2 passed; 0 failed")
    run("acyclic-cache-performance", ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib", "checker::relations::tests::shared_object_graph_with_intrinsic_object_scales_after_cache_saturation", "--", "--exact", "--nocapture"], required="1 passed; 0 failed")
    run("fmt", ["cargo", "fmt", "--all", "--check"])
    run("check", ["cargo", "check", "--workspace", "--locked"])
    run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
    run("fetch", ["cargo", "fetch", "--locked"])
    run("authority", ["cargo", "run", "--locked", "-p", "bamts-verification", "--", "source", "fetch", "typescript-primary-tests", "--dest", "target/authority/typescript-7.0.2-tests"])
    run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"])
    run("node-contracts", ["cargo", "test", "--locked", "-p", "bamts-verification", "--lib", "--", "--ignored", "--exact", "corpus::tests::verify_node_version_accepts_pinned_and_rejects_missing", "corpus::tests::oracle_runs_a_real_corpus_case", "facets::tests::forged_unpinned_js_outcomes_are_not_a_public_proof_path"])
    run("npm", ["node", "--test", "--test-skip-pattern=staged cli-|clean consumer", *[str(p) for p in sorted((source / "npm/test").glob("*.test.mjs"))]])
    identity()
    print("BAMTI_GATES_COMPLETE " + json.dumps({"tree": manifest["tree"], "exits": exits}, sort_keys=True), flush=True)
else:
    raise ValueError(mode)
