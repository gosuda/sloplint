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
assert mode == "x64", mode
run("fmt", ["cargo", "fmt", "--all", "--check"])
archive = download(
    "https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
    "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb",
    "powershell.tar.gz",
)
powershell = evidence / "powershell"
powershell.mkdir()
run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
(powershell / "pwsh").chmod(0o755)
run("powershell-literal-oracle", [str(powershell / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(proof / "oracle.ps1"), "-CasesPath", str(proof / "cases.json")], required="LITERAL_ORACLE_COMPLETE 21")
calls = source / "crates/sloplint-syntax/src/lang/powershell/calls.rs"
candidate = calls.read_bytes()
try:
    calls.write_bytes(subprocess.check_output(["git", "show", f"{manifest['head']}:crates/sloplint-syntax/src/lang/powershell/calls.rs"]))
    output = run("old-literal-adapter", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_literal_runs", "--", "--nocapture"], 101)
    for name in ["anonymous_prefix", "verbatim_backtick"]:
        assert f"{name} ... FAILED" in output, name
    output = run("old-literal-cli", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures", "--target", target, "fixtures_match_expected", "--", "--nocapture"], 101)
    assert "contiguous-log-literal" in output
finally:
    calls.write_bytes(candidate)
identity()
run("restored-literal-adapter", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", target, "powershell_literal_runs", "--", "--nocapture"], required="21 passed; 0 failed")
run("restored-literal-cli", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures", "--target", target, "fixtures_match_expected", "--", "--nocapture"])
identity()
print("POWERSHELL_LITERAL_COMPLETE " + json.dumps({"tree": manifest["tree"], "exits": results}, sort_keys=True), flush=True)
