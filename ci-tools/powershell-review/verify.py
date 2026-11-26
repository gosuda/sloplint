from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.request
from typing import cast

source = Path.cwd()
proof = source.parent / "proof" / "ci-tools" / "powershell-review"
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
files = cast(dict[str, str], manifest["files"])
evidence = Path(os.environ["RUNNER_TEMP"]) / "powershell-review-evidence"
evidence.mkdir(exist_ok=True)
environment = dict(os.environ)
target = os.environ.get("TARGET", "x86_64-unknown-linux-gnu")
results: dict[str, int] = {}

def run(name: str, args: list[str], expected: int = 0, required: str = "", cwd: Path = source) -> str:
    print(f"RUN {name}: {args!r}", flush=True)
    result = subprocess.run(args, cwd=cwd, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    (evidence / f"{name}.log").write_text(result.stdout)
    results[name] = result.returncode
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "exits": results}, sort_keys=True, indent=2) + "\n")
    print(f"EXIT {name}: {result.returncode}\n{result.stdout[-30000:]}", flush=True)
    assert result.returncode == expected, (name, result.returncode, expected)
    if required:
        assert required in result.stdout, (name, required)
    return result.stdout

def identity() -> None:
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() == manifest["head"]
    for path, expected in files.items():
        assert hashlib.sha256((source / path).read_bytes()).hexdigest() == expected, path
    for path, expected in cast(dict[str, str], manifest["grammar_blobs"]).items():
        actual = subprocess.check_output(["git", "hash-object", "--", path], cwd=source, text=True).strip()
        assert actual == expected, path
    assert subprocess.check_output(["git", "write-tree"], cwd=source, text=True).strip() == manifest["tree"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=source, check=True)
    print("POWERSHELL_SOURCE " + json.dumps(manifest, sort_keys=True), flush=True)

def download(url: str, digest: str, name: str) -> Path:
    path = evidence / name
    with urllib.request.urlopen(url, timeout=120) as response:
        path.write_bytes(response.read())
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    return path

def parser_cases(binary: Path, cases: list[dict[str, str | bool]], phase: str) -> list[str]:
    root = evidence / "cases"
    root.mkdir(exist_ok=True)
    mismatches: list[str] = []
    records: list[dict[str, str | bool | int]] = []
    for index, case in enumerate(cases):
        path = root / f"case-{index}.ps1"
        path.write_bytes(str(case["source"]).encode())
        result = subprocess.run([str(binary), "parse", "--quiet", str(path)], cwd=vendor, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        assert "Failed to load language" not in result.stdout, result.stdout
        accepted = result.returncode == 0
        record: dict[str, str | bool | int] = {"id": str(case["id"]), "accepted": accepted, "expected": bool(case["accepted"]), "exit": result.returncode}
        records.append(record)
        if accepted != case["accepted"]:
            mismatches.append(str(case["id"]))
            print("PARSER_MISMATCH " + json.dumps({"phase": phase, **record}), flush=True)
            (evidence / f"{phase}-case-{index}.log").write_text(result.stdout)
            if phase == "candidate" and len(mismatches) <= 2:
                diagnostic = subprocess.run([str(binary), "parse", str(path)], cwd=vendor, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
                print("PARSER_DETAIL " + diagnostic.stdout[:6000], flush=True)
    (evidence / f"{phase}-cases.json").write_text(json.dumps(records, indent=2) + "\n")
    print("PARSER_CASES " + json.dumps({"phase": phase, "count": len(cases), "mismatches": mismatches}), flush=True)
    return mismatches


mode = sys.argv[1]
run("fetch-control-source", ["git", "fetch", "--no-tags", "--depth=1", "origin", str(manifest["previous"])])
identity()
if mode == "lint":
    run("fmt", ["cargo", "fmt", "--all", "--check"])
    run("grammar-manifest", ["bash", "scripts/grammars.sh", "check"])
    run("release-scripts", ["bash", "scripts/selftest-release-scripts.sh"])
    archive = download(
        "https://github.com/EmbarkStudios/cargo-deny/releases/download/0.20.2/cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz",
        "9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f",
        "cargo-deny.tar.gz",
    )
    run("extract-deny", ["tar", "-xzf", str(archive), "-C", str(evidence), "cargo-deny-0.20.2-x86_64-unknown-linux-musl/cargo-deny"])
    run("deny", [str(evidence / "cargo-deny-0.20.2-x86_64-unknown-linux-musl/cargo-deny"), "check"])
elif mode in ("x64", "arm"):
    if mode == "x64":
        archive = download(
            "https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
            "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb",
            "powershell.tar.gz",
        )
        powershell = evidence / "powershell"
        powershell.mkdir()
        run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
        (powershell / "pwsh").chmod(0o755)
        run("powershell-grammar-oracle", [str(powershell / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(proof / "grammar-oracle.ps1"), "-CasesPath", str(proof / "grammar-cases.json")], required="POWERSHELL_ORACLE_COMPLETE")
        run("powershell-literal-oracle", [str(powershell / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(proof / "literal-oracle.ps1"), "-CasesPath", str(proof / "literal-cases.json")], required="LITERAL_ORACLE_COMPLETE 26")
        compressed = download(
            "https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
            "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d",
            "tree-sitter.gz",
        )
        tree_sitter = evidence / "tree-sitter"
        tree_sitter.write_bytes(gzip.decompress(compressed.read_bytes()))
        tree_sitter.chmod(0o755)
        run("tree-sitter-version", [str(tree_sitter), "--version"], required="tree-sitter 0.27.1")
        vendor = source / "vendor" / "tree-sitter-powershell"
        cases = cast(list[dict[str, str | bool]], json.loads((proof / "grammar-cases.json").read_text()))
        assert len(cases) == 151
        run("scanner-syntax", ["cc", "-fsyntax-only", "-std=c11", "-I", str(vendor / "src"), str(vendor / "src/scanner.c")])
        run("grammar-generate", [str(tree_sitter), "generate"], cwd=vendor)
        assert parser_cases(tree_sitter, cases, "candidate") == []
        assert "#define STATE_COUNT 2017" in (vendor / "src/parser.c").read_text()
        patch_lines = (vendor / "sloplint-grammar.patch").read_text().splitlines()
        assert len(patch_lines) == 2355 and sum(line.startswith("@@ ") for line in patch_lines) == 49
        run("grammar-corpus", [str(tree_sitter), "test"], cwd=vendor)
        environment["GRAMMARS_TS_BIN"] = str(tree_sitter)
        environment["GRAMMARS_SCRATCH"] = str(evidence / "grammar-upstream")
        run("grammar-verify", ["bash", "scripts/grammars.sh", "verify", "tree-sitter-powershell"], required="tree-sitter-powershell OK")
        identity()
        calls = source / "crates/sloplint-syntax/src/lang/powershell/calls.rs"
        saved = calls.read_bytes()
        try:
            calls.write_bytes(subprocess.check_output(["git", "show", f"{manifest['previous']}:crates/sloplint-syntax/src/lang/powershell/calls.rs"]))
            output = run("old-adapter", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_literal_runs", "--", "--nocapture"], 101)
            for name in ["anonymous_prefix", "verbatim_backtick"]:
                assert f"{name} ... FAILED" in output, name
            run("old-adapter-fixtures", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures", "--target", target, "fixtures_match_expected", "--", "--nocapture"], 101, "contiguous-log-literal-near-miss__powershell")
        finally:
            calls.write_bytes(saved)
        try:
            calls.write_bytes((proof / "pre-unicode-calls.rs").read_bytes())
            output = run("old-unicode-predicate", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_literal_runs", "--", "--nocapture"], 101)
            for name in ["roman_numeral_is_literal", "numeric_fraction_is_literal", "astral_letter_is_literal", "variable_ends_before_astral_letter"]:
                assert f"{name} ... FAILED" in output, name
        finally:
            calls.write_bytes(saved)
        identity()
        run("candidate-regressions", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_", "--", "--nocapture"])
    run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--target", target, "--", "-D", "warnings"])
    run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast", "--target", target])
    run("release", ["cargo", "build", "--release", "--locked", "--target", target])
    if mode == "x64":
        run("self-lint", [str(source / "target" / target / "release" / "sloplint"), "check", "crates", "--format", "agent", "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
else:
    raise ValueError(mode)
run("whitespace", ["git", "diff", "--check", str(manifest["previous"])])
identity()
print("POWERSHELL_GATES_COMPLETE " + json.dumps({"mode": mode, "tree": manifest["tree"], "exits": results}, sort_keys=True), flush=True)
