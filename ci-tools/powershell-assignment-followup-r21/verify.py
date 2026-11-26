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
PROOF = WORKSPACE / "proof/ci-tools/powershell-assignment-followup-r21"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-assignment-followup-r21"
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
    print(f"PROBE_EXIT {name} {process.returncode} timed_out={timed_out}\n{text[-25000:]}", flush=True)
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
    return records

def tree_text(text: str) -> str:
    start = text.index("(program")
    return text[start:text.rindex(")") + 1]

def prepare() -> None:
    install_rust()
    binary = tree_sitter()
    native_oracles()
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
        run(name + "-format", ["cargo", "fmt", "--all"])
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
        text = run(str(stage["id"]) + "-powershell-regressions", regressions)
        native = cast(list[dict[str, object]], json.loads((EVIDENCE / "native-cases.json").read_text()))
        expected = {str(item["id"]): item["native_count"] for item in native if item["kind"] == "arguments"}
        observed = re.findall(r"NATIVE_ARGUMENT_CASE ([^\s]+) (\d+)", text)
        counts = {name: int(count) for name, count in observed}
        assert len(expected) == 190 and len(observed) == 190 and counts == expected, (len(counts), len(expected))
        print("NATIVE_ARGUMENT_COUNTS_MATCH 190", flush=True)
        if stage["id"] in ("format-mark-words", "legacy-query-tokens"):
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
        if stage["id"] == "legacy-query-tokens":
            observed_queries = [line for line in text.splitlines() if line.startswith("LEGACY_SEPARATOR_QUERY ")]
            expected_queries = json.loads((PROOF / "query-baseline-lines.json").read_text())
            assert len(observed_queries) == len(expected_queries) == 24 and sorted(observed_queries) == sorted(expected_queries)
            print("ORIGINAL_CUSTOM_QUERY_CAPTURE_SPANS_MATCH 24", flush=True)
        assert full_identity(tree, changed) == identity
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
