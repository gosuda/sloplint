from __future__ import annotations

import hashlib
import gzip
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import urllib.request
from typing import cast

workspace = Path(os.environ["GITHUB_WORKSPACE"])
source = workspace / "source"
proof = workspace / "proof/ci-tools/powershell-round14"
evidence = Path(os.environ["RUNNER_TEMP"]) / "powershell-round14-candidate"
evidence.mkdir()
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
environment = dict(os.environ)
environment.update(CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="8", CARGO_TERM_COLOR="never",
                   CARGO_PROFILE_DEV_DEBUG="line-tables-only", CARGO_PROFILE_TEST_DEBUG="line-tables-only",
                   SLOPLINT_REQUIRE_FS_CAPS="1")
results: list[dict[str, object]] = []
vendor = source / "vendor/tree-sitter-powershell"

def output(args: list[str], cwd: Path = source, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=cwd, env=environment if env is None else env, text=True).strip()

def run(name: str, args: list[str], expected: int = 0, cwd: Path = source) -> str:
    start = time.monotonic()
    result = subprocess.run(args, cwd=cwd, env=environment, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=2400, check=False)
    (evidence / (name + ".log")).write_text(result.stdout)
    results.append({"name": name, "args": args, "exit": result.returncode, "expected": expected,
                    "seconds": time.monotonic() - start,
                    "log_sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")
    print(f"CANDIDATE_GATE_EXIT {name} {result.returncode}\n{result.stdout[-18000:]}", flush=True)
    assert result.returncode == expected, (name, result.returncode, expected)
    return result.stdout

def download(url: str, digest: str, name: str) -> Path:
    path = evidence / name
    with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    return path

def clean_source() -> None:
    assert output(["git", "rev-parse", "HEAD"]) == manifest["base"]
    assert output(["git", "write-tree"]) == manifest["base_tree"]
    assert output(["git", "status", "--porcelain", "--untracked-files=all"]) == ""

for name, digest in cast(dict[str, str], manifest["proof_sha256"]).items():
    assert hashlib.sha256((proof / name).read_bytes()).hexdigest() == digest, name
clean_source()
authored = cast(list[dict[str, str]], json.loads((proof / "candidate-elements.json").read_text()))
old_patch = (vendor / "sloplint-grammar.patch").read_bytes()
old_calls = (source / "crates/sloplint-syntax/src/lang/powershell/calls.rs").read_bytes()
old_words = (source / "crates/sloplint-syntax/src/lang/powershell/calls/words.rs").read_bytes()
for item in authored:
    file = source / item["path"]
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(item["content"])

archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
binary = evidence / "tree-sitter"
binary.write_bytes(gzip.decompress(archive.read_bytes()))
binary.chmod(0o755)
run("rust-version", ["rustc", "--version"])
run("scanner-syntax", ["cc", "-fsyntax-only", "-std=c11", "-I", str(vendor / "src"), str(vendor / "src/scanner.c")])
run("generate", [str(binary), "generate"], cwd=vendor)
run("format-source", ["cargo", "fmt", "--all"])
for item in authored:
    if item["path"].startswith("crates/"):
        assert len((source / item["path"]).read_text().splitlines()) < 600, item["path"]
states = re.search(r"#define STATE_COUNT (\d+)", (vendor / "src/parser.c").read_text())
assert states is not None
notes = vendor / "SLOPLINT.md"
notes.write_text(re.sub(r"The regenerated parser uses ABI 15 and has [0-9,]+ states.",
                        f"The regenerated parser uses ABI 15 and has {int(states.group(1)):,} states.", notes.read_text()))

upstream = evidence / "upstream"
run("upstream-init", ["git", "init", "-q", str(upstream)])
run("upstream-origin", ["git", "remote", "add", "origin", "https://github.com/airbus-cert/tree-sitter-powershell"], cwd=upstream)
run("upstream-fetch", ["git", "fetch", "--depth=1", "origin", "65ea6919e816a7ac142acec7ee72757f89db8439"], cwd=upstream)
run("upstream-checkout", ["git", "checkout", "-q", "FETCH_HEAD"], cwd=upstream)
previous_patch = evidence / "previous.patch"
previous_patch.write_bytes(old_patch)
run("upstream-patch", ["git", "apply", "--index", str(previous_patch)], cwd=upstream)
for name in ("grammar.js", "src/scanner.c"):
    (upstream / name).write_bytes((vendor / name).read_bytes())
patch = subprocess.check_output(["git", "diff", "--binary", "HEAD", "--", "grammar.js", "src/scanner.c", "test/corpus"], cwd=upstream)
(vendor / "sloplint-grammar.patch").write_bytes(patch)

changed = output(["git", "diff", "--name-only"]).splitlines() + output(["git", "ls-files", "--others", "--exclude-standard"]).splitlines()
changed = sorted(set(changed))
allowed = {item["path"] for item in authored} | {
    "vendor/tree-sitter-powershell/src/parser.c", "vendor/tree-sitter-powershell/src/grammar.json",
    "vendor/tree-sitter-powershell/src/node-types.json", "vendor/tree-sitter-powershell/sloplint-grammar.patch",
}
assert set(changed) <= allowed, changed
index_environment = dict(environment)
index_environment["GIT_INDEX_FILE"] = str(evidence / "candidate.index")
output(["git", "read-tree", "HEAD"], env=index_environment)
output(["git", "add", "--", *changed], env=index_environment)
tree = output(["git", "write-tree"], env=index_environment)
snapshot = {"base": manifest["base"], "tree": tree,
            "files": [{"path": name, "sha256": hashlib.sha256((source / name).read_bytes()).hexdigest()} for name in changed]}
(evidence / "candidate-snapshot.json").write_text(json.dumps(snapshot, indent=2) + "\n")
print("POWERSHELL_CANDIDATE_SNAPSHOT " + json.dumps(snapshot), flush=True)
for item in snapshot["files"]:
    content = (source / item["path"]).read_text()
    size = 16000
    for part, offset in enumerate(range(0, len(content), size)):
        print("GENERATED_SOURCE_CHUNK " + json.dumps({"path": item["path"], "sha256": item["sha256"],
              "part": part, "parts": (len(content) + size - 1) // size, "content": content[offset:offset + size]}), flush=True)

archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
                   "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "powershell.tar.gz")
powershell = evidence / "powershell"
powershell.mkdir()
run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
pwsh = powershell / "pwsh"
pwsh.chmod(0o755)
run("native-grammar-oracle", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(proof / "grammar-oracle.ps1"),
                              "-CasesPath", str(proof / "grammar-cases.json")])
literal = run("native-literal-oracle", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(proof / "literal-oracle.ps1"),
                                       "-CasesPath", str(proof / "literal-cases.json")])
assert "LITERAL_ORACLE_COMPLETE 39" in literal
cases = cast(list[dict[str, object]], json.loads((proof / "grammar-cases.json").read_text()))
case_root = evidence / "cases"
case_root.mkdir()
case_results: list[dict[str, object]] = []
for index, case in enumerate(cases):
    path = case_root / f"case-{index}.ps1"
    path.write_text(str(case["source"]))
    result = subprocess.run([str(binary), "parse", "--quiet", str(path)], cwd=vendor, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
    assert "Failed to load language" not in result.stdout
    accepted = result.returncode == 0
    case_results.append({"id": case["id"], "accepted": accepted, "expected": case["accepted"], "exit": result.returncode})
    if accepted != case["accepted"]:
        print("PARSER_MISMATCH " + json.dumps(case_results[-1]) + "\n" + result.stdout, flush=True)
(evidence / "parser-cases.json").write_text(json.dumps(case_results, indent=2) + "\n")
assert all(item["accepted"] == item["expected"] for item in case_results)
print("PARSER_CASES_PASS " + str(len(cases)), flush=True)
run("grammar-corpus", [str(binary), "test"], cwd=vendor)
environment["GRAMMARS_TS_BIN"] = str(binary)
environment["GRAMMARS_SCRATCH"] = str(evidence / "verify-upstream")
run("grammar-verify", ["bash", "scripts/grammars.sh", "verify", "tree-sitter-powershell"])
run("grammar-manifest", ["bash", "scripts/grammars.sh", "check"])
run("fmt", ["cargo", "fmt", "--all", "--check"])
run("regressions", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "powershell", "--", "--nocapture"])
calls = source / "crates/sloplint-syntax/src/lang/powershell/calls.rs"
words = source / "crates/sloplint-syntax/src/lang/powershell/calls/words.rs"
saved_calls, saved_words = calls.read_bytes(), words.read_bytes()
try:
    calls.write_bytes(old_calls)
    words.write_bytes(old_words)
    red = run("old-cli-fixtures", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures",
                                 "fixtures_match_expected", "--", "--nocapture"], expected=101)
    assert "contiguous-log-literal__powershell" in red
    assert "contiguous-log-literal-near-miss__powershell" in red
finally:
    calls.write_bytes(saved_calls)
    words.write_bytes(saved_words)
run("candidate-cli-fixtures", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures", "fixtures_match_expected", "--", "--nocapture"])
run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"])
run("release", ["cargo", "build", "--release", "--locked"])
run("self-lint", [str(source / "target/release/sloplint"), "check", "crates", "--format", "agent", "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
run("whitespace", ["git", "diff", "--check"])
assert output(["git", "write-tree"], env=index_environment) == tree
for item in snapshot["files"]:
    assert hashlib.sha256((source / item["path"]).read_bytes()).hexdigest() == item["sha256"], item["path"]
current_paths = output(["git", "diff", "--name-only"]).splitlines() + output(["git", "ls-files", "--others", "--exclude-standard"]).splitlines()
assert set(current_paths) == set(changed), current_paths
print("POWERSHELL_CANDIDATE_X64_PASS " + tree, flush=True)
