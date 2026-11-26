from __future__ import annotations

import base64
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
import zipfile
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PREVIOUS = WORKSPACE / "pre_source"
PROOF = WORKSPACE / "proof/ci-tools/powershell-separator-ownership-r23"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-separator-ownership-r23"
EVIDENCE.mkdir(exist_ok=True)
MANIFEST = cast(dict[str, object], json.loads((PROOF / "guard-boundary-manifest.json").read_text()))
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
    ("previous", PREVIOUS, str(MANIFEST["previous_base"]), str(MANIFEST["previous_tree"])),
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


failure_root = EVIDENCE / "failed-baseline"
archives = [path for path in failure_root.rglob("*") if path.is_file() and zipfile.is_zipfile(path)]
assert len(archives) == 1
archive = archives[0]
assert hashlib.sha256(archive.read_bytes()).hexdigest() == "3491cb4e240d43bfc1dc11795a0afd02f76844cb41d5ea3267b206e289de5b4a"
with zipfile.ZipFile(archive) as handle:
    def member(name: str) -> bytes:
        matches = [entry for entry in handle.namelist() if Path(entry).name == name]
        assert len(matches) == 1, name
        return handle.read(matches[0])
    saved = json.loads(member("results.json"))
    failed = next(result for result in saved["results"] if result["name"] == "native-witnesses")
    assert failed["exit"] == 1 and failed["expected"] == 0 and not failed["timed_out"]
    original_log = member("native-witnesses.log")
    assert hashlib.sha256(original_log).hexdigest() == failed["log_sha256"]
    original = [json.loads(line.removeprefix("NATIVE_CONTEXT ")) for line in original_log.decode().splitlines() if line.startswith("NATIVE_CONTEXT ")]
assert len(original) == 63
before = json.loads((PROOF / "guard-boundary-before-cases.json").read_text())
after = json.loads((PROOF / "guard-boundary-cases.json").read_text())
assert saved["manifest"]["proof_sha256"]["guard-boundary-cases.json"] == hashlib.sha256((PROOF / "guard-boundary-before-cases.json").read_bytes()).hexdigest()
for name in ("guard-boundary-cases.tsv", "guard-boundary-modules.json", "native-oracle.ps1"):
    assert saved["manifest"]["proof_sha256"][name] == MANIFEST["proof_sha256"][name], name
corrections = []
for old, new, actual in zip(before, after, original, strict=True):
    assert old["id"] == new["id"] == actual["id"]
    assert old["source_sha256"] == new["source_sha256"] == actual["source_sha256"]
    if old == new:
        assert actual["matches"]
        continue
    assert not actual["matches"] and old["id"].endswith("-array")
    assert {key: value for key, value in old.items() if key != "expected_values"} == {key: value for key, value in new.items() if key != "expected_values"}
    assert actual["accepted"] and actual["native_count"] == 2
    assert new["expected_values"] == actual["native_values"]
    assert new["argument_sources"] == [node["text"] for node in actual["argument_nodes"]]
    record = {"id": old["id"], "source_sha256": old["source_sha256"], "before_expected_values": old["expected_values"],
              "after_expected_values": new["expected_values"], "actual_native": actual}
    corrections.append(record)
    print("BOUNDARY_EXPECTATION_CORRECTION " + json.dumps(record), flush=True)
assert len(corrections) == 6
(EVIDENCE / "observer-correction.json").write_text(json.dumps({"failed_producer": "bd88e818128fca3332205eb4bb895cd164c960d4",
    "failed_run": 45907185583, "artifact": 15158213198, "failed_result": failed,
    "native_log_sha256": failed["log_sha256"], "corrections": corrections}, indent=2) + "\n")

native_output = run("native-witnesses", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "native-oracle.ps1"), "-CasesPath", str(PROOF / "guard-boundary-cases.json")])
native = [json.loads(line.removeprefix("NATIVE_CONTEXT ")) for line in native_output.splitlines() if line.startswith("NATIVE_CONTEXT ")]
cases = cast(list[dict[str, object]], json.loads((PROOF / "guard-boundary-cases.json").read_text()))
assert len(native) == len(cases) == MANIFEST["native_cases"]
assert all(record["matches"] for record in native)
assert all(record["accepted"] for record in native if record["id"] in MANIFEST["primary_ids"])
(EVIDENCE / "native-cases.json").write_text(json.dumps(native, indent=2, sort_keys=True) + "\n")
for record in native:
    print("FROZEN_NATIVE " + json.dumps(record), flush=True)
case_root = EVIDENCE / "cases"
case_root.mkdir()
native_by_id = {record["id"]: record for record in native}
all_parsers: dict[str, list[dict[str, object]]] = {}
all_queries: dict[str, list[str]] = {}
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
        assert result.returncode in (0, 1), (case["id"], result.returncode)
        assert "Failed to load language" not in result.stdout
        expected = native_by_id[case["id"]]
        commands = spans("command", content, result.stdout)
        record: dict[str, object] = {
            "label": label, "id": case["id"], "exit": result.returncode,
            "accepted": result.returncode == 0, "native_accepted": expected["accepted"],
            "commands": commands, "native_commands": [command["text"] for command in expected["commands"]],
            "generic_words": spans("generic_token", content, result.stdout),
            "target_nodes": spans("redirected_file_name", content, result.stdout),
            "separator_nodes": spans("command_argument_sep", content, result.stdout),
            "comments": spans("comment", content, result.stdout),
            "arrays": spans("array_literal_expression", content, result.stdout),
            "tree_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
        }
        record["acceptance_matches"] = record["accepted"] == expected["accepted"]
        records.append(record)
        print("OWNERSHIP_PARSER_OBSERVATION " + json.dumps(record), flush=True)
        if case["id"] in MANIFEST["primary_ids"]:
            (EVIDENCE / f"{label}-{case['id']}-tree.log").write_text(result.stdout)
            print("PRIMARY_PARSER_TREE " + label + " " + str(case["id"]) + "\n" + result.stdout, flush=True)
    all_parsers[label] = records
    (EVIDENCE / f"{label}-parser-cases.json").write_text(json.dumps(records, indent=2, sort_keys=True) + "\n")
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in library.glob("*.so")]
    assert len(libraries) == 1, libraries
    print("ISOLATED_PARSER_LIBRARY " + json.dumps({"source": initial[label], "libraries": libraries}), flush=True)
    assert identity(source, head, tree) == initial[label]

ENVIRONMENT["SLOPLINT_BOUNDARY_CASES"] = str(PROOF / "guard-boundary-cases.tsv")
modules = cast(list[dict[str, str]], json.loads((PROOF / "guard-boundary-modules.json").read_text()))
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
            assert saved[module["path"]] is None
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
        text = run(label + "-query-observation", [*args, "powershell_separator_observations::observe_separator_ownership_queries", "--", "--exact", "--nocapture"], cwd=source)
        assert "running 1 test" in text and "1 passed; 0 failed" in text
        queries = [line for line in text.splitlines() if line.startswith("OWNERSHIP_QUERY_")]
        assert sum(line.startswith("OWNERSHIP_QUERY_COMPILE ") and line.endswith(" OK") for line in queries) == 19
        assert sum(line.startswith("OWNERSHIP_QUERY_ROW ") for line in queries) == 19 * len(cases)
        all_queries[label] = queries
    finally:
        module_file.write_bytes(saved_module)
        for relative, content in saved.items():
            path = source / relative
            if content is None:
                path.unlink()
            else:
                path.write_bytes(content)
        assert identity(source, head, tree) == initial[label]
payload = json.dumps({"manifest": MANIFEST, "sources": initial, "native": native, "parsers": all_parsers,
                      "queries": all_queries, "results": RESULTS}, separators=(",", ":"), ensure_ascii=True).encode()
packed = gzip.compress(payload, mtime=0)
encoded = base64.b64encode(packed).decode()
(EVIDENCE / "guard-boundary-packed.json.gz").write_bytes(packed)
(EVIDENCE / "guard-boundary-complete.json").write_bytes(payload)
transport = {"payload_sha256": hashlib.sha256(payload).hexdigest(), "gzip_sha256": hashlib.sha256(packed).hexdigest(),
             "payload_bytes": len(payload), "gzip_bytes": len(packed), "base64_bytes": len(encoded)}
(EVIDENCE / "packed-identity.json").write_text(json.dumps(transport, indent=2) + "\n")
print("OWNERSHIP_GUARD_BASELINE_IDENTITY " + json.dumps(transport), flush=True)
for index in range(0, len(encoded), 24000):
    print("OWNERSHIP_GUARD_BASELINE_CHUNK " + json.dumps({"part": index // 24000, "data": encoded[index:index + 24000]}), flush=True)
print("OWNERSHIP_GUARD_BASELINE_COMPLETE " + json.dumps({"sources": initial, "native_cases": len(native), "results": RESULTS}, sort_keys=True), flush=True)
