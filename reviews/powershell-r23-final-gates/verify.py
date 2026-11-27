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
PROOF = WORKSPACE / "proof/ci-tools/powershell-separator-ownership-r23"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-separator-ownership-r23"
EVIDENCE.mkdir(exist_ok=True)
VENDOR = SOURCE / "vendor/tree-sitter-powershell"
MANIFEST = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
ENVIRONMENT = dict(os.environ)
ENVIRONMENT.update(RUSTUP_TOOLCHAIN="1.100.0", CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="8", CARGO_TERM_COLOR="never",
                   CARGO_PROFILE_DEV_DEBUG="line-tables-only", CARGO_PROFILE_TEST_DEBUG="line-tables-only",
                   SLOPLINT_REQUIRE_FS_CAPS="1")
INDEX_ENVIRONMENT = dict(ENVIRONMENT)
INDEX_ENVIRONMENT["GIT_INDEX_FILE"] = str(EVIDENCE / "candidate.index")
RESULTS: list[dict[str, object]] = []

def output(args: list[str], cwd: Path = SOURCE, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=cwd, env=ENVIRONMENT if env is None else env, text=True).strip()

def run(name: str, args: list[str], expected: int = 0, cwd: Path = SOURCE, limit: int = 2400) -> str:
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
    RESULTS.append({"name": name, "args": args, "exit": process.returncode, "expected": expected,
                    "seconds": time.monotonic() - started, "timed_out": timed_out,
                    "log_sha256": hashlib.sha256(text.encode()).hexdigest()})
    (EVIDENCE / "results.json").write_text(json.dumps({"manifest": MANIFEST, "results": RESULTS}, indent=2) + "\n")
    preview = text[-25000:]
    if process.returncode != expected and len(text) > 25000:
        preview = text[:12000] + "\nOUTPUT_MIDDLE_OMITTED; complete log preserved in artifact\n" + preview
    print(f"PROBE_EXIT {name} {process.returncode} timed_out={timed_out}\n{preview}", flush=True)
    assert not timed_out and process.returncode == expected, (name, process.returncode, expected, timed_out)
    return text

def download(url: str, digest: str, name: str) -> Path:
    path = EVIDENCE / name
    with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    assert actual == digest, (name, actual, digest)
    print(f"VERIFIED_DOWNLOAD {name} {actual}", flush=True)
    return path

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

def install_rust() -> None:
    run("rust-toolchain", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
    assert output(["rustc", "--version"]).startswith("rustc 1.100.0 ")

def tree_sitter() -> Path:
    archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                       "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
    binary = EVIDENCE / "tree-sitter"
    binary.write_bytes(gzip.decompress(archive.read_bytes()))
    binary.chmod(0o755)
    return binary

def apply_elements(elements: list[dict[str, str]]) -> None:
    for item in elements:
        content = item["content"].encode()
        assert hashlib.sha256(content).hexdigest() == item["sha256"], item["path"]
        path = SOURCE / item["path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)

def native_oracles() -> list[dict[str, object]]:
    archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
                       "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "powershell.tar.gz")
    powershell = EVIDENCE / "powershell"
    powershell.mkdir(exist_ok=True)
    run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
    pwsh = powershell / "pwsh"
    pwsh.chmod(0o755)
    native_bin = EVIDENCE / "native-bin"
    native_bin.mkdir(exist_ok=True)
    capture = native_bin / "Capture-Native"
    capture.write_text(f"#!{sys.executable}\nimport json\nimport sys\nprint(json.dumps({{'count': len(sys.argv) - 1, 'values': sys.argv[1:]}}, ensure_ascii=True))\n")
    capture.chmod(0o755)
    ENVIRONMENT["PATH"] = str(native_bin) + os.pathsep + ENVIRONMENT["PATH"]
    run("preserved-native-grammar", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "preserved-grammar-oracle.ps1"), "-CasesPath", str(PROOF / "preserved-grammar-cases.json")])
    run("preserved-native-literals", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "preserved-literal-oracle.ps1"), "-CasesPath", str(PROOF / "preserved-literal-cases.json")])
    text = run("native-new-cases", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "native-oracle.ps1"), "-CasesPath", str(PROOF / "cases.json")])
    records = [json.loads(line.removeprefix("NATIVE_CASE ")) for line in text.splitlines() if line.startswith("NATIVE_CASE ")]
    assert len(records) == 243 and all(item["matches"] for item in records)
    (EVIDENCE / "native-cases.json").write_text(json.dumps(records, indent=2, sort_keys=True) + "\n")
    text = run("native-token-boundaries", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "token-oracle.ps1"), "-CasesPath", str(PROOF / "token-cases.json")])
    tokens = [json.loads(line.removeprefix("TOKEN_CASE ")) for line in text.splitlines() if line.startswith("TOKEN_CASE ")]
    expected = cast(list[dict[str, object]], json.loads((PROOF / "token-expected.json").read_text()))
    assert len(tokens) == 40 and tokens == expected, (len(tokens), len(expected))
    (EVIDENCE / "native-token-boundaries.json").write_text(json.dumps(tokens, indent=2, sort_keys=True) + "\n")
    text = run("native-followup-cases", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "followup-oracle.ps1"), "-CasesPath", str(PROOF / "followup-cases.json")])
    followup = [json.loads(line.removeprefix("NATIVE_FOLLOWUP ")) for line in text.splitlines() if line.startswith("NATIVE_FOLLOWUP ")]
    expected_followup = json.loads((PROOF / "followup-native-expected.json").read_text())
    assert len(followup) == 176 and followup == expected_followup
    assert all(item["matches"] for item in followup)
    (EVIDENCE / "followup-native-cases.json").write_text(json.dumps(followup, indent=2, sort_keys=True) + "\n")
    print("NATIVE_FOLLOWUP_IDENTITY " + hashlib.sha256(json.dumps(followup, sort_keys=True).encode()).hexdigest(), flush=True)
    text = run("native-near-controls", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "followup-oracle.ps1"), "-CasesPath", str(PROOF / "near-controls.json")])
    near = [json.loads(line.removeprefix("NATIVE_FOLLOWUP ")) for line in text.splitlines() if line.startswith("NATIVE_FOLLOWUP ")]
    assert len(near) == 6 and all(item["matches"] for item in near)
    (EVIDENCE / "native-near-controls.json").write_text(json.dumps(near, indent=2, sort_keys=True) + "\n")
    text = run("native-context-cases", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "context-oracle.ps1"), "-CasesPath", str(PROOF / "context-cases.json")])
    contexts = [json.loads(line.removeprefix("NATIVE_CONTEXT ")) for line in text.splitlines() if line.startswith("NATIVE_CONTEXT ")]
    expected_contexts = json.loads((PROOF / "context-native-expected.json").read_text())
    assert len(contexts) == MANIFEST["context_native_cases"]
    assert len(expected_contexts) == 442 and contexts[:442] == expected_contexts
    additional = json.loads((PROOF / "context-cases.json").read_text())[442:]
    for case, record in zip(additional, contexts[442:], strict=True):
        if "expected_branches" in case:
            assert [{"clauses": branch["clauses"], "has_else": branch["has_else"]} for branch in record["branches"]] == case["expected_branches"]
            assert record["commands"] == case["expected_commands"]
        print("ADDITIONAL_NATIVE_CONTEXT " + json.dumps(record), flush=True)
    assert all(item["matches"] for item in contexts)
    (EVIDENCE / "native-context-cases.json").write_text(json.dumps(contexts, indent=2, sort_keys=True) + "\n")
    print("FROZEN_NATIVE_CONTEXTS_MATCH " + str(len(contexts)), flush=True)
    return records

def tree_text(text: str) -> str:
    start = text.index("(program")
    return text[start:text.rindex(")") + 1]


def baseline_queries() -> dict[str, list[str]]:
    receipt = json.loads((PROOF / "context-baseline-receipt.json").read_text())
    root = EVIDENCE / "baseline"
    archives = [path for path in root.rglob("*") if path.is_file() and zipfile.is_zipfile(path)]
    assert len(archives) == 1, [str(path) for path in archives]
    archive = archives[0]
    archive_digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    assert archive_digest == "0a76bd3c9df88fd1d3cb3243a58736082a53538421d8d78983cc84567b0e5569"
    print("VERIFIED_BASELINE_ARTIFACT_ZIP " + archive_digest, flush=True)
    results = {}
    for label in ("legacy", "previous", "current"):
        name = label + "-query-observation"
        with zipfile.ZipFile(archive) as handle:
            matches = [member for member in handle.namelist() if Path(member).name == name + ".log"]
            assert len(matches) == 1, (name, matches)
            content = handle.read(matches[0])
        expected = next(item["log_sha256"] for item in receipt["results"] if item["name"] == name)
        assert hashlib.sha256(content).hexdigest() == expected, name
        lines = [line for line in content.decode().splitlines() if line.startswith("PUBLIC_QUERY_")]
        results[label] = lines
        (EVIDENCE / (name + "-verified.log")).write_bytes(content)
        print("VERIFIED_BASELINE_QUERY_LOG " + json.dumps({"name": name, "sha256": expected, "lines": len(lines)}), flush=True)
        for line in lines:
            print("ORIGINAL_QUERY " + label + " " + line, flush=True)
    assert len([line for line in results["legacy"] if line.startswith("PUBLIC_QUERY_COMPILE ") and line.endswith(" OK")]) == 5
    assert len([line for line in results["legacy"] if line.startswith("PUBLIC_QUERY_ROW ")]) == 370
    return results

def context_spans(kind: str, content: str, tree: str) -> list[str]:
    encoded = content.encode()
    lines = encoded.splitlines(keepends=True)
    spans = []
    for match in re.finditer(rf"\({kind} \[(\d+), (\d+)\] - \[(\d+), (\d+)\]", tree):
        start_row, start_column, end_row, end_column = map(int, match.groups())
        start = sum(map(len, lines[:start_row])) + start_column
        end = sum(map(len, lines[:end_row])) + end_column
        spans.append(encoded[start:end].decode())
    return spans

def context_parsers(binary: Path, stage: str, branch_only: bool = False) -> None:
    cases = json.loads((PROOF / "context-cases.json").read_text())
    native = {item["id"]: item for item in json.loads((EVIDENCE / "native-context-cases.json").read_text())}
    historical = json.loads((PROOF / "context-original-observations.json").read_text())
    case_root = EVIDENCE / ("context-cases-" + stage)
    case_root.mkdir()
    library = EVIDENCE / ("context-parser-" + stage)
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    results = []
    observations = []
    for index, case in enumerate(cases):
        if branch_only and case["kind"] != "branch":
            continue
        content = case["source"]
        assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"], case["id"]
        path = case_root / f"case-{index}.ps1"
        path.write_text(content)
        process = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, env=ENVIRONMENT, text=True,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        assert "Failed to load language" not in process.stdout
        expected = native[case["id"]]
        record = {"id": case["id"], "exit": process.returncode, "accepted": process.returncode == 0,
                  "native_accepted": expected["accepted"]}
        if case["id"].startswith("continued-") and "-none-" in case["id"]:
            record["historical"] = [item for item in historical if item["id"] == case["id"]]
            record["classification"] = "Older no-indent bare-dash limitation, outside the indented regression."
            observations.append(record)
            continue
        record["matches"] = record["accepted"] == expected["accepted"]
        if expected["accepted"] and record["accepted"]:
            record["commands"] = context_spans("command", content, process.stdout)
            record["native_commands"] = [command["text"] for command in expected["commands"]]
            record["matches"] = record["matches"] and record["commands"] == record["native_commands"]
            record["elseifs"] = len(re.findall(r"\(elseif_clause ", process.stdout))
            record["elses"] = len(re.findall(r"\(else_clause ", process.stdout))
            record["matches"] = record["matches"] and record["elseifs"] == sum(branch["clauses"] - 1 for branch in expected["branches"]) and record["elses"] == sum(int(branch["has_else"]) for branch in expected["branches"])
        if case["kind"] == "redirect":
            record["generic_words"] = context_spans("generic_token", content, process.stdout)
            record["native_locations"] = [item["file"] for item in expected["redirections"]]
            if case["id"].endswith("-dash"):
                record["target_scope"] = "Legacy generic-word start and captures; native single-dash target splitting is an older independent limitation."
        if case["id"] in MANIFEST["trace_cases"] and record["accepted"]:
            record["tree_sha256"] = hashlib.sha256(tree_text(process.stdout).encode()).hexdigest()
        results.append(record)
        if not record["matches"]:
            print("CONTEXT_PARSER_MISMATCH " + json.dumps(record) + "\n" + process.stdout, flush=True)
    (EVIDENCE / (stage + "-context-parser.json")).write_text(json.dumps({"results": results, "older_observations": observations}, indent=2) + "\n")
    expected_count = 258 if branch_only else 432
    assert len(results) == expected_count, (len(results), expected_count)
    assert all(record["matches"] for record in results)
    assert branch_only or len(observations) == 16
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in library.glob("*.so")]
    assert len(libraries) == 1
    print("NATIVE_CONTEXT_PARSERS_MATCH " + json.dumps({"stage": stage, "required": len(results), "older_observations": observations, "libraries": libraries}), flush=True)


def compare_public_queries(original: dict[str, list[str]]) -> None:
    module_file = SOURCE / "crates/sloplint-syntax/tests/grammar_gaps.rs"
    path = SOURCE / "crates/sloplint-syntax/tests/grammar_gaps/powershell_context_observer.rs"
    original_module = module_file.read_bytes()
    assert not path.exists()
    try:
        path.write_bytes((PROOF / "context-observer.rs").read_bytes())
        module_file.write_text(original_module.decode() + '\n#[path = "grammar_gaps/powershell_context_observer.rs"]\nmod powershell_context_observer;\n')
        text = run("candidate-public-query-observation", ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", "x86_64-unknown-linux-gnu", "powershell_context_observer::observe_public_redirection_queries", "--", "--exact", "--nocapture"])
        actual = [line for line in text.splitlines() if line.startswith("PUBLIC_QUERY_")]
        ids = {case["id"] for case in json.loads((PROOF / "context-cases.json").read_text()) if case["kind"] == "redirect" and not case["id"].startswith("mixed-") and "-em-" not in case["id"]}
        assert len(ids) == 62
        def relevant(lines: list[str]) -> list[str]:
            return sorted(line for line in lines if line.startswith("PUBLIC_QUERY_COMPILE ") or line.split()[1] in ids)
        expected = relevant(original["legacy"])
        observed = relevant(actual)
        if observed != expected:
            print("LEGACY_QUERY_MISMATCH " + json.dumps({"missing": sorted(set(expected) - set(observed)), "unexpected": sorted(set(observed) - set(expected))}), flush=True)
        assert observed == expected
        print("LEGACY_REDIRECTION_QUERIES_MATCH inputs=62 queries=5", flush=True)
    finally:
        module_file.write_bytes(original_module)
        path.unlink()

def verify_ownership_contracts(binary: Path) -> None:
    receipt = json.loads((PROOF / "ownership-diagnostic-receipt.json").read_text())
    archives = [path for path in (EVIDENCE / "ownership-verified").rglob("*")
                if path.is_file() and zipfile.is_zipfile(path)]
    assert len(archives) == 1
    archive = archives[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == receipt["zip_sha256"]
    with zipfile.ZipFile(archive) as handle:
        def member(name: str) -> bytes:
            matches = [path for path in handle.namelist() if Path(path).name == name]
            assert len(matches) == 1, (name, matches)
            return handle.read(matches[0])
        packed = member("experiment-packed.json.gz")
        assert len(packed) == receipt["gzip_bytes"]
        assert hashlib.sha256(packed).hexdigest() == receipt["gzip_sha256"]
        data = gzip.decompress(packed)
        assert len(data) == receipt["payload_bytes"]
        assert hashlib.sha256(data).hexdigest() == receipt["payload_sha256"]
        verified = json.loads(data)
        assert verified["manifest"]["base"] == MANIFEST["base"]
        assert verified["manifest"]["base_tree"] == MANIFEST["base_tree"]
        assert len(verified["outcomes"]) == 1
        outcome = verified["outcomes"][0]
        assert not outcome["eligible"] and outcome["focused_exit"] == outcome["corpus_exit"] == 0
        assert outcome["node_types_unchanged"]
        assert outcome["failures"] == [{"kind": "native-argument-case-count", "id": "last-comment-and-redirect-guards", "expected": 18, "actual": 17}]
        # The application snapshot adds permanent tests, documentation and patch provenance.
        # Every compiled grammar/scanner byte must match the verified diagnostic source.
        for relative, digest in outcome["source"]["files"].items():
            assert hashlib.sha256((SOURCE / relative).read_bytes()).hexdigest() == digest, relative
        frozen = json.loads(member("formatted-probe-tests.json"))
        regression = next(item for item in frozen if item["path"].endswith("/powershell_separator_ownership.rs"))
        assert hashlib.sha256((SOURCE / regression["path"]).read_bytes()).hexdigest() == regression["sha256"]
        assert regression["sha256"] == MANIFEST["formatted_regression_sha256"]
        for result in verified["results"]:
            assert not result["timed_out"], result
            if result["expected"] is not None:
                assert result["exit"] == result["expected"], result
            elif result["name"].startswith("double-gap-"):
                assert result["exit"] in (0, 1), result
            else:
                assert result["exit"] == 0, result
            content = member(result["name"] + ".log")
            assert hashlib.sha256(content).hexdigest() == result["log_sha256"], result["name"]
        correction = json.loads((PROOF / "ownership-output-correction.json").read_text())
        focused_log_bytes = member(outcome["id"] + "-focused-powershell.log")
        assert hashlib.sha256(focused_log_bytes).hexdigest() == correction["focused_log_sha256"]
        focused_log = focused_log_bytes.decode()
        assert correction["missing_line"] in focused_log
        original_prefix_count = sum(line.startswith("GUARD_NATIVE_ARG_CASE ") for line in focused_log.splitlines())
        recovered_counts = re.findall(r"GUARD_NATIVE_ARG_CASE ([a-z0-9-]+) ([0-9]+)(?=\s|$)", focused_log)
        assert original_prefix_count == correction["old_line_prefix_count"] == 17
        assert len(recovered_counts) == 18
        assert [[identity, int(count)] for identity, count in recovered_counts] == correction["exact_count_records"]
        print("VERIFIED_PRESERVED_COUNT_RECORDS " + json.dumps({
            "diagnostic_conclusion": "failure", "failed_observer_repeat": 45914586866,
            "inspection": correction, "recovered_count": len(recovered_counts),
        }), flush=True)
        zero_width_logs = [result for result in verified["results"] if "-trace-" in result["name"]]
        assert len(zero_width_logs) == 13
        for result in zero_width_logs:
            text = member(result["name"] + ".log").decode()
            guards = re.findall(r"lexed_lookahead sym:(_commented_(?:argument|redirect)), size:(\d+)", text)
            assert all(size == "0" for _, size in guards), result["name"]
        redirect_trace = member(outcome["id"] + "-trace-comment-redirect-single-dash.log").decode()
        assert "lexed_lookahead sym:_commented_redirect, size:0" in redirect_trace
        assert "COMMENTED_REDIRECT_PROBE accepted=1" in redirect_trace
        negative_trace = member(outcome["id"] + "-trace-external-comment-redirect-space-named.log").decode()
        assert "COMMENTED_REDIRECT_PROBE accepted=0" in negative_trace
    print("VERIFIED_OWNERSHIP_DIAGNOSTIC " + json.dumps({"receipt": receipt, "source": outcome["source"],
          "zero_width_trace_cases": len(zero_width_logs), "permanent_regression_sha256": regression["sha256"]}), flush=True)
    for current, frozen_name in (
        ("ownership-cases.json", "external-boundary-cases.json"),
        ("ownership-cases.tsv", "external-boundary-cases.tsv"),
        ("ownership-observer.rs", "external-observer.rs"),
        ("ownership-double-gap-cases.json", "double-gap-cases.json"),
        ("ownership-double-gap-cases.tsv", "double-gap-cases.tsv"),
        ("ownership-native-oracle.ps1", "double-gap-oracle.ps1"),
        ("ownership-prefix-cases.json", "prefix-boundary-cases.json"),
    ):
        assert hashlib.sha256((PROOF / current).read_bytes()).hexdigest() == verified["manifest"]["proof_sha256"][frozen_name], current

    cases = json.loads((PROOF / "ownership-cases.json").read_text())
    additional = json.loads((PROOF / "ownership-double-gap-cases.json").read_text())
    prefix_cases = json.loads((PROOF / "ownership-prefix-cases.json").read_text())
    assert len(cases) == 801 and len(additional) == len(prefix_cases) == 2
    external_receipt = verified["external_receipt"]
    native_packed = base64.b64decode((PROOF / "ownership-native-baseline-packed.b64").read_text())
    assert len(native_packed) == external_receipt["gzip_bytes"]
    assert hashlib.sha256(native_packed).hexdigest() == external_receipt["gzip_sha256"]
    native_bytes = gzip.decompress(native_packed)
    assert len(native_bytes) == external_receipt["payload_bytes"]
    assert hashlib.sha256(native_bytes).hexdigest() == external_receipt["payload_sha256"]
    expected_native = json.loads(native_bytes)["native"]
    assert len(expected_native) == len(cases)
    pwsh = EVIDENCE / "powershell/pwsh"
    native_results = {}
    for label, case_path, oracle_path, expected in (
        ("all", "ownership-cases.json", "ownership-native-oracle.ps1", expected_native),
        ("double-gap", "ownership-double-gap-cases.json", "ownership-native-oracle.ps1", verified["double_gap_native"]),
        ("completed-prefix", "ownership-prefix-cases.json", "ownership-prefix-oracle.ps1", [item["native"] for item in outcome["prefix_controls"]]),
    ):
        text = run("ownership-native-" + label, [str(pwsh), "-NoLogo", "-NoProfile", "-File",
                   str(PROOF / oracle_path), "-CasesPath", str(PROOF / case_path)])
        actual = [json.loads(line.removeprefix("NATIVE_CONTEXT ")) for line in text.splitlines() if line.startswith("NATIVE_CONTEXT ")]
        assert actual == expected and all(item["matches"] for item in actual), label
        native_results[label] = actual
    (EVIDENCE / "ownership-native.json").write_text(json.dumps(native_results, indent=2) + "\n")
    # A dedicated one-test invocation keeps protocol records separate from concurrent progress.
    word_ids = {case["id"] for case in cases if case.get("scope") == "word-list"}
    expected_counts = {item["id"]: item["native_count"] for item in native_results["all"] if item["id"] in word_ids}
    assert len(word_ids) == len(expected_counts) == 18
    assert {identity: int(count) for identity, count in recovered_counts} == expected_counts
    count_test = run("ownership-isolated-native-counts", [
        "cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps",
        "--target", "x86_64-unknown-linux-gnu",
        "powershell_separator_ownership::comments_keep_array_arguments_and_protected_commas",
        "--", "--exact", "--nocapture", "--test-threads=1",
    ])
    assert "running 1 test" in count_test and "1 passed; 0 failed" in count_test
    fresh_counts = re.findall(r"GUARD_NATIVE_ARG_CASE ([a-z0-9-]+) ([0-9]+)(?=\s|$)", count_test)
    assert len(fresh_counts) == 18 and {identity: int(count) for identity, count in fresh_counts} == expected_counts
    print("ISOLATED_NATIVE_ARGUMENT_COUNTS_MATCH " + json.dumps({"count": 18, "records": fresh_counts}), flush=True)

    library = EVIDENCE / "ownership-final-libraries"
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    root = EVIDENCE / "ownership-final-cases"
    root.mkdir()
    parser_results = []
    expected_parsers = {record["id"]: record for record in outcome["parsers"]}
    for index, case in enumerate(cases):
        content = case["source"]
        assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"]
        path = root / f"case-{index}.ps1"
        path.write_text(content)
        result = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, env=ENVIRONMENT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30, check=False)
        assert result.returncode in (0, 1) and "Failed to load language" not in result.stdout, case["id"]
        canonical = "\n".join(line for line in result.stdout.splitlines() if re.match(r"^\s*(?:[A-Za-z_]+: )?\(", line)) + "\n"
        record = {"id": case["id"], "exit": result.returncode, "accepted": result.returncode == 0,
                  "canonical_tree_sha256": hashlib.sha256(canonical.encode()).hexdigest()}
        for key, kind in (("commands", "command"), ("generic_words", "generic_token"),
                          ("target_nodes", "redirected_file_name"), ("redirections", "redirection"),
                          ("arrays", "array_literal_expression"), ("separator_nodes", "command_argument_sep"),
                          ("comments", "comment")):
            record[key] = context_spans(kind, content, result.stdout)
        expected = expected_parsers[case["id"]]
        for key, value in record.items():
            assert value == expected[key], (case["id"], key, expected[key], value)
        parser_results.append(record)
    prefix_results = []
    for index, case in enumerate(prefix_cases):
        content = case["source"]
        assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"]
        path = root / f"prefix-{index}.ps1"
        path.write_text(content)
        text = run("ownership-final-" + case["id"], [str(binary), "parse", str(path)], cwd=VENDOR)
        record = {"id": case["id"], "source_sha256": case["source_sha256"]}
        for key, kind in (("commands", "command"), ("parameters", "command_parameter"),
                          ("generic_words", "generic_token"), ("separator_nodes", "command_argument_sep"),
                          ("comments", "comment")):
            record[key] = context_spans(kind, content, text)
        expected = outcome["prefix_controls"][index]
        for key, value in record.items():
            assert value == expected[key], (case["id"], key, expected[key], value)
        record["limitation"] = expected["limitation"]
        prefix_results.append(record)
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                 for path in library.glob("*.so")]
    assert len(libraries) == 1
    print("OWNERSHIP_FINAL_PARSER_MATCH " + json.dumps({"cases": len(parser_results),
          "prefix_controls": prefix_results, "libraries": libraries}), flush=True)

    all_cases = EVIDENCE / "ownership-all-public-cases.tsv"
    all_cases.write_bytes((PROOF / "ownership-cases.tsv").read_bytes() + (PROOF / "ownership-double-gap-cases.tsv").read_bytes())
    module_file = SOURCE / "crates/sloplint-syntax/tests/grammar_gaps.rs"
    original_module = module_file.read_bytes()
    observer = SOURCE / "crates/sloplint-syntax/tests/grammar_gaps/powershell_separator_observations.rs"
    assert not observer.exists()
    ENVIRONMENT["SLOPLINT_BOUNDARY_CASES"] = str(all_cases)
    try:
        observer.write_bytes((PROOF / "ownership-observer.rs").read_bytes())
        module_file.write_text(original_module.decode() + '\n#[path = "grammar_gaps/powershell_separator_observations.rs"]\nmod powershell_separator_observations;\n')
        run("ownership-format-observer", ["cargo", "fmt", "--all"])
        assert hashlib.sha256(observer.read_bytes()).hexdigest() == verified["manifest"]["formatted_observer_sha256"]
        args = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", "x86_64-unknown-linux-gnu"]
        run("ownership-compile-public-observer", [*args, "--no-run"])
        text = run("ownership-final-public-queries", [*args, "powershell_separator_observations::observe_separator_ownership_queries", "--", "--exact", "--nocapture"])
        assert "running 1 test" in text and "1 passed; 0 failed" in text
        actual_queries = [line for line in text.splitlines() if line.startswith("OWNERSHIP_QUERY_")]
        assert sum(line.startswith("OWNERSHIP_QUERY_COMPILE ") and line.endswith(" OK") for line in actual_queries) == 21
        assert sum(line.startswith("OWNERSHIP_QUERY_ROW ") for line in actual_queries) == 803 * 21
        assert sorted(actual_queries) == sorted(outcome["queries"]), "Final public captures differ from the exact verified candidate"
    finally:
        module_file.write_bytes(original_module)
        observer.unlink()
    report = {"receipt": receipt, "historical_diagnostic_conclusion": "failure", "output_correction": correction,
              "fresh_count_records": fresh_counts, "grammar_source": outcome["source"], "native_cases": 805,
              "parser_cases": len(parser_results), "prefix_controls": prefix_results,
              "query_inputs": 803, "queries_per_input": 21, "failures": [],
              "parsers": parser_results, "queries": actual_queries}
    (EVIDENCE / "ownership-contracts.json").write_text(json.dumps(report, indent=2) + "\n")
    print("OWNERSHIP_FINAL_CONTRACTS_MATCH native=805 parser=801 prefix=2 query_inputs=803 queries=21", flush=True)


def prepare() -> None:
    install_rust()
    binary = tree_sitter()
    native_oracles()
    queries = baseline_queries()
    controls = json.loads((PROOF / "near-controls.json").read_text())
    control_root = EVIDENCE / "near-controls"
    control_root.mkdir()
    control_library = EVIDENCE / "original-control-parser"
    control_library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(control_library)
    original_controls = {}
    for index, case in enumerate(controls):
        path = control_root / f"case-{index}.ps1"
        path.write_text(case["source"])
        text = run("original-" + case["id"], [str(binary), "parse", str(path)], cwd=VENDOR)
        original_controls[case["id"]] = tree_text(text)
    (EVIDENCE / "original-control-trees.json").write_text(json.dumps(original_controls, indent=2) + "\n")
    old_patch = (VENDOR / "sloplint-grammar.patch").read_bytes()
    upstream = EVIDENCE / "upstream"
    run("upstream-init", ["git", "init", "-q", str(upstream)])
    run("upstream-origin", ["git", "remote", "add", "origin", "https://github.com/airbus-cert/tree-sitter-powershell"], cwd=upstream)
    run("upstream-fetch", ["git", "fetch", "--depth=1", "origin", "65ea6919e816a7ac142acec7ee72757f89db8439"], cwd=upstream)
    run("upstream-checkout", ["git", "checkout", "-q", "FETCH_HEAD"], cwd=upstream)
    previous_patch = EVIDENCE / "previous.patch"
    previous_patch.write_bytes(old_patch)
    run("upstream-patch", ["git", "apply", "--index", str(previous_patch)], cwd=upstream)
    authored = cast(list[dict[str, object]], json.loads((PROOF / "authored-stages.json").read_text()))
    stages: list[dict[str, object]] = []
    prior_files: dict[str, str] = {}
    prior_tree = str(MANIFEST["base_tree"])
    allowed: set[str] = set()
    generated = {"vendor/tree-sitter-powershell/src/parser.c", "vendor/tree-sitter-powershell/src/grammar.json",
                 "vendor/tree-sitter-powershell/src/node-types.json", "vendor/tree-sitter-powershell/sloplint-grammar.patch"}
    regressions = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "--target", "x86_64-unknown-linux-gnu", "powershell", "--", "--nocapture"]
    for stage in authored:
        name = str(stage["id"])
        elements = cast(list[dict[str, str]], stage["elements"])
        apply_elements(elements)
        allowed.update(item["path"] for item in elements)
        if stage["regenerate"]:
            run(name + "-scanner-syntax", ["cc", "-fsyntax-only", "-std=c11", "-I", str(VENDOR / "src"), str(VENDOR / "src/scanner.c")])
            run(name + "-generate", [str(binary), "generate"], cwd=VENDOR)
            states = re.search(r"#define STATE_COUNT (\d+)", (VENDOR / "src/parser.c").read_text())
            assert states is not None
            notes = VENDOR / "SLOPLINT.md"
            notes.write_text(re.sub(r"The regenerated parser uses ABI 15 and has [0-9,]+ states.",
                                   f"The regenerated parser uses ABI 15 and has {int(states.group(1)):,} states.", notes.read_text()))
            for relative in ("grammar.js", "src/scanner.c"):
                (upstream / relative).write_bytes((VENDOR / relative).read_bytes())
            patch = subprocess.check_output(["git", "diff", "--binary", "HEAD", "--", "grammar.js", "src/scanner.c", "test/corpus"], cwd=upstream)
            patch = b"\n".join(b"" if line == b" " else line for line in patch.split(b"\n"))
            (VENDOR / "sloplint-grammar.patch").write_bytes(patch)
            run(name + "-corpus", [str(binary), "test"], cwd=VENDOR)
        notes = VENDOR / "SLOPLINT.md"
        patch_content = (VENDOR / "sloplint-grammar.patch").read_bytes()
        state_count = re.search(r"#define STATE_COUNT (\d+)", (VENDOR / "src/parser.c").read_text())
        assert state_count is not None
        text, count = re.subn(r"The regenerated parser uses ABI 15 and has [0-9,]+ states.",
                             f"The regenerated parser uses ABI 15 and has {int(state_count.group(1)):,} states.", notes.read_text())
        assert count == 1
        patch_lines = len(patch_content.splitlines())
        patch_hunks = sum(line.startswith(b"@@ ") for line in patch_content.splitlines())
        text, count = re.subn(r"The current patch has [0-9,]+ lines and [0-9,]+ diff hunks.",
                             f"The current patch has {patch_lines:,} lines and {patch_hunks:,} diff hunks.", text)
        assert count == 1
        notes.write_text(text)
        run(name + "-format", ["cargo", "fmt", "--all"])
        test_file = SOURCE / "crates/sloplint-syntax/tests/grammar_gaps/powershell_separator_ownership.rs"
        assert hashlib.sha256(test_file.read_bytes()).hexdigest() == MANIFEST["formatted_regression_sha256"]
        changed = set(output(["git", "diff", "--name-only"]).splitlines())
        changed.update(output(["git", "ls-files", "--others", "--exclude-standard"]).splitlines())
        assert changed <= allowed | generated, sorted(changed - allowed - generated)
        for relative in changed:
            if relative.startswith("crates/"):
                assert len((SOURCE / relative).read_text().splitlines()) < 600, relative
        output(["git", "read-tree", "HEAD"], env=INDEX_ENVIRONMENT)
        output(["git", "add", "--", *sorted(changed)], env=INDEX_ENVIRONMENT)
        tree = output(["git", "write-tree"], env=INDEX_ENVIRONMENT)
        identity = full_identity(tree, changed)
        delta: list[dict[str, str]] = []
        for relative in sorted(changed):
            content = (SOURCE / relative).read_text()
            digest = hashlib.sha256(content.encode()).hexdigest()
            if prior_files.get(relative) != digest:
                delta.append({"path": relative, "content": content, "sha256": digest})
            prior_files[relative] = digest
        record = {"id": name, "message": stage["message"], "base_tree": prior_tree, "tree": tree,
                  "identity": identity, "paths": sorted(changed), "elements": delta}
        stages.append(record)
        prior_tree = tree
        print("CANDIDATE_STAGE " + json.dumps({k: v for k, v in record.items() if k != "elements"}, sort_keys=True), flush=True)
        run(name + "-whitespace", ["git", "diff", "--check"])
        if not stage["regenerate"]:
            assert name == "recovery-documentation"
            continue
        text = run(str(stage["id"]) + "-powershell-regressions", regressions)
        native = cast(list[dict[str, object]], json.loads((EVIDENCE / "native-cases.json").read_text()))
        expected = {str(item["id"]): item["native_count"] for item in native if item["kind"] == "arguments"}
        observed = re.findall(r"NATIVE_ARGUMENT_CASE ([^\s]+) (\d+)", text)
        counts = {name: int(count) for name, count in observed}
        assert len(expected) == 190 and len(observed) == 190 and counts == expected, (len(counts), len(expected))
        print("NATIVE_ARGUMENT_COUNTS_MATCH 190", flush=True)
        followup = json.loads((EVIDENCE / "followup-native-cases.json").read_text())
        expected_dash = {item["id"]: item["native_count"] for item in followup if item["kind"] == "arguments" and "-protected-" not in item["id"]}
        observed_dash = re.findall(r"NATIVE_DASH_CASE ([^\s]+) (\d+)", text)
        counts_dash = {name: int(count) for name, count in observed_dash}
        assert len(expected_dash) == 42 and len(observed_dash) == 42 and counts_dash == expected_dash
        expected_comma = {item["id"]: item["native_count"] for item in followup if item["kind"] == "arguments_observe"}
        observed_comma = re.findall(r"NATIVE_COMMA_CASE ([^\s]+) (\d+)", text)
        assert len(expected_comma) == len(observed_comma) == 18
        assert {name: int(count) for name, count in observed_comma} == expected_comma
        print("FOLLOWUP_NATIVE_ARGUMENT_COUNTS_MATCH bare=42 comma=18 protected=12", flush=True)
        observed_branches = re.findall(r"NATIVE_BRANCH_CASE ([^\s]+)", text)
        expected_branches = {item["id"] for item in json.loads((PROOF / "followup-cases.json").read_text()) if item["kind"] == "conditional"}
        assert len(observed_branches) == len(expected_branches) == 96 and set(observed_branches) == expected_branches
        observed_queries = [line for line in text.splitlines() if line.startswith("LEGACY_SEPARATOR_QUERY ")]
        expected_queries = json.loads((PROOF / "query-baseline-lines.json").read_text())
        assert len(observed_queries) == len(expected_queries) == 24 and sorted(observed_queries) == sorted(expected_queries)
        print("ORIGINAL_CUSTOM_QUERY_CAPTURE_SPANS_MATCH 24", flush=True)
        if name == "separator-ownership":
            observed_continuations = re.findall(r"NATIVE_CONTINUED_DASH_CASE ([^\s]+) (\d+)", text)
            expected_continuations = {record["id"]: record["native_count"] for record in json.loads((EVIDENCE / "native-context-cases.json").read_text()) if record["id"].startswith("continued-") and "-none-" not in record["id"]}
            assert len(observed_continuations) == len(expected_continuations) == 96
            assert {identifier: int(count) for identifier, count in observed_continuations} == expected_continuations
            print("NATIVE_INDENTED_DASH_COUNTS_MATCH 96", flush=True)
        assert full_identity(tree, changed) == identity
    context_parsers(binary, "final")
    compare_public_queries(queries)
    verify_ownership_contracts(binary)
    assert full_identity(str(stages[-1]["tree"]), set(cast(list[str], stages[-1]["paths"]))) == stages[-1]["identity"]
    payload = {"manifest": MANIFEST, "stages": stages}
    data = (json.dumps(payload, indent=2) + "\n").encode()
    (EVIDENCE / "stages.json").write_bytes(data)
    digest = hashlib.sha256(data).hexdigest()
    print("CANDIDATE_ARTIFACT_SHA256 " + digest, flush=True)
    packed = gzip.compress(data, mtime=0)
    encoded = base64.b64encode(packed).decode("ascii")
    (EVIDENCE / "stages.json.gz").write_bytes(packed)
    print("PACKED_SOURCE_IDENTITY " + json.dumps({"payload_sha256": digest,
          "gzip_sha256": hashlib.sha256(packed).hexdigest(), "gzip_bytes": len(packed),
          "payload_bytes": len(data), "base64_bytes": len(encoded),
          "chunks": (len(encoded) + 23999) // 24000}), flush=True)
    for part, offset in enumerate(range(0, len(encoded), 24000)):
        print("PACKED_SOURCE_CHUNK " + json.dumps({"part": part, "content": encoded[offset:offset + 24000]}), flush=True)
    cases = cast(list[dict[str, object]], json.loads((PROOF / "preserved-grammar-cases.json").read_text()))
    cases += cast(list[dict[str, object]], json.loads((PROOF / "cases.json").read_text()))
    cases += cast(list[dict[str, object]], json.loads((PROOF / "token-parser-cases.json").read_text()))
    controls = cast(list[dict[str, object]], json.loads((PROOF / "terminator-parser-cases.json").read_text()))
    observed = cast(list[dict[str, object]], json.loads((PROOF / "terminator-native-expected.json").read_text()))
    assert len(controls) == len(observed) == 24
    for case, record in zip(controls, observed, strict=True):
        assert case["source"] == record["source"] and case["accepted"] == record["accepted"]
        assert sum(item["clauses"] - 1 for item in cast(list[dict[str, int]], record["branches"])) == case["elseifs"]
        assert sum(int(item["has_else"]) for item in cast(list[dict[str, bool]], record["branches"])) == case["elses"]
    cases += controls
    followup = cast(list[dict[str, object]], json.loads((PROOF / "followup-cases.json").read_text()))
    cases += [case for case in followup if case["kind"] != "observation"]
    library = EVIDENCE / "candidate-parser-library"
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    case_root = EVIDENCE / "cases"
    case_root.mkdir(exist_ok=True)
    results: list[dict[str, object]] = []
    for index, case in enumerate(cases):
        path = case_root / f"case-{index}.ps1"
        content = str(case["source"])
        if "source_sha256" in case:
            assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"], case["id"]
        path.write_text(content)
        process = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, env=ENVIRONMENT,
                                 text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        assert "Failed to load language" not in process.stdout
        record = {"id": case["id"], "exit": process.returncode, "accepted": process.returncode == 0, "expected": case["accepted"]}
        if "elseifs" in case:
            record["structure_matches"] = (
                len(re.findall(r"\(elseif_clause ", process.stdout)) == case["elseifs"]
                and len(re.findall(r"\(else_clause ", process.stdout)) == case["elses"])
        if str(case.get("kind", "")).startswith("arguments") and case["id"].startswith(("dash-", "comma-")):
            lines = content.encode().splitlines(keepends=True)
            words = []
            for match in re.finditer(r"\(generic_token \[(\d+), (\d+)\] - \[(\d+), (\d+)\]", process.stdout):
                start_row, start_column, end_row, end_column = map(int, match.groups())
                start = sum(map(len, lines[:start_row])) + start_column
                end = sum(map(len, lines[:end_row])) + end_column
                words.append(content.encode()[start:end].decode())
            record["generic_words"] = words
            if case["kind"] == "arguments" and not case.get("protected", False):
                record["word_matches"] = words == [case["argument_source"]]
            elif case["kind"] == "arguments_observe":
                native_record = next(item for item in json.loads((PROOF / "followup-native-expected.json").read_text()) if item["id"] == case["id"])
                native_argument = native_record["argument_nodes"][0]
                if native_argument["kind"] == "ArrayLiteralAst":
                    first = next(token["text"] for token in native_record["tokens"] if token["kind"] == "Generic" and token["flags"] != "CommandName")
                    record["word_matches"] = first in words and not any("," in word for word in words) and "(array_literal_expression " in process.stdout
                else:
                    record["word_matches"] = words == [native_argument["text"]]
        results.append(record)
        if record["accepted"] != record["expected"] or not record.get("structure_matches", True) or not record.get("word_matches", True):
            print("PARSER_MISMATCH " + json.dumps(record) + "\n" + process.stdout, flush=True)
    (EVIDENCE / "parser-cases.json").write_text(json.dumps(results, indent=2) + "\n")
    assert len(results) == 653
    for index, case in enumerate(json.loads((PROOF / "near-controls.json").read_text())):
        path = EVIDENCE / "near-controls" / f"case-{index}.ps1"
        text = run("candidate-" + case["id"], [str(binary), "parse", str(path)], cwd=VENDOR)
        assert tree_text(text) == original_controls[case["id"]], case["id"]
    print("NATIVE_NEAR_CONTROL_TREES_UNCHANGED 6", flush=True)
    assert all(item["accepted"] == item["expected"] and item.get("structure_matches", True) and item.get("word_matches", True) for item in results)
    observations = []
    for index, case in enumerate(case for case in followup if case["kind"] == "observation"):
        path = case_root / f"observation-{index}.ps1"
        path.write_text(str(case["source"]))
        process = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, env=ENVIRONMENT, text=True,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        observations.append({"id": case["id"], "native_accepted": True, "pre_r20_accepted": False, "published_r20_accepted": False,
                             "candidate_accepted": process.returncode == 0, "actual_exit": process.returncode,
                             "classification": "Native-valid leading escaped command name rejected on both historical snapshots; outside this source repair."})
    (EVIDENCE / "prior-limit-observations.json").write_text(json.dumps(observations, indent=2) + "\n")
    print("PRIOR_LIMIT_OBSERVATIONS " + json.dumps(observations), flush=True)
    libraries = [{"path": str(path.relative_to(EVIDENCE)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in library.glob("*.so")]
    assert len(libraries) == 1
    print("ISOLATED_CANDIDATE_PARSER " + json.dumps(libraries), flush=True)
    assert full_identity(str(stages[-1]["tree"]), set(cast(list[str], stages[-1]["paths"]))) == stages[-1]["identity"]
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as handle:
        handle.write(f"stages_sha256={digest}\n")
    print("CANDIDATE_PREPARED " + str(stages[-1]["tree"]) + " native_parser_cases=653", flush=True)


def apply_stage(stage: dict[str, object]) -> None:
    apply_elements(cast(list[dict[str, str]], stage["elements"]))
    paths = cast(list[str], stage["paths"])
    output(["git", "read-tree", "HEAD"], env=INDEX_ENVIRONMENT)
    output(["git", "add", "--", *paths], env=INDEX_ENVIRONMENT)
    assert full_identity(str(stage["tree"]), set(paths)) == stage["identity"]

def validate(mode: str) -> None:
    data = (EVIDENCE / "transfer/stages.json").read_bytes()
    assert hashlib.sha256(data).hexdigest() == os.environ["STAGES_SHA256"]
    payload = cast(dict[str, object], json.loads(data))
    assert payload["manifest"] == MANIFEST
    stages = cast(list[dict[str, object]], payload["stages"])
    install_rust()
    for stage in stages:
        apply_stage(stage)
        assert full_identity(str(stage["tree"]), set(cast(list[str], stage["paths"]))) == stage["identity"]
    final = stages[-1]
    if mode in ("x64", "arm64"):
        target = "x86_64-unknown-linux-gnu" if mode == "x64" else "aarch64-unknown-linux-gnu"
        assert "host: " + target in output(["rustc", "-vV"])
        run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--target", target, "--", "-D", "warnings"])
        run("workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast", "--target", target])
        run("release", ["cargo", "build", "--release", "--locked", "--target", target])
        if mode == "x64":
            run("self-lint", [str(SOURCE / "target" / target / "release/sloplint"), "check", "crates", "--format", "agent",
                              "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
    elif mode == "lint":
        binary = tree_sitter()
        run("fmt", ["cargo", "fmt", "--all", "--check"])
        run("grammar-manifest", ["bash", "scripts/grammars.sh", "check"])
        run("release-script-selftests", ["bash", "scripts/selftest-release-scripts.sh"])
        archive = download("https://github.com/EmbarkStudios/cargo-deny/releases/download/0.20.2/cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz",
                           "9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f", "cargo-deny.tar.gz")
        run("extract-cargo-deny", ["tar", "-xzf", str(archive), "-C", str(EVIDENCE), "cargo-deny-0.20.2-x86_64-unknown-linux-musl/cargo-deny"])
        ENVIRONMENT["PATH"] = str(EVIDENCE / "cargo-deny-0.20.2-x86_64-unknown-linux-musl") + os.pathsep + ENVIRONMENT["PATH"]
        run("cargo-deny", ["cargo", "deny", "check"])
        ENVIRONMENT["GRAMMARS_TS_BIN"] = str(binary)
        ENVIRONMENT["GRAMMARS_SCRATCH"] = str(EVIDENCE / "verify-upstream")
        run("authoritative-grammar-verify", ["bash", "scripts/grammars.sh", "verify", "tree-sitter-powershell"])
        run("grammar-corpus", [str(binary), "test"], cwd=VENDOR)
        run("fresh-generation", [str(binary), "generate"], cwd=VENDOR)
    else:
        raise AssertionError(mode)
    run("whitespace", ["git", "diff", "--check"])
    assert full_identity(str(final["tree"]), set(cast(list[str], final["paths"]))) == final["identity"]
    receipt = {"mode": mode, "source": final["identity"], "stages_sha256": os.environ["STAGES_SHA256"], "results": RESULTS}
    (EVIDENCE / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("CANDIDATE_GATE_PASS " + json.dumps(receipt, sort_keys=True), flush=True)

for name, digest in cast(dict[str, str], MANIFEST["proof_sha256"]).items():
    assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
assert output(["git", "status", "--porcelain", "--untracked-files=all"]) == ""
output(["git", "read-tree", "HEAD"], env=INDEX_ENVIRONMENT)
full_identity(str(MANIFEST["base_tree"]), set())
if sys.argv[1] == "prepare":
    prepare()
else:
    validate(sys.argv[1])
