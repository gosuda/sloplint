from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request
from typing import cast

workspace = Path(os.environ["GITHUB_WORKSPACE"])
source = workspace / "source"
proof = workspace / "proof/ci-tools/powershell-round14"
evidence = Path(os.environ["RUNNER_TEMP"]) / "powershell-round14"
evidence.mkdir(exist_ok=True)
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
environment = dict(os.environ)

def run(name: str, args: list[str], cwd: Path = source) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=cwd, env=environment, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=1200, check=False)
    (evidence / (name + ".log")).write_text(result.stdout)
    print(f"EXIT {name} {result.returncode}\n{result.stdout[-14000:]}", flush=True)
    return result

def download(url: str, digest: str, name: str) -> Path:
    path = evidence / name
    with urllib.request.urlopen(url, timeout=120) as response:
        path.write_bytes(response.read())
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest
    return path

def identity() -> None:
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() == manifest["head"]
    assert subprocess.check_output(["git", "write-tree"], cwd=source, text=True).strip() == manifest["tree"]
    assert subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=source, text=True).strip() == ""

for relative, digest in cast(dict[str, str], manifest["files"]).items():
    assert hashlib.sha256((proof / relative).read_bytes()).hexdigest() == digest, relative
identity()
archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
                   "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "pwsh.tar.gz")
pwsh = evidence / "powershell"
pwsh.mkdir()
assert run("extract-pwsh", ["tar", "-xzf", str(archive), "-C", str(pwsh)]).returncode == 0
(pwsh / "pwsh").chmod(0o755)
native = run("native-grammar", [str(pwsh / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(proof / "grammar-oracle.ps1"), "-CasesPath", str(proof / "grammar-cases.json")])
assert native.returncode == 0 and "NATIVE_GRAMMAR_COMPLETE 27" in native.stdout
native_cases = {record["id"]: record for line in native.stdout.splitlines() if line.startswith("NATIVE_GRAMMAR ")
                for record in [cast(dict[str, object], json.loads(line.removeprefix("NATIVE_GRAMMAR ")))]}
assert len(native_cases) == 27
literal = run("native-literal", [str(pwsh / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(proof / "literal-oracle.ps1"), "-CasesPath", str(proof / "literal-cases.json")])
assert literal.returncode == 0 and "LITERAL_ORACLE_COMPLETE 38" in literal.stdout
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
cli = evidence / "tree-sitter"
cli.write_bytes(gzip.decompress(archive.read_bytes()))
cli.chmod(0o755)
vendor = source / "vendor/tree-sitter-powershell"
cases = cast(list[dict[str, object]], json.loads((proof / "grammar-cases.json").read_text()))
records: list[dict[str, object]] = []
for index, case in enumerate(cases):
    path = evidence / f"grammar-{index}.ps1"
    path.write_text(cast(str, case["source"]))
    result = run(f"parser-{index}", [str(cli), "parse", "--quiet", str(path)], vendor)
    actual = native_cases[cast(str, case["id"])]
    record = {"id": case["id"], "native": actual["accepted"], "old_parser": result.returncode == 0,
              "initial_hypothesis": case["accepted"], "native_errors": actual["errors"]}
    records.append(record)
    print("GRAMMAR_BASELINE " + json.dumps(record, sort_keys=True), flush=True)
(evidence / "grammar-baseline.json").write_text(json.dumps(records, indent=2) + "\n")
target = source / "crates/sloplint-syntax/tests/grammar_gaps/powershell_literal_runs.rs"
original = target.read_bytes()
try:
    target.write_bytes(original + (proof / "tests.rs.txt").read_bytes())
    result = run("old-literal-shapes", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps",
                                      "powershell_literal_runs", "--", "--nocapture"])
    assert result.returncode == 101
    for name in ("uppercase_escape_spelling_stays_searchable", "escaped_quotes_and_dollars_stay_literal", "a_hidden_subexpression_is_one_dynamic_hole", "a_comment_does_not_merge_separate_calls"):
        assert f"{name} ... FAILED" in result.stdout
finally:
    target.write_bytes(original)
    identity()
print("POWERSHELL_95_BASELINE_COMPLETE", flush=True)
