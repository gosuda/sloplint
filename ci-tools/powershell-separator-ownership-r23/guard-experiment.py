from __future__ import annotations

import base64
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import time
import urllib.request
import zipfile
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "proof/ci-tools/powershell-separator-ownership-r23"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-separator-ownership-r23"
EVIDENCE.mkdir(exist_ok=True)
MANIFEST = cast(dict[str, object], json.loads((PROOF / "guard-experiment-manifest.json").read_text()))
ENVIRONMENT = dict(os.environ)
ENVIRONMENT.update(RUSTUP_TOOLCHAIN="1.100.0", CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="8", CARGO_TERM_COLOR="never",
                   CARGO_PROFILE_DEV_DEBUG="line-tables-only", CARGO_PROFILE_TEST_DEBUG="line-tables-only",
                   SLOPLINT_REQUIRE_FS_CAPS="1")
INDEX_ENVIRONMENT = dict(ENVIRONMENT)
INDEX_ENVIRONMENT["GIT_INDEX_FILE"] = str(EVIDENCE / "experiment.index")
RESULTS: list[dict[str, object]] = []

def output(args: list[str], cwd: Path = SOURCE, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=cwd, env=ENVIRONMENT if env is None else env, text=True).strip()

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
    print(f"EXPERIMENT_EXIT {name} {process.returncode} timed_out={timed_out}\n{text[-25000:]}", flush=True)
    assert not timed_out and (expected is None or process.returncode == expected), (name, process.returncode, expected, timed_out)
    return text

def download(url: str, digest: str, name: str) -> Path:
    path = EVIDENCE / name
    with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    print("VERIFIED_DOWNLOAD " + name + " " + digest, flush=True)
    return path

def unchanged_identity(source: Path, head: str, tree: str) -> dict[str, object]:
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


def full_identity(tree: str, paths: set[str]) -> dict[str, object]:
    assert output(["git", "rev-parse", "HEAD"]) == MANIFEST["base"]
    assert output(["git", "write-tree"], env=INDEX_ENVIRONMENT) == tree
    index = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=SOURCE, env=INDEX_ENVIRONMENT)
    count = 0
    for entry in index.split(b"\0"):
        if not entry:
            continue
        metadata, relative = entry.split(b"\t", 1)
        mode, expected, stage = metadata.split()
        assert stage == b"0"
        path = SOURCE / os.fsdecode(relative)
        if mode == b"120000":
            content = os.fsencode(os.readlink(path))
        else:
            assert mode in (b"100644", b"100755"), mode
            content = path.read_bytes()
            assert bool(path.stat().st_mode & 0o111) == (mode == b"100755"), str(path)
        actual = hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest()
        assert actual == expected.decode(), os.fsdecode(relative)
        count += 1
    changed = set(output(["git", "diff", "--name-only"]).splitlines())
    changed.update(output(["git", "ls-files", "--others", "--exclude-standard"]).splitlines())
    assert changed == paths, (sorted(changed), sorted(paths))
    result = {"base": MANIFEST["base"], "tree": tree, "tracked_blobs": count,
              "index_sha256": hashlib.sha256(index).hexdigest()}
    print("EXACT_SOURCE_IDENTITY " + json.dumps(result, sort_keys=True), flush=True)
    return result


VENDOR = SOURCE / "vendor/tree-sitter-powershell"
PATHS = ["vendor/tree-sitter-powershell/grammar.js", "vendor/tree-sitter-powershell/src/grammar.json",
         "vendor/tree-sitter-powershell/src/parser.c", "vendor/tree-sitter-powershell/src/node-types.json",
         "vendor/tree-sitter-powershell/src/scanner.c"]
ORIGINAL = {path: (SOURCE / path).read_bytes() for path in PATHS}
FROZEN_REGRESSION = (PROOF / "guard-regression.rs").read_bytes()
FROZEN_OBSERVER = (PROOF / "external-observer.rs").read_bytes()
CASES = cast(list[dict[str, object]], json.loads((PROOF / "external-boundary-cases.json").read_text()))
CONTROLS = cast(dict[str, object], json.loads((PROOF / "experiment-controls.json").read_text()))
for name, digest in cast(dict[str, str], MANIFEST["proof_sha256"]).items():
    assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
packed = base64.b64decode((PROOF / "baseline-packed.b64").read_text(), validate=False)
assert len(packed) == CONTROLS["packed"]["gzip_bytes"]
assert hashlib.sha256(packed).hexdigest() == CONTROLS["packed"]["gzip_sha256"]
reference_bytes = gzip.decompress(packed)
assert len(reference_bytes) == CONTROLS["packed"]["payload_bytes"]
assert hashlib.sha256(reference_bytes).hexdigest() == CONTROLS["packed"]["payload_sha256"]
REFERENCE = cast(dict[str, object], json.loads(reference_bytes))
assert REFERENCE["sources"]["current"]["head"] == "57adc9fff126a9826e0fb0be921c28e6cdb08391"
assert REFERENCE["sources"]["current"]["tree"] == "f518b3943c6b86af53c334175309303c3d79f38b"
print("VERIFIED_FROZEN_BASELINE " + json.dumps(CONTROLS["baseline"], sort_keys=True), flush=True)
initial = unchanged_identity(SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"]))
run("rust-toolchain", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
assert output(["rustc", "--version"]).startswith("rustc 1.100.0 ")
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
binary = EVIDENCE / "tree-sitter"
binary.write_bytes(gzip.decompress(archive.read_bytes()))
binary.chmod(0o755)
ENVIRONMENT["SLOPLINT_BOUNDARY_CASES"] = str(PROOF / "external-boundary-cases.tsv")
case_root = EVIDENCE / "cases"
case_root.mkdir(exist_ok=True)
for index, case in enumerate(CASES):
    content = str(case["source"])
    assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"]
    (case_root / f"case-{index}.ps1").write_text(content)
native_by_id = {record["id"]: record for record in REFERENCE["native"]}
case_by_id = {case["id"]: case for case in CASES}
outcomes: list[dict[str, object]] = []

def spans(kind: str, content: str, tree_text: str) -> list[str]:
    encoded = content.encode()
    lines = encoded.splitlines(keepends=True)
    result: list[str] = []
    for match in re.finditer(rf"\({kind} \[(\d+), (\d+)\] - \[(\d+), (\d+)\]", tree_text):
        start_row, start_column, end_row, end_column = map(int, match.groups())
        begin = sum(map(len, lines[:start_row])) + start_column
        finish = sum(map(len, lines[:end_row])) + end_column
        result.append(encoded[begin:finish].decode())
    return result

def source_snapshot() -> dict[str, object]:
    changed = set(output(["git", "diff", "--name-only"]).splitlines())
    assert changed <= set(PATHS), sorted(changed)
    output(["git", "read-tree", "HEAD"], env=INDEX_ENVIRONMENT)
    if changed:
        output(["git", "add", "--", *sorted(changed)], env=INDEX_ENVIRONMENT)
    tree = output(["git", "write-tree"], env=INDEX_ENVIRONMENT)
    result = full_identity(tree, changed)
    result["files"] = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in PATHS}
    return result

def parse_cases(label: str) -> list[dict[str, object]]:
    library = EVIDENCE / ("libraries-" + label)
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    records: list[dict[str, object]] = []
    for index, case in enumerate(CASES):
        path = case_root / f"case-{index}.ps1"
        result = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, env=ENVIRONMENT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30, check=False)
        assert result.returncode in (0, 1), (label, case["id"], result.returncode)
        assert "Failed to load language" not in result.stdout
        content = str(case["source"])
        canonical_tree = "\n".join(line for line in result.stdout.splitlines() if re.match(r"^\s*(?:[A-Za-z_]+: )?\(", line)) + "\n"
        record = {"id": case["id"], "exit": result.returncode, "accepted": result.returncode == 0,
                  "commands": spans("command", content, result.stdout),
                  "generic_words": spans("generic_token", content, result.stdout),
                  "target_nodes": spans("redirected_file_name", content, result.stdout),
                  "redirections": spans("redirection", content, result.stdout),
                  "arrays": spans("array_literal_expression", content, result.stdout),
                  "separator_nodes": spans("command_argument_sep", content, result.stdout),
                  "comments": spans("comment", content, result.stdout),
                  "tree_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
                  "canonical_tree_sha256": hashlib.sha256(canonical_tree.encode()).hexdigest()}
        records.append(record)
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in library.glob("*.so")]
    assert len(libraries) == 1
    print("EXPERIMENT_PARSER_LIBRARY " + json.dumps({"label": label, "libraries": libraries}), flush=True)
    (EVIDENCE / (label + "-parsers.json")).write_text(json.dumps(records, indent=2) + "\n")
    return records

def query_observations(label: str, focused: bool) -> tuple[list[str], int | None]:
    module_file = SOURCE / "crates/sloplint-syntax/tests/grammar_gaps.rs"
    module_saved = module_file.read_bytes()
    entries = [
        ("powershell_separator_ownership", FROZEN_REGRESSION),
        ("powershell_separator_observations", FROZEN_OBSERVER),
    ]
    created: list[Path] = []
    try:
        additions = ""
        for name, content in entries:
            path = SOURCE / f"crates/sloplint-syntax/tests/grammar_gaps/{name}.rs"
            assert not path.exists()
            path.write_bytes(content)
            created.append(path)
            additions += f'\n#[path = "grammar_gaps/{name}.rs"]\nmod {name};\n'
        module_file.write_text(module_saved.decode() + additions)
        run(label + "-format-probes", ["cargo", "fmt", "--all"])
        for path in created:
            assert len(path.read_text().splitlines()) < 600
        frozen = [{"path": str(path.relative_to(SOURCE)), "content": path.read_text(),
                   "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in created]
        for item in frozen:
            print("GUARD_FROZEN_TEST_SOURCE " + json.dumps(item), flush=True)
        assert hashlib.sha256(created[1].read_bytes()).hexdigest() == MANIFEST["formatted_observer_sha256"]
        (EVIDENCE / "formatted-probe-tests.json").write_text(json.dumps(frozen, indent=2) + "\n")
        run(label + "-clean-grammar", ["cargo", "clean", "-p", "tree-sitter-powershell"])
        args = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps"]
        run(label + "-compile-probes", [*args, "--no-run"])
        text = run(label + "-queries", [*args, "powershell_separator_observations::observe_separator_ownership_queries", "--", "--exact", "--nocapture"])
        assert "running 1 test" in text and "1 passed; 0 failed" in text
        lines = [line for line in text.splitlines() if line.startswith("OWNERSHIP_QUERY_")]
        assert sum(line.startswith("OWNERSHIP_QUERY_COMPILE ") and line.endswith(" OK") for line in lines) == 21
        assert sum(line.startswith("OWNERSHIP_QUERY_ROW ") for line in lines) == len(CASES) * 21
        ENVIRONMENT["SLOPLINT_BOUNDARY_CASES"] = str(PROOF / "double-gap-cases.tsv")
        try:
            extra_text = run(label + "-double-gap-public-queries", [*args, "powershell_separator_observations::observe_separator_ownership_queries", "--", "--exact", "--nocapture"])
            assert "running 1 test" in extra_text and "1 passed; 0 failed" in extra_text
            extra_lines = [line for line in extra_text.splitlines() if line.startswith("OWNERSHIP_QUERY_")]
            assert sum(line.startswith("OWNERSHIP_QUERY_COMPILE ") and line.endswith(" OK") for line in extra_lines) == 21
            assert sum(line.startswith("OWNERSHIP_QUERY_ROW ") for line in extra_lines) == 42
            lines.extend(line for line in extra_lines if not line.startswith("OWNERSHIP_QUERY_COMPILE "))
        finally:
            ENVIRONMENT["SLOPLINT_BOUNDARY_CASES"] = str(PROOF / "external-boundary-cases.tsv")
        focused_exit: int | None = None
        if focused:
            text = run(label + "-focused-powershell", [*args, "powershell", "--", "--nocapture"], expected=None)
            focused_exit = int(RESULTS[-1]["exit"])
            assert focused_exit in (0, 101)
        return lines, focused_exit
    finally:
        module_file.write_bytes(module_saved)
        for path in created:
            path.unlink()

def query_records(lines: list[str], identity: str, query: str) -> list[str]:
    prefix = "OWNERSHIP_QUERY_"
    return sorted(line for line in lines if line.startswith(prefix) and len(line.split()) >= 3 and line.split()[1:3] == [identity, query])

def query_hits(lines: list[str], identity: str, query: str) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    for line in lines:
        if line.startswith(f"OWNERSHIP_QUERY_HIT {identity} {query} "):
            parts = line.split()
            result.append((int(parts[3]), bytes.fromhex(parts[4] if len(parts) == 5 else "").decode()))
    return result

def healthy_query(lines: list[str], identity: str, query: str) -> bool:
    rows = [line for line in lines if line.startswith(f"OWNERSHIP_QUERY_ROW {identity} {query} ")]
    return len(rows) == 1 and "error=false partial=false timeout=false" in rows[0]

def utf16_to_byte(source: str, offset: int) -> int:
    return len(source.encode("utf-16-le")[:offset * 2].decode("utf-16-le").encode())


def unpack(name: str, receipt: dict[str, object]) -> dict[str, object]:
    packed = base64.b64decode((PROOF / name).read_text())
    assert len(packed) == receipt["gzip_bytes"]
    assert hashlib.sha256(packed).hexdigest() == receipt["gzip_sha256"]
    raw = gzip.decompress(packed)
    assert len(raw) == receipt["payload_bytes"]
    assert hashlib.sha256(raw).hexdigest() == receipt["payload_sha256"]
    return cast(dict[str, object], json.loads(raw))

external_receipt = json.loads((PROOF / "external-baseline-receipt.json").read_text())
def verify_artifact_pack(root: str, filename: str, receipt: dict[str, object], pack_file: str) -> None:
    archives = [path for path in (EVIDENCE / root).rglob("*") if path.is_file() and zipfile.is_zipfile(path)]
    assert len(archives) == 1, root
    archive = archives[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == receipt["zip_sha256"]
    with zipfile.ZipFile(archive) as handle:
        matches = [entry for entry in handle.namelist() if Path(entry).name == filename]
        assert len(matches) == 1, filename
        archived_pack = handle.read(matches[0])
    assert hashlib.sha256(archived_pack).hexdigest() == receipt["gzip_sha256"]
    assert archived_pack == base64.b64decode((PROOF / pack_file).read_text())
    print("VERIFIED_ARTIFACT_PACK " + json.dumps({"root": root, "filename": filename, "receipt": receipt}), flush=True)

verify_artifact_pack("external-baseline", "external-boundary-packed.json.gz", external_receipt, "external-baseline-packed.b64")
external = unpack("external-baseline-packed.b64", external_receipt)
assert external["sources"]["current"] == initial
assert external["manifest"]["proof_sha256"]["external-boundary-cases.json"] == MANIFEST["proof_sha256"]["external-boundary-cases.json"]
assert external["manifest"]["proof_sha256"]["external-boundary-cases.tsv"] == MANIFEST["proof_sha256"]["external-boundary-cases.tsv"]
for name in ("native-witnesses", "previous-frozen-tests-compile", "previous-query-observation",
             "current-frozen-tests-compile", "current-query-observation"):
    result = next(item for item in external["results"] if item["name"] == name)
    assert result["exit"] == result["expected"] == 0 and not result["timed_out"]
baseline_parsers = external["parsers"]["previous"]
baseline_queries = external["queries"]["previous"]
current_parsers = external["parsers"]["current"]
current_queries = external["queries"]["current"]
assert len(baseline_parsers) == len(current_parsers) == len(CASES) == 801
assert sum(line.startswith("OWNERSHIP_QUERY_COMPILE ") and line.endswith(" OK") for line in baseline_queries) == 21
assert sum(line.startswith("OWNERSHIP_QUERY_ROW ") for line in baseline_queries) == len(CASES) * 21
assert len(external["native"]) == len(CASES)
native_by_id = {record["id"]: record for record in external["native"]}
for case, native in zip(CASES, external["native"], strict=True):
    assert case["id"] == native["id"] and case["source_sha256"] == native["source_sha256"]
baseline_by_id = {record["id"]: record for record in baseline_parsers}
current_by_id = {record["id"]: record for record in current_parsers}
reference_current = {record["id"]: record for record in REFERENCE["parsers"]["current"]}
for identity, record in reference_current.items():
    actual = baseline_by_id[identity]
    for key in ("exit", "accepted", "commands", "generic_words", "target_nodes", "separator_nodes", "comments"):
        assert actual[key] == record[key], (identity, key, actual[key], record[key])
    if record["accepted"]:
        assert actual["tree_sha256"] == record["tree_sha256"], identity
for query in ("direct-comment", "separator-comment", "element-separator", "all-comments", "parameter",
              "all-words", "redirect-word", "redirect-separator", "redirect-comment"):
    for case in REFERENCE["native"]:
        identity = case["id"]
        assert query_records(baseline_queries, identity, query) == query_records(REFERENCE["queries"]["current"], identity, query)

boundary_receipt = json.loads((PROOF / "guard-boundary-success-receipt.json").read_text())
verify_artifact_pack("guard-baseline", "guard-boundary-packed.json.gz", boundary_receipt, "guard-boundary-packed.b64")
boundary = unpack("guard-boundary-packed.b64", boundary_receipt)
assert boundary["sources"]["current"] == external["sources"]["previous"]
boundary_by_id = {case["id"]: case for case in json.loads((PROOF / "guard-boundary-cases.json").read_text())}
for identity, case in boundary_by_id.items():
    assert case["source_sha256"] == case_by_id[identity]["source_sha256"]
    for query in ("direct-comment", "separator-comment", "element-separator", "all-comments", "parameter",
                  "all-words", "redirect-word", "redirect-separator", "redirect-comment", "array-literal",
                  "array-word", "argument-list"):
        assert query_records(boundary["queries"]["current"], identity, query) == query_records(baseline_queries, identity, query)
prefix_receipt = json.loads((PROOF / "prefix-baseline-receipt.json").read_text())
verify_artifact_pack("prefix-baseline", "prefix-boundary-packed.json.gz", prefix_receipt, "prefix-baseline-packed.b64")
prefix_reference = unpack("prefix-baseline-packed.b64", prefix_receipt)
assert prefix_reference["sources"]["current"] == initial
prefix_cases = json.loads((PROOF / "prefix-boundary-cases.json").read_text())
assert len(prefix_cases) == 2
assert [case["source_sha256"] for case in prefix_cases] == [record["source_sha256"] for record in prefix_reference["native"]]
prefix_root = EVIDENCE / "prefix-controls"
prefix_root.mkdir()
for index, case in enumerate(prefix_cases):
    assert hashlib.sha256(case["source"].encode()).hexdigest() == case["source_sha256"]
    (prefix_root / f"prefix-{index}.ps1").write_text(case["source"])

print("VERIFIED_CURRENT_AND_HISTORICAL_BASELINES " + json.dumps({
    "external_receipt": external_receipt, "boundary_receipt": boundary_receipt,
    "cases": len(CASES), "public_queries": 21, "original_cases": len(reference_current),
}), flush=True)
double_gap_cases = json.loads((PROOF / "double-gap-cases.json").read_text())
assert len(double_gap_cases) == 2
gap_root = EVIDENCE / "double-gap-controls"
gap_root.mkdir()
for index, case in enumerate(double_gap_cases):
    assert hashlib.sha256(case["source"].encode()).hexdigest() == case["source_sha256"]
    (gap_root / f"case-{index}.ps1").write_text(case["source"])

archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
                   "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "double-gap-powershell.tar.gz")
powershell = EVIDENCE / "double-gap-powershell"
powershell.mkdir()
run("double-gap-extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
pwsh = powershell / "pwsh"
pwsh.chmod(0o755)
native_text = run("double-gap-native", [str(pwsh), "-NoLogo", "-NoProfile", "-File",
                  str(PROOF / "double-gap-oracle.ps1"), "-CasesPath", str(PROOF / "double-gap-cases.json")])
double_gap_native = [json.loads(line.removeprefix("NATIVE_CONTEXT ")) for line in native_text.splitlines() if line.startswith("NATIVE_CONTEXT ")]
assert len(double_gap_native) == 2 and all(item["accepted"] and item["matches"] for item in double_gap_native)

historical_head = "57adc9fff126a9826e0fb0be921c28e6cdb08391"
run("double-gap-fetch-historical-source", ["git", "fetch", "--depth=1", "origin", historical_head])
assert output(["git", "rev-parse", historical_head + "^{tree}"]) == "f518b3943c6b86af53c334175309303c3d79f38b"
historical_root = EVIDENCE / "double-gap-historical"
historical_root.mkdir()
archived = subprocess.check_output(["git", "archive", historical_head, "vendor/tree-sitter-powershell"], cwd=SOURCE)
with tarfile.open(fileobj=io.BytesIO(archived)) as handle:
    handle.extractall(historical_root, filter="data")
historical_vendor = historical_root / "vendor/tree-sitter-powershell"
entries = subprocess.check_output(["git", "ls-tree", "-r", "-z", historical_head, "--", "vendor/tree-sitter-powershell"], cwd=SOURCE)
vendor_count = 0
for entry in entries.split(b"\0"):
    if not entry:
        continue
    metadata, relative = entry.split(b"\t", 1)
    mode, kind, expected = metadata.split()
    assert kind == b"blob"
    path = historical_root / os.fsdecode(relative)
    if mode == b"120000":
        content = os.fsencode(os.readlink(path))
    else:
        assert mode in (b"100644", b"100755")
        content = path.read_bytes()
        assert bool(path.stat().st_mode & 0o111) == (mode == b"100755")
    assert hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest() == expected.decode(), str(path)
    vendor_count += 1
print("VERIFIED_HISTORICAL_VENDOR " + json.dumps({"head": historical_head, "tree": "f518b3943c6b86af53c334175309303c3d79f38b",
      "tracked_vendor_blobs": vendor_count, "vendor_entries_sha256": hashlib.sha256(entries).hexdigest()}), flush=True)

def located_spans(kind: str, source: str, text: str) -> list[tuple[int, str]]:
    encoded = source.encode()
    lines = encoded.splitlines(keepends=True)
    result: list[tuple[int, str]] = []
    for match in re.finditer(rf"\({kind} \[(\d+), (\d+)\] - \[(\d+), (\d+)\]", text):
        start_row, start_column, end_row, end_column = map(int, match.groups())
        begin = sum(map(len, lines[:start_row])) + start_column
        finish = sum(map(len, lines[:end_row])) + end_column
        result.append((begin, encoded[begin:finish].decode()))
    return result

double_gap_baseline = {}
for label, vendor in (("previous", historical_vendor), ("current", VENDOR)):
    library = EVIDENCE / ("double-gap-libraries-" + label)
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    records = []
    for index, case in enumerate(double_gap_cases):
        text = run("double-gap-" + label + "-" + case["id"], [str(binary), "parse", str(gap_root / f"case-{index}.ps1")], expected=None, cwd=vendor)
        actual_exit = int(RESULTS[-1]["exit"])
        assert actual_exit in (0, 1), (label, case["id"], actual_exit)
        record = {"id": case["id"], "source_sha256": case["source_sha256"], "exit": actual_exit,
                  "redirection": located_spans("redirection", case["source"], text),
                  "redirect-target": located_spans("redirected_file_name", case["source"], text),
                  "commands": spans("command", case["source"], text)}
        records.append(record)
        print("FROZEN_DOUBLE_GAP " + label + " " + json.dumps(record), flush=True)
    double_gap_baseline[label] = records
assert unchanged_identity(SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"])) == initial

for stage in MANIFEST["stages"]:
    label = stage["id"]
    try:
        for path, content in ORIGINAL.items():
            (SOURCE / path).write_bytes(content)
        authored = (PROOF / stage["file"]).read_bytes()
        assert hashlib.sha256(authored).hexdigest() == stage["sha256"]
        (VENDOR / "grammar.js").write_bytes(authored)
        scanner = (PROOF / stage["scanner_file"]).read_bytes()
        assert hashlib.sha256(scanner).hexdigest() == stage["scanner_sha256"]
        (VENDOR / "src/scanner.c").write_bytes(scanner)
        generation = run(label + "-generate", [str(binary), "generate"], expected=None, cwd=VENDOR)
        if RESULTS[-1]["exit"] != 0:
            outcomes.append({"id": label, "generation_exit": RESULTS[-1]["exit"], "eligible": False})
            continue
        snapshot = source_snapshot()
        parsers = parse_cases(label)
        queries, focused_exit = query_observations(label, True)
        assert source_snapshot() == snapshot
        by_id = {record["id"]: record for record in parsers}
        failures: list[dict[str, object]] = []
        def fail(kind: str, identity: str, expected: object, actual: object) -> None:
            failures.append({"kind": kind, "id": identity, "expected": expected, "actual": actual})
        for identity in CONTROLS["native_acceptance_ids"]:
            if not by_id[identity]["accepted"]:
                fail("native-acceptance", identity, True, by_id[identity])
        for identity in CONTROLS["native_command_ids"]:
            expected = [command["text"] for command in native_by_id[identity]["commands"]]
            if by_id[identity]["commands"] != expected:
                fail("native-command-extents", identity, expected, by_id[identity]["commands"])
        for identity in CONTROLS["exact_target_ids"]:
            native = native_by_id[identity]
            assert len(native["redirections"]) == 1
            target = native["redirections"][0]
            source = str(case_by_id[identity]["source"])
            begin = utf16_to_byte(source, int(target["location_start"]))
            end = utf16_to_byte(source, int(target["location_end"]))
            expected = [(begin, target["file"])]
            assert end == begin + len(str(target["file"]).encode())
            actual = query_hits(queries, identity, "redirect-word")
            if actual != expected or not healthy_query(queries, identity, "redirect-word"):
                fail("native-target-bytes", identity, expected, actual)
        for identity in CONTROLS["restored_comment_query_ids"]:
            for query in CONTROLS["restored_comment_queries"]:
                expected = query_records(REFERENCE["queries"]["previous"], identity, query)
                actual = query_records(queries, identity, query)
                if actual != expected:
                    fail("old-comment-query:" + query, identity, expected, actual)
        for identity in CONTROLS["unchanged_comment_tree_ids"]:
            if by_id[identity]["tree_sha256"] != reference_current[identity]["tree_sha256"]:
                fail("unchanged-comment-tree", identity, reference_current[identity], by_id[identity])
        for identity in CONTROLS["current_compatibility_ids"]:
            expected = sorted(line for line in baseline_queries if line.split()[1] == identity)
            actual = sorted(line for line in queries if line.split()[1] == identity)
            if actual != expected:
                fail("current-public-query", identity, expected, actual)
            if by_id[identity]["tree_sha256"] != baseline_by_id[identity]["tree_sha256"]:
                fail("current-parser-tree", identity, baseline_by_id[identity], by_id[identity])
        for identity, case in boundary_by_id.items():
            previous = baseline_by_id[identity]
            actual = by_id[identity]
            positive = case["scope"] in ("positive", "word-list") or identity in (
                "comment-guard-prefix-ddd", "comment-guard-prefix-dd9", "comment-guard-prefix-ddd9",
            )
            if positive:
                if not actual["accepted"]:
                    fail("guard-positive-acceptance", identity, True, actual)
                expected_commands = [command["text"] for command in native_by_id[identity]["commands"]]
                if actual["commands"] != expected_commands:
                    fail("guard-positive-command", identity, expected_commands, actual["commands"])
                for query in ("direct-comment", "separator-comment", "element-separator"):
                    expected = query_records(boundary["queries"]["previous"], identity, query)
                    observed = query_records(queries, identity, query)
                    if observed != expected:
                        fail("guard-old-comment:" + query, identity, expected, observed)
                if case["scope"] == "word-list":
                    for query in ("all-words", "array-literal", "array-word", "argument-list"):
                        expected = query_records(baseline_queries, identity, query)
                        observed = query_records(queries, identity, query)
                        if observed != expected:
                            fail("guard-word-list-shape:" + query, identity, expected, observed)
            else:
                for key in ("exit", "accepted", "commands", "generic_words", "target_nodes", "separator_nodes", "comments", "arrays"):
                    if actual[key] != previous[key]:
                        fail("guard-negative-boundary:" + key, identity, previous[key], actual[key])
                expected = sorted(line for line in baseline_queries if line.split()[1] == identity)
                observed = sorted(line for line in queries if line.split()[1] == identity)
                if observed != expected:
                    fail("guard-negative-public-queries", identity, expected, observed)
                if previous["accepted"] and actual["tree_sha256"] != previous["tree_sha256"]:
                    fail("guard-negative-tree", identity, previous["tree_sha256"], actual["tree_sha256"])
        for case in CASES:
            identity = str(case["id"])
            if not identity.startswith("external-comment-"):
                continue
            previous = baseline_by_id[identity]
            actual = by_id[identity]
            native = native_by_id[identity]
            if actual["accepted"] != previous["accepted"]:
                fail("external-comment-acceptance", identity, previous["accepted"], actual["accepted"])
            if actual["commands"] != previous["commands"]:
                fail("external-comment-command", identity, previous["commands"], actual["commands"])
            expected_native_redirects = [redirect["text"] for redirect in native["redirections"]]
            if native["accepted"] and previous["redirections"] == expected_native_redirects:
                if actual["redirections"] != expected_native_redirects:
                    fail("external-native-redirection", identity, expected_native_redirects, actual["redirections"])
            # Restore prior target ownership, including its bounded historical limitations.
            if previous["redirections"]:
                for query in ("redirection", "redirect-target", "redirect-word", "redirect-separator", "redirect-comment"):
                    expected = query_records(baseline_queries, identity, query)
                    observed = query_records(queries, identity, query)
                    if observed != expected:
                        fail("external-redirect-query:" + query, identity, expected, observed)
            guarded = case["scope"] in ("first", "later") and any(
                identity.endswith("-" + suffix) for suffix in ("named", "marked", "array")
            )
            if not guarded:
                expected = sorted(line for line in baseline_queries if line.split()[1] == identity)
                observed = sorted(line for line in queries if line.split()[1] == identity)
                if observed != expected:
                    fail("external-unchanged-public-queries", identity, expected, observed)
                if previous["accepted"] and actual["tree_sha256"] != previous["tree_sha256"]:
                    fail("external-unchanged-tree", identity, previous["tree_sha256"], actual["tree_sha256"])
        prefix_records = []
        for index, case in enumerate(prefix_cases):
            text = run(label + "-prefix-" + case["id"], [str(binary), "parse", str(prefix_root / f"prefix-{index}.ps1")], cwd=VENDOR)
            record = {"id": case["id"], "source_sha256": case["source_sha256"],
                      "commands": spans("command", case["source"], text),
                      "parameters": spans("command_parameter", case["source"], text),
                      "generic_words": spans("generic_token", case["source"], text),
                      "separator_nodes": spans("command_argument_sep", case["source"], text),
                      "comments": spans("comment", case["source"], text)}
            before = prefix_reference["parsers"]["previous"][index]
            original = prefix_reference["parsers"]["original"][index]
            assert record["id"] == before["id"] == original["id"]
            for key in ("commands", "generic_words", "comments"):
                if record[key] != before[key]:
                    fail("complete-prefix-boundary:" + key, str(case["id"]), before[key], record[key])
            if record["separator_nodes"] != original["separator_nodes"]:
                fail("complete-prefix-separator", str(case["id"]), original["separator_nodes"], record["separator_nodes"])
            record["native"] = prefix_reference["native"][index]
            record["limitation"] = "Attached CRLF splits native commands but all three historical parsers join them; only historical comment ownership is asserted here." if case["id"].endswith("-crlf") else None
            prefix_records.append(record)
        print("COMPLETE_PREFIX_CONTROLS " + json.dumps(prefix_records), flush=True)
        for previous in double_gap_baseline["previous"]:
            identity = str(previous["id"])
            if previous["exit"] != 0:
                fail("double-gap-historical-coverage", identity, 0, previous["exit"])
            for query in ("redirection", "redirect-target"):
                actual = query_hits(queries, identity, query)
                expected = previous[query]
                if actual != expected or not healthy_query(queries, identity, query):
                    fail("double-gap-target:" + query, identity, expected, actual)
        print("DOUBLE_GAP_QUERY_RECORDS " + json.dumps([line for line in queries if line.split()[1].startswith("redirect-double-gap-")]), flush=True)
        argument_log = (EVIDENCE / (label + "-focused-powershell.log")).read_text()
        argument_rows = [line.split() for line in argument_log.splitlines() if line.startswith("GUARD_NATIVE_ARG_CASE ")]
        expected_arguments = {identity: native for identity, native in native_by_id.items()
                              if identity in boundary_by_id and boundary_by_id[identity]["scope"] == "word-list"}
        if len(argument_rows) != len(expected_arguments):
            fail("native-argument-case-count", label, len(expected_arguments), len(argument_rows))
        for fields in argument_rows:
            identity, count = fields[1], int(fields[2])
            native = expected_arguments[identity]
            if count != native["native_count"]:
                fail("native-argument-count", identity, native["native_count"], count)
        node_types_unchanged = hashlib.sha256((VENDOR / "src/node-types.json").read_bytes()).hexdigest() == MANIFEST["historical_node_types_sha256"]
        trace_ids = (
            "comment-single-parameter-space", "comment-single-plain-space",
            "comment-guard-lf-multiline-inside", "comment-guard-zwsp-1-array",
            "comment-guard-prefix-ddd", "comment-guard-lf-line-named",
            "external-comment-redirect-number", "external-comment-redirect-space-marked",
            "external-comment-adjacent-word", "comment-redirect-single-dash",
            "comment-redirect-repeated-dash", "comment-redirect-multiline-dash",
            "external-comment-redirect-space-named",
        )
        trace_receipts = []
        scanner_path = VENDOR / "src/scanner.c"
        scanner_original = scanner_path.read_bytes()
        scanner_text = scanner_original.decode()
        trace_source = scanner_text.replace('#include <wctype.h>', '#include <wctype.h>\n#include <stdio.h>')
        trace_source = trace_source.replace('    bool commented_redirect_valid = comment_at_entry && valid_symbols[COMMENTED_REDIRECT];',
            '    unsigned callback_column = lexer->get_column(lexer);\n    bool commented_redirect_valid = comment_at_entry && valid_symbols[COMMENTED_REDIRECT];')
        trace_source = trace_source.replace('            if (commented_redirect_valid) return scan_commented_redirect(lexer);',
            '            if (commented_redirect_valid) {\n'
            '                unsigned target_column = lexer->get_column(lexer);\n'
            '                bool accepted = scan_commented_redirect(lexer);\n'
            '                fprintf(stderr, "COMMENTED_REDIRECT_PROBE accepted=%d callback_col=%u target_col=%u\\n", accepted, callback_column, target_column);\n'
            '                return accepted;\n'
            '            }')
        assert trace_source.count("COMMENTED_REDIRECT_PROBE") == 1
        library_before = ENVIRONMENT["TREE_SITTER_LIBDIR"]
        trace_library = EVIDENCE / (label + "-trace-libraries")
        trace_library.mkdir()
        try:
            scanner_path.write_text(trace_source)
            ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(trace_library)
            for identity in trace_ids:
                case_index = next(index for index, case in enumerate(CASES) if case["id"] == identity)
                text = run(label + "-trace-" + identity, [str(binary), "parse", "--debug=normal", str(case_root / f"case-{case_index}.ps1")],
                           expected=int(by_id[identity]["exit"]), cwd=VENDOR)
                canonical_tree = "\n".join(line for line in text.splitlines() if re.match(r"^\s*(?:[A-Za-z_]+: )?\(", line)) + "\n"
                assert hashlib.sha256(canonical_tree.encode()).hexdigest() == by_id[identity]["canonical_tree_sha256"], identity
                guards = [(kind, int(size)) for kind, size in re.findall(r"lexed_lookahead sym:(_commented_(?:argument|redirect)), size:(\d+)", text)]
                assert all(size == 0 for _, size in guards), (identity, guards)
                probes = [{"accepted": bool(int(value)), "callback_column": int(start), "target_column": int(target)}
                          for value, start, target in re.findall(r"COMMENTED_REDIRECT_PROBE accepted=(\d) callback_col=(\d+) target_col=(\d+)", text)]
                if identity == "external-comment-redirect-space-named":
                    assert any(not probe["accepted"] for probe in probes), (identity, probes)
                if identity == "comment-redirect-single-dash":
                    assert ("_commented_redirect", 0) in guards and any(probe["accepted"] for probe in probes), (identity, guards, probes)
                trace_receipts.append({"id": identity, "guard_tokens": guards, "redirect_probes": probes,
                                       "canonical_tree_sha256": by_id[identity]["canonical_tree_sha256"]})
        finally:
            scanner_path.write_bytes(scanner_original)
            ENVIRONMENT["TREE_SITTER_LIBDIR"] = library_before
            assert source_snapshot() == snapshot
        print("GUARD_ZERO_WIDTH_TRACE " + json.dumps({"instrumented_scanner_sha256": hashlib.sha256(trace_source.encode()).hexdigest(),
              "restored_scanner_sha256": hashlib.sha256(scanner_original).hexdigest(), "traces": trace_receipts}), flush=True)
        corpus = run(label + "-corpus", [str(binary), "test"], expected=None, cwd=VENDOR)
        corpus_exit = int(RESULTS[-1]["exit"])
        assert source_snapshot() == snapshot
        record = {"id": label, "source": snapshot, "node_types_unchanged": node_types_unchanged,
                  "focused_exit": focused_exit, "corpus_exit": corpus_exit, "failures": failures,
                  "eligible": focused_exit == 0 and corpus_exit == 0 and node_types_unchanged and not failures,
                  "parsers": parsers, "queries": queries, "prefix_controls": prefix_records}
        outcomes.append(record)
        (EVIDENCE / (label + "-outcome.json")).write_text(json.dumps(record, indent=2) + "\n")
        print("EXPERIMENT_STAGE_OUTCOME " + json.dumps({key: value for key, value in record.items() if key not in ("parsers", "queries")}), flush=True)
    finally:
        for path, content in ORIGINAL.items():
            (SOURCE / path).write_bytes(content)
        assert unchanged_identity(SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"])) == initial

payload = json.dumps({"manifest": MANIFEST, "baseline_source": initial, "baseline_parsers": baseline_parsers,
                      "baseline_queries": baseline_queries, "external_receipt": external_receipt,
                      "boundary_receipt": boundary_receipt, "double_gap_native": double_gap_native,
                      "double_gap_baseline": double_gap_baseline, "outcomes": outcomes, "results": RESULTS},
                     separators=(",", ":"), ensure_ascii=True).encode()
packed = gzip.compress(payload, mtime=0)
encoded = base64.b64encode(packed).decode()
(EVIDENCE / "experiment-complete.json").write_bytes(payload)
(EVIDENCE / "experiment-packed.json.gz").write_bytes(packed)
transport = {"payload_sha256": hashlib.sha256(payload).hexdigest(), "gzip_sha256": hashlib.sha256(packed).hexdigest(),
             "payload_bytes": len(payload), "gzip_bytes": len(packed), "base64_bytes": len(encoded)}
(EVIDENCE / "experiment-packed-identity.json").write_text(json.dumps(transport, indent=2) + "\n")
print("OWNERSHIP_EXPERIMENT_PACKED_IDENTITY " + json.dumps(transport), flush=True)
for index in range(0, len(encoded), 24000):
    print("OWNERSHIP_EXPERIMENT_CHUNK " + json.dumps({"part": index // 24000, "data": encoded[index:index + 24000]}), flush=True)
print("OWNERSHIP_EXPERIMENT_COMPLETE " + json.dumps({"stages": [{key: value for key, value in item.items() if key not in ("parsers", "queries", "failures")} for item in outcomes], "failure_counts": [len(item.get("failures", [])) for item in outcomes], "restored": initial}), flush=True)
if not outcomes[-1]["eligible"]:
    raise SystemExit(1)
