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
    assert subprocess.check_output(["git", "write-tree"], cwd=source, text=True).strip() == manifest["tree"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=source, check=True)
    print("POWERSHELL_SOURCE " + json.dumps(manifest, sort_keys=True), flush=True)

def download(url: str, digest: str, name: str) -> Path:
    path = evidence / name
    with urllib.request.urlopen(url, timeout=120) as response:
        path.write_bytes(response.read())
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    return path

mode = sys.argv[1]
if mode == "prepare":
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() == manifest["head"]
    patch = proof / "candidate.patch"
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    run("apply-check", ["git", "apply", "--check", str(patch)])
    run("apply", ["git", "apply", str(patch)])
    subprocess.run(["git", "add", "--", *files], check=True)
    identity()
    raise SystemExit(0)

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
        run("powershell-oracle", [str(powershell / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(proof / "oracle.ps1"), "-CasesPath", str(proof / "cases.json")], required="POWERSHELL_ORACLE_COMPLETE")
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
        run("grammar-generate", [str(tree_sitter), "generate"], cwd=vendor)
        run("grammar-corpus", [str(tree_sitter), "test"], cwd=vendor)
        environment["GRAMMARS_TS_BIN"] = str(tree_sitter)
        environment["GRAMMARS_SCRATCH"] = str(evidence / "grammar-upstream")
        run("grammar-verify", ["bash", "scripts/grammars.sh", "verify", "tree-sitter-powershell"], required="tree-sitter-powershell OK")
        identity()
        scanner = source / "vendor/tree-sitter-powershell/src/scanner.c"
        saved = scanner.read_bytes()
        try:
            scanner.write_bytes(subprocess.check_output(["git", "show", f"{manifest['head']}:vendor/tree-sitter-powershell/src/scanner.c"]))
            output = run("old-scanner", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_words", "--", "--nocapture"], 101)
            for name in ["here_string_dollar_does_not_consume_the_closing_line", "leading_pipes_require_an_adjacent_nonblank_line"]:
                assert f"{name} ... FAILED" in output, name
        finally:
            scanner.write_bytes(saved)
        calls = source / "crates/sloplint-syntax/src/lang/powershell/calls.rs"
        saved = calls.read_bytes()
        try:
            calls.write_bytes(subprocess.check_output(["git", "show", f"{manifest['head']}:crates/sloplint-syntax/src/lang/powershell/calls.rs"]))
            run("old-adapter", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "variable_path_literals_use_powershell_escape_rules", "--", "--nocapture"], 101, "variable_path_literals_use_powershell_escape_rules ... FAILED")
            run("old-adapter-fixtures", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures", "--target", target, "fixtures_match_expected", "--", "--nocapture"], 101, "contiguous-log-literal-near-miss__powershell")
        finally:
            calls.write_bytes(saved)
        identity()
        run("candidate-regressions", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_words", "--", "--nocapture"])
    run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--target", target, "--", "-D", "warnings"])
    run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast", "--target", target])
    run("release", ["cargo", "build", "--release", "--locked", "--target", target])
    if mode == "x64":
        run("self-lint", [str(source / "target" / target / "release" / "sloplint"), "check", "crates", "--format", "agent", "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
else:
    raise ValueError(mode)
identity()
print("POWERSHELL_GATES_COMPLETE " + json.dumps({"mode": mode, "tree": manifest["tree"], "exits": results}, sort_keys=True), flush=True)
