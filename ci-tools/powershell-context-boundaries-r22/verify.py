from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import urllib.request
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PREVIOUS = WORKSPACE / "pre_source"
LEGACY = WORKSPACE / "legacy_source"
PROOF = WORKSPACE / "proof/ci-tools/powershell-context-boundaries-r22"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-context-boundaries-r22"
EVIDENCE.mkdir(exist_ok=True)
MANIFEST = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
ENVIRONMENT = dict(os.environ)
ENVIRONMENT.update(RUSTUP_TOOLCHAIN="1.100.0", CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="8", CARGO_TERM_COLOR="never",
                   CARGO_PROFILE_DEV_DEBUG="line-tables-only", CARGO_PROFILE_TEST_DEBUG="line-tables-only",
                   SLOPLINT_REQUIRE_FS_CAPS="1")
RESULTS: list[dict[str, object]] = []

def output(args: list[str], cwd: Path = SOURCE) -> str:
    return subprocess.check_output(args, cwd=cwd, env=ENVIRONMENT, text=True).strip()

def run(name: str, args: list[str], expected: int | None = 0, cwd: Path = SOURCE, limit: int = 1200) -> str:
    started = time.monotonic()
    process = subprocess.Popen(args, cwd=cwd, env=ENVIRONMENT, text=True, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, start_new_session=True)
    timed_out = False
    try:
        text, _ = process.communicate(timeout=limit)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        text, _ = process.communicate()
    (EVIDENCE / f"{name}.log").write_text(text)
    RESULTS.append({"name": name, "args": args, "cwd": str(cwd), "exit": process.returncode, "expected": expected,
                    "seconds": time.monotonic() - started, "timed_out": timed_out,
                    "log_sha256": hashlib.sha256(text.encode()).hexdigest()})
    (EVIDENCE / "results.json").write_text(json.dumps({"manifest": MANIFEST, "results": RESULTS}, indent=2) + "\n")
    print(f"BASELINE_EXIT {name} {process.returncode} timed_out={timed_out}\n{text[-25000:]}", flush=True)
    assert not timed_out and (expected is None or process.returncode == expected), (name, process.returncode, expected, timed_out)
    return text

def download(url: str, digest: str, name: str) -> Path:
    path = EVIDENCE / name
    with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    print("VERIFIED_DOWNLOAD " + name + " " + digest, flush=True)
    return path

def identity(source: Path, head: str, tree: str) -> dict[str, object]:
    assert output(["git", "rev-parse", "HEAD"], source) == head
    assert output(["git", "write-tree"], source) == tree
    assert output(["git", "status", "--porcelain", "--untracked-files=all"], source) == ""
    index = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=source)
    count = 0
    for entry in index.split(b"\0"):
        if not entry:
            continue
        metadata, relative = entry.split(b"\t", 1)
        mode, expected, stage = metadata.split()
        assert stage == b"0"
        path = source / os.fsdecode(relative)
        if mode == b"120000":
            content = os.fsencode(os.readlink(path))
        else:
            assert mode in (b"100644", b"100755")
            assert bool(path.stat().st_mode & 0o111) == (mode == b"100755")
            content = path.read_bytes()
        assert hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest() == expected.decode(), str(path)
        count += 1
    record = {"head": head, "tree": tree, "tracked_blobs": count, "index_sha256": hashlib.sha256(index).hexdigest()}
    print("UNCHANGED_SOURCE_IDENTITY " + json.dumps(record, sort_keys=True), flush=True)
    return record

for name, digest in cast(dict[str, str], MANIFEST["proof_sha256"]).items():
    assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
snapshots = [
    ("legacy", LEGACY, str(MANIFEST["legacy_base"]), str(MANIFEST["legacy_tree"])),
    ("previous", PREVIOUS, str(MANIFEST["pre_base"]), str(MANIFEST["pre_tree"])),
    ("current", SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"])),
]
initial = {name: identity(source, head, tree) for name, source, head, tree in snapshots}
run("rust-toolchain", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
assert output(["rustc", "--version"]).startswith("rustc 1.100.0 ")
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
binary = EVIDENCE / "tree-sitter"
binary.write_bytes(gzip.decompress(archive.read_bytes()))
binary.chmod(0o755)
archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
                   "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "powershell.tar.gz")
powershell = EVIDENCE / "powershell"
powershell.mkdir()
run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
pwsh = powershell / "pwsh"
pwsh.chmod(0o755)
native_bin = EVIDENCE / "native-bin"
native_bin.mkdir()
capture = native_bin / "Capture-Native"
capture.write_text(f"#!{sys.executable}\nimport json\nimport sys\nprint(json.dumps({{'count': len(sys.argv) - 1, 'values': sys.argv[1:]}}, ensure_ascii=True))\n")
capture.chmod(0o755)
ENVIRONMENT["PATH"] = str(native_bin) + os.pathsep + ENVIRONMENT["PATH"]
native_output = run("native-witnesses", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "native-oracle.ps1"), "-CasesPath", str(PROOF / "cases.json")])
native = [json.loads(line.removeprefix("NATIVE_CONTEXT ")) for line in native_output.splitlines() if line.startswith("NATIVE_CONTEXT ")]
cases = cast(list[dict[str, object]], json.loads((PROOF / "cases.json").read_text()))
assert len(native) == len(cases) == MANIFEST["native_cases"]
assert all(record["matches"] for record in native)
(EVIDENCE / "native-cases.json").write_text(json.dumps(native, indent=2, sort_keys=True) + "\n")
for record in native:
    print("FROZEN_NATIVE " + json.dumps(record), flush=True)
case_root = EVIDENCE / "cases"
case_root.mkdir()
native_by_id = {record["id"]: record for record in native}

def spans(kind: str, content: str, tree_text: str) -> list[str]:
    encoded = content.encode()
    lines = encoded.splitlines(keepends=True)
    result = []
    for match in re.finditer(rf"\({kind} \[(\d+), (\d+)\] - \[(\d+), (\d+)\]", tree_text):
        start_row, start_column, end_row, end_column = map(int, match.groups())
        begin = sum(map(len, lines[:start_row])) + start_column
        finish = sum(map(len, lines[:end_row])) + end_column
        result.append(encoded[begin:finish].decode())
    return result

for label, source, head, tree in snapshots:
    library = EVIDENCE / f"parser-libraries-{label}"
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    vendor = source / "vendor/tree-sitter-powershell"
    records: list[dict[str, object]] = []
    for index, case in enumerate(cases):
        content = str(case["source"])
        assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"], case["id"]
        path = case_root / f"case-{index}.ps1"
        path.write_text(content)
        result = subprocess.run([str(binary), "parse", str(path)], cwd=vendor, env=ENVIRONMENT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        assert "Failed to load language" not in result.stdout
        expected = native_by_id[case["id"]]
        commands = spans("command", content, result.stdout)
        record: dict[str, object] = {"label": label, "id": case["id"], "exit": result.returncode,
                                    "accepted": result.returncode == 0, "native_accepted": expected["accepted"],
                                    "commands": commands, "native_commands": [command["text"] for command in expected["commands"]]}
        record["acceptance_matches"] = record["accepted"] == expected["accepted"]
        if expected["accepted"] and record["accepted"]:
            record["command_extents_match"] = commands == record["native_commands"]
            record["elseifs"] = len(re.findall(r"\(elseif_clause ", result.stdout))
            record["elses"] = len(re.findall(r"\(else_clause ", result.stdout))
            record["branch_structure_matches"] = record["elseifs"] == sum(branch["clauses"] - 1 for branch in expected["branches"]) and record["elses"] == sum(int(branch["has_else"]) for branch in expected["branches"])
        if case["kind"] == "redirect":
            record["generic_words"] = spans("generic_token", content, result.stdout)
            record["target_nodes"] = spans("redirected_file_name", content, result.stdout)
        records.append(record)
        print("PARSER_OBSERVATION " + json.dumps(record), flush=True)
    (EVIDENCE / f"{label}-parser-cases.json").write_text(json.dumps(records, indent=2, sort_keys=True) + "\n")
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in library.glob("*.so")]
    assert len(libraries) == 1, libraries
    print("ISOLATED_PARSER_LIBRARY " + json.dumps({"source": initial[label], "libraries": libraries}), flush=True)
    assert identity(source, head, tree) == initial[label]

trace_ids = set(cast(list[str], MANIFEST["trace_cases"]))
for label, source, head, tree in snapshots:
    scanner = source / "vendor/tree-sitter-powershell/src/scanner.c"
    saved = scanner.read_bytes()
    original = saved.decode()
    dispatch = "    if (scan_bare_dashes(payload, lexer, valid_symbols)) return true;\n    return scan_statement_terminator(payload, lexer, valid_symbols);"
    instrumented_dispatch = """
    fprintf(stderr, "EXT_ENTRY col=%u char=%04x valid=%d%d%d%d%d\\n", lexer->get_column(lexer), lexer->lookahead, valid_symbols[0], valid_symbols[1], valid_symbols[2], valid_symbols[3], valid_symbols[4]);
    bool bare = scan_bare_dashes(payload, lexer, valid_symbols);
    fprintf(stderr, "EXT_AFTER_BARE accepted=%d col=%u char=%04x result=%d\\n", bare, lexer->get_column(lexer), lexer->lookahead, lexer->result_symbol);
    if (bare) return true;
    bool terminator = scan_statement_terminator(payload, lexer, valid_symbols);
    fprintf(stderr, "EXT_AFTER_TERMINATOR accepted=%d col=%u char=%04x result=%d\\n", terminator, lexer->get_column(lexer), lexer->lookahead, lexer->result_symbol);
    return terminator;"""
    assert original.count(dispatch) == 1
    instrumented = original.replace("#include <wctype.h>", "#include <wctype.h>\n#include <stdio.h>").replace(dispatch, instrumented_dispatch)
    library = EVIDENCE / f"trace-libraries-{label}"
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    try:
        scanner.write_text(instrumented)
        print("TRACE_SOURCE_IDENTITY " + json.dumps({"label": label, "base": head, "original_sha256": hashlib.sha256(saved).hexdigest(), "instrumented_sha256": hashlib.sha256(instrumented.encode()).hexdigest()}), flush=True)
        for index, case in enumerate(cases):
            if case["id"] in trace_ids:
                path = case_root / f"case-{index}.ps1"
                run(f"trace-{label}-{case['id']}", [str(binary), "parse", str(path)], expected=None, cwd=source / "vendor/tree-sitter-powershell", limit=60)
    finally:
        scanner.write_bytes(saved)
        assert identity(source, head, tree) == initial[label]
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in library.glob("*.so")]
    assert len(libraries) == 1
    print("ISOLATED_TRACE_LIBRARY " + json.dumps({"source": initial[label], "libraries": libraries}), flush=True)

modules = cast(list[dict[str, str]], json.loads((PROOF / "frozen-modules.json").read_text()))
for label, source, head, tree in snapshots:
    module_file = source / "crates/sloplint-syntax/tests/grammar_gaps.rs"
    saved_module = module_file.read_bytes()
    saved: dict[str, bytes | None] = {}
    try:
        for module in modules:
            content = module["content"].encode()
            assert hashlib.sha256(content).hexdigest() == module["sha256"]
            path = source / module["path"]
            saved[module["path"]] = path.read_bytes() if path.exists() else None
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        anchor = '#[path = "grammar_gaps/powershell.rs"]\nmod powershell;\n'
        text = module_file.read_text()
        assert text.count(anchor) == 1
        additions = "".join(f'#[path = "grammar_gaps/{module["name"]}.rs"]\nmod {module["name"]};\n' for module in modules)
        module_file.write_text(text.replace(anchor, anchor + additions))
        run(label + "-format", ["cargo", "fmt", "--all"], cwd=source)
        frozen = [{"path": module["path"], "content": (source / module["path"]).read_text(),
                   "sha256": hashlib.sha256((source / module["path"]).read_bytes()).hexdigest()} for module in modules]
        frozen.append({"path": "crates/sloplint-syntax/tests/grammar_gaps.rs", "content": module_file.read_text(),
                       "sha256": hashlib.sha256(module_file.read_bytes()).hexdigest()})
        (EVIDENCE / f"{label}-formatted-frozen-elements.json").write_text(json.dumps(frozen, indent=2) + "\n")
        for item in frozen:
            assert len(item["content"].splitlines()) < 600, item["path"]
            print("FROZEN_TEST_SOURCE " + json.dumps({"label": label, **item}), flush=True)
        args = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps"]
        run(label + "-frozen-tests-compile", [*args, "--no-run"], cwd=source)
        tests = [
            ("continued-dashes", "powershell_context_boundaries::indented_bare_dashes_follow_separated_line_continuations", None),
            ("keyword-command", "powershell_context_boundaries::keyword_like_executable_after_an_escaped_line_is_a_separate_call", None),
            ("redirect-query", "powershell_context_boundaries::spaced_dash_redirection_retains_its_legacy_separator_capture", None),
            ("query-observation", "powershell_context_boundaries::observe_public_redirection_queries", 0),
        ]
        for name, test, expected in tests:
            run(label + "-" + name, [*args, test, "--", "--exact", "--nocapture"], expected=expected, cwd=source)
    finally:
        module_file.write_bytes(saved_module)
        for relative, content in saved.items():
            path = source / relative
            if content is None:
                path.unlink()
            else:
                path.write_bytes(content)
        assert identity(source, head, tree) == initial[label]
print("FROZEN_CONTEXT_BASELINE_COMPLETE " + json.dumps({"sources": initial, "native_cases": len(native), "results": RESULTS}, sort_keys=True), flush=True)
