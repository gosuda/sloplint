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
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "proof/ci-tools/powershell-separator-ownership-r23"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-separator-ownership-r23"
EVIDENCE.mkdir(exist_ok=True)
MANIFEST = cast(dict[str, object], json.loads((PROOF / "experiment-manifest.json").read_text()))
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
         "vendor/tree-sitter-powershell/src/parser.c", "vendor/tree-sitter-powershell/src/node-types.json"]
ORIGINAL = {path: (SOURCE / path).read_bytes() for path in PATHS}
FROZEN_REGRESSION = (PROOF / "regression.rs").read_bytes()
FROZEN_OBSERVER = (PROOF / "experiment-observer.rs").read_bytes()
CASES = cast(list[dict[str, object]], json.loads((PROOF / "experiment-cases.json").read_text()))
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
assert REFERENCE["sources"]["current"]["head"] == MANIFEST["base"]
assert REFERENCE["sources"]["current"]["tree"] == MANIFEST["base_tree"]
print("VERIFIED_FROZEN_BASELINE " + json.dumps(CONTROLS["baseline"], sort_keys=True), flush=True)
initial = unchanged_identity(SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"]))
run("rust-toolchain", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
assert output(["rustc", "--version"]).startswith("rustc 1.100.0 ")
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
binary = EVIDENCE / "tree-sitter"
binary.write_bytes(gzip.decompress(archive.read_bytes()))
binary.chmod(0o755)
ENVIRONMENT["SLOPLINT_BOUNDARY_CASES"] = str(PROOF / "experiment-cases.tsv")
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
        assert hashlib.sha256(created[0].read_bytes()).hexdigest() == MANIFEST["formatted_regression_sha256"]
        run(label + "-clean-grammar", ["cargo", "clean", "-p", "tree-sitter-powershell"])
        args = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps"]
        run(label + "-compile-probes", [*args, "--no-run"])
        text = run(label + "-queries", [*args, "powershell_separator_observations::observe_separator_ownership_queries", "--", "--exact", "--nocapture"])
        assert "running 1 test" in text and "1 passed; 0 failed" in text
        lines = [line for line in text.splitlines() if line.startswith("OWNERSHIP_QUERY_")]
        assert sum(line.startswith("OWNERSHIP_QUERY_COMPILE ") and line.endswith(" OK") for line in lines) == 16
        assert sum(line.startswith("OWNERSHIP_QUERY_ROW ") for line in lines) == len(CASES) * 16
        focused_exit: int | None = None
        if focused:
            text = run(label + "-focused-powershell", [*args, "powershell"], expected=None)
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

baseline_parsers = parse_cases("unchanged-current")
baseline_queries, _ = query_observations("unchanged-current", False)
baseline_by_id = {record["id"]: record for record in baseline_parsers}
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
assert unchanged_identity(SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"])) == initial
print("FROZEN_CURRENT_COMPATIBILITY_MATCH " + json.dumps({"cases": len(CASES), "queries": 16, "reference_cases": 483}), flush=True)

for stage in MANIFEST["stages"]:
    label = stage["id"]
    try:
        for path, content in ORIGINAL.items():
            (SOURCE / path).write_bytes(content)
        authored = (PROOF / stage["file"]).read_bytes()
        assert hashlib.sha256(authored).hexdigest() == stage["sha256"]
        (VENDOR / "grammar.js").write_bytes(authored)
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
            if by_id[identity]["canonical_tree_sha256"] != baseline_by_id[identity]["canonical_tree_sha256"]:
                fail("current-parser-tree", identity, baseline_by_id[identity], by_id[identity])
        node_types_unchanged = (VENDOR / "src/node-types.json").read_bytes() == ORIGINAL["vendor/tree-sitter-powershell/src/node-types.json"]
        corpus = run(label + "-corpus", [str(binary), "test"], expected=None, cwd=VENDOR)
        corpus_exit = int(RESULTS[-1]["exit"])
        assert source_snapshot() == snapshot
        record = {"id": label, "source": snapshot, "node_types_unchanged": node_types_unchanged,
                  "focused_exit": focused_exit, "corpus_exit": corpus_exit, "failures": failures,
                  "eligible": focused_exit == 0 and corpus_exit == 0 and node_types_unchanged and not failures,
                  "parsers": parsers, "queries": queries}
        outcomes.append(record)
        (EVIDENCE / (label + "-outcome.json")).write_text(json.dumps(record, indent=2) + "\n")
        print("EXPERIMENT_STAGE_OUTCOME " + json.dumps({key: value for key, value in record.items() if key not in ("parsers", "queries")}), flush=True)
    finally:
        for path, content in ORIGINAL.items():
            (SOURCE / path).write_bytes(content)
        assert unchanged_identity(SOURCE, str(MANIFEST["base"]), str(MANIFEST["base_tree"])) == initial

payload = json.dumps({"manifest": MANIFEST, "baseline_source": initial, "baseline_parsers": baseline_parsers,
                      "baseline_queries": baseline_queries, "outcomes": outcomes, "results": RESULTS},
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
