from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "probe" / "ci-tools"
EVIDENCE = WORKSPACE / "evidence"
VENDOR = SOURCE / "vendor/tree-sitter-powershell"
EVIDENCE.mkdir(exist_ok=True)
RESULTS: dict[str, int] = {}


def run(name: str, args: list[str], cwd: Path = SOURCE, expected: int | None = 0) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=900)
    (EVIDENCE / f"{name}.log").write_text(result.stdout, encoding="utf-8")
    RESULTS[name] = result.returncode
    (EVIDENCE / "results.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
    print(f"RESULT {name}: {result.returncode}", flush=True)
    if result.returncode:
        print(result.stdout[-8000:], flush=True)
    if expected is not None:
        assert result.returncode == expected, (name, result.returncode, expected)
    return result


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def download(url: str, digest: str, name: str) -> Path:
    path = EVIDENCE / name
    with urllib.request.urlopen(url, timeout=60) as response:
        path.write_bytes(response.read())
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    return path


def parser_cases(binary: Path, cases: list[dict[str, str | bool]], phase: str) -> list[str]:
    root = EVIDENCE / "cases"
    root.mkdir(exist_ok=True)
    mismatches: list[str] = []
    records: list[dict[str, str | bool | int]] = []
    for index, case in enumerate(cases):
        path = root / f"case-{index}.ps1"
        path.write_bytes(str(case["source"]).encode())
        result = subprocess.run([str(binary), "parse", "--quiet", str(path)], cwd=VENDOR, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        assert "Failed to load language" not in result.stdout, result.stdout
        accepted = result.returncode == 0
        record: dict[str, str | bool | int] = {"id": str(case["id"]), "accepted": accepted, "expected": bool(case["accepted"]), "exit": result.returncode}
        records.append(record)
        if accepted != case["accepted"]:
            mismatches.append(str(case["id"]))
            print("PARSER_MISMATCH " + json.dumps({"phase": phase, **record}), flush=True)
            (EVIDENCE / f"{phase}-case-{index}.log").write_text(result.stdout)
            if phase == "candidate" and len(mismatches) <= 2:
                diagnostic = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
                print("PARSER_DETAIL " + diagnostic.stdout[:6000], flush=True)
    (EVIDENCE / f"{phase}-cases.json").write_text(json.dumps(records, indent=2) + "\n")
    print("PARSER_CASES " + json.dumps({"phase": phase, "count": len(cases), "mismatches": mismatches}), flush=True)
    return mismatches


def main() -> None:
    manifest = cast(dict[str, str | dict[str, str]], json.loads((PROOF / "remaining-s137-input.json").read_text()))
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("status", "--porcelain") == ""
    cases = cast(list[dict[str, str | bool]], json.loads((PROOF / "remaining-s137-cases.json").read_text()))
    archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz", "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "powershell.tar.gz")
    powershell = EVIDENCE / "powershell"
    powershell.mkdir()
    run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
    (powershell / "pwsh").chmod(0o755)
    oracle = run("powershell-oracle", [str(powershell / "pwsh"), "-NoLogo", "-NoProfile", "-File", str(PROOF / "remaining-s137-oracle.ps1"), "-CasesPath", str(PROOF / "remaining-s137-cases.json")])
    assert "POWERSHELL_ORACLE_COMPLETE" in oracle.stdout
    print("ORACLE " + oracle.stdout, flush=True)
    compressed = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz", "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
    tree_sitter = EVIDENCE / "tree-sitter"
    tree_sitter.write_bytes(gzip.decompress(compressed.read_bytes()))
    tree_sitter.chmod(0o755)
    assert "tree-sitter 0.27.1" in run("tree-sitter-version", [str(tree_sitter), "--version"]).stdout
    baseline = parser_cases(tree_sitter, cases, "baseline")
    assert "multiline-here-lf-8" in baseline
    assert "parenthesized-pipe-lf-0-0-blank" in baseline
    patch = PROOF / "remaining-s137-input.patch"
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    run("apply", ["git", "apply", "--index", "--whitespace=error", str(patch)])
    assert git("write-tree") == manifest["draft_tree"]
    for path, digest in cast(dict[str, str], manifest["input_files"]).items():
        assert hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() == digest, path
    run("scanner-syntax", ["cc", "-fsyntax-only", "-std=c11", "-I", str(VENDOR / "src"), str(VENDOR / "src/scanner.c")])
    run("generate", [str(tree_sitter), "generate"], cwd=VENDOR)
    assert parser_cases(tree_sitter, cases, "candidate") == []
    corpus_path = VENDOR / "test/corpus/sloplint_multiline_boundaries.txt"
    existing_corpus = {path: (SOURCE / path).read_bytes() for path in git("ls-files", "vendor/tree-sitter-powershell/test/corpus/*").splitlines() if SOURCE / path != corpus_path}
    run("corpus-update", [str(tree_sitter), "test", "--update", "--include", "^(multiline-here|parenthesized-pipe)-"], cwd=VENDOR)
    rewritten = [path for path, content in existing_corpus.items() if (SOURCE / path).read_bytes() != content]
    (EVIDENCE / "corpus-formatter-rewrites.json").write_text(json.dumps(rewritten, indent=2) + "\n")
    for path, content in existing_corpus.items():
        (SOURCE / path).write_bytes(content)
    corpus_path.write_text(corpus_path.read_text().rstrip() + "\n")
    run("corpus", [str(tree_sitter), "test"], cwd=VENDOR)
    assert all((SOURCE / path).read_bytes() == content for path, content in existing_corpus.items())
    print("CORPUS_PRESERVED " + json.dumps({"existing_files": len(existing_corpus), "formatter_rewrites_restored": rewritten}), flush=True)
    run("rustfmt", ["rustfmt", "--edition", "2024", "crates/sloplint-syntax/tests/grammar_gaps/powershell_words.rs"])
    upstream = EVIDENCE / "upstream"
    upstream.mkdir()
    run("upstream-init", ["git", "init", "-q"], cwd=upstream)
    run("upstream-remote", ["git", "remote", "add", "origin", "https://github.com/airbus-cert/tree-sitter-powershell"], cwd=upstream)
    run("upstream-fetch", ["git", "fetch", "--depth=1", "origin", "65ea6919e816a7ac142acec7ee72757f89db8439"], cwd=upstream)
    run("upstream-checkout", ["git", "checkout", "-q", "FETCH_HEAD"], cwd=upstream)
    for path in ["grammar.js", "src/scanner.c"]:
        (upstream / path).write_bytes((VENDOR / path).read_bytes())
    patch_bytes = subprocess.check_output(["git", "-c", "diff.suppressBlankEmpty=true", "diff", "--binary", "--", "grammar.js", "src/scanner.c"], cwd=upstream)
    (VENDOR / "sloplint-grammar.patch").write_bytes(patch_bytes)
    os.environ["GRAMMARS_TS_BIN"] = str(tree_sitter)
    os.environ["GRAMMARS_SCRATCH"] = str(EVIDENCE / "grammar-reproduction")
    run("grammar-verify", ["bash", "scripts/grammars.sh", "verify", "tree-sitter-powershell"])
    allowed = set(cast(dict[str, str], manifest["input_files"])) | {
        "vendor/tree-sitter-powershell/src/grammar.json",
        "vendor/tree-sitter-powershell/src/node-types.json",
        "vendor/tree-sitter-powershell/src/parser.c",
        "vendor/tree-sitter-powershell/sloplint-grammar.patch",
    }
    changed = set(git("diff", "--name-only", "HEAD").splitlines())
    assert changed <= allowed, sorted(changed - allowed)
    assert git("ls-files", "--others", "--exclude-standard") == ""
    subprocess.run(["git", "add", "--", *sorted(changed)], cwd=SOURCE, check=True)
    run("diff-check", ["git", "diff", "--cached", "--check"])
    files = {path: (SOURCE / path).read_text(encoding="utf-8") for path in sorted(changed)}
    payload = {"head": manifest["head"], "base": manifest["base"], "draft_tree": manifest["draft_tree"], "tree": git("write-tree"), "files": files, "sha256": {path: hashlib.sha256(content.encode()).hexdigest() for path, content in files.items()}, "results": RESULTS}
    (EVIDENCE / "prepared.json").write_text(json.dumps(payload, ensure_ascii=True) + "\n", encoding="utf-8")
    summary = {key: value for key, value in payload.items() if key != "files"}
    print("PREPARED " + json.dumps(summary), flush=True)
    text = json.dumps(payload, ensure_ascii=True)
    width = 1000
    total = (len(text) + width - 1) // width
    for index in range(total):
        print("PAYLOAD_CHUNK " + json.dumps({"label": "s137-grammar", "index": index, "total": total, "data": text[index * width:(index + 1) * width]}), flush=True)


if __name__ == "__main__":
    main()
