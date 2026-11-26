from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import urllib.request
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "proof/ci-tools/powershell-three-findings-r20"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-three-findings-r20"
EVIDENCE.mkdir(exist_ok=True)
VENDOR = SOURCE / "vendor/tree-sitter-powershell"
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
    RESULTS.append({"name": name, "args": args, "exit": process.returncode, "expected": expected,
                    "seconds": time.monotonic() - started, "timed_out": timed_out,
                    "log_sha256": hashlib.sha256(text.encode()).hexdigest()})
    (EVIDENCE / "results.json").write_text(json.dumps({"manifest": MANIFEST, "results": RESULTS}, indent=2) + "\n")
    print(f"PROBE_EXIT {name} {process.returncode} timed_out={timed_out}\n{text[-25000:]}", flush=True)
    assert not timed_out, f"{name}: actual timeout exit {process.returncode}"
    assert expected is None or process.returncode == expected, (name, process.returncode, expected)
    return text

def download(url: str, digest: str, name: str) -> Path:
    path = EVIDENCE / name
    with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    assert actual == digest, (name, actual, digest)
    print(f"VERIFIED_DOWNLOAD {name} {actual}", flush=True)
    return path

def source_identity() -> dict[str, object]:
    assert output(["git", "rev-parse", "HEAD"]) == MANIFEST["base"]
    assert output(["git", "write-tree"]) == MANIFEST["base_tree"]
    assert output(["git", "status", "--porcelain", "--untracked-files=all"]) == ""
    index = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=SOURCE)
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
        actual = hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest()
        assert actual == expected.decode(), os.fsdecode(relative)
        count += 1
    record = {"head": MANIFEST["base"], "tree": MANIFEST["base_tree"], "tracked_blobs": count,
              "index_sha256": hashlib.sha256(index).hexdigest()}
    print("BASELINE_SOURCE_IDENTITY " + json.dumps(record, sort_keys=True), flush=True)
    return record

for name, digest in cast(dict[str, str], MANIFEST["proof_sha256"]).items():
    assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
initial = source_identity()
(EVIDENCE / "baseline-identity.json").write_text(json.dumps(initial, indent=2, sort_keys=True) + "\n")
run("rust-toolchain", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
assert output(["rustc", "--version"]).startswith("rustc 1.100.0 ")
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
tree_sitter = EVIDENCE / "tree-sitter"
tree_sitter.write_bytes(gzip.decompress(archive.read_bytes()))
tree_sitter.chmod(0o755)
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
native_output = run("native-new-cases", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "native-oracle.ps1"), "-CasesPath", str(PROOF / "cases.json")], expected=None)
native = [json.loads(line.removeprefix("NATIVE_CASE ")) for line in native_output.splitlines() if line.startswith("NATIVE_CASE ")]
cases = cast(list[dict[str, object]], json.loads((PROOF / "cases.json").read_text()))
assert len(native) == len(cases)
native_by_id = {item["id"]: item for item in native}
assert len(native_by_id) == len(cases)
(EVIDENCE / "native-cases.json").write_text(json.dumps(native, indent=2, sort_keys=True) + "\n")
parser_results: list[dict[str, object]] = []
case_dir = EVIDENCE / "cases"
case_dir.mkdir(exist_ok=True)
for index, case in enumerate(cases):
    content = str(case["source"])
    assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"]
    path = case_dir / f"case-{index}.ps1"
    path.write_text(content)
    process = subprocess.run([str(tree_sitter), "parse", "--quiet", str(path)], cwd=VENDOR, env=ENVIRONMENT,
                             text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
    assert "Failed to load language" not in process.stdout
    native_case = native_by_id[case["id"]]
    parser_results.append({"id": case["id"], "exit": process.returncode, "accepted": process.returncode == 0,
                           "native_accepted": native_case["accepted"], "native_matches_hypothesis": native_case["matches"]})
(EVIDENCE / "baseline-parser-cases.json").write_text(json.dumps(parser_results, indent=2, sort_keys=True) + "\n")
print("BASELINE_PARSER_MISMATCHES " + json.dumps([item for item in parser_results if item["accepted"] != item["native_accepted"]]), flush=True)

elements = cast(list[dict[str, str]], json.loads((PROOF / "frozen-elements.json").read_text()))
saved: dict[str, bytes | None] = {}
try:
    for item in elements:
        path = SOURCE / item["path"]
        saved[item["path"]] = path.read_bytes() if path.exists() else None
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(item["content"])
    run("format-frozen-tests", ["cargo", "fmt", "--all"])
    frozen = [{"path": item["path"], "content": (SOURCE / item["path"]).read_text(),
               "sha256": hashlib.sha256((SOURCE / item["path"]).read_bytes()).hexdigest()} for item in elements]
    (EVIDENCE / "formatted-frozen-elements.json").write_text(json.dumps(frozen, indent=2) + "\n")
    for item in frozen:
        if item["path"].endswith(".rs"):
            assert len(item["content"].splitlines()) < 600, item["path"]
        print("FROZEN_SOURCE " + json.dumps(item), flush=True)
    args = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps"]
    run("frozen-tests-compile", [*args, "--no-run"])
    for name in [
        "powershell_separators::unicode_horizontal_spaces_preserve_native_argument_counts",
        "powershell_plain_escapes::unicode_escape_spelling_does_not_lengthen_plain_literal_runs",
        "powershell_escaped_lines::backtick_only_lines_do_not_bridge_an_ordinary_newline_to_a_pipe",
    ]:
        result = run("red-" + name.split("::")[0], [*args, name, "--", "--exact", "--nocapture"], expected=101)
        assert "FAILED" in result
    for name in [
        "powershell_separators::protected_spaces_and_format_marks_stay_inside_native_arguments",
        "powershell_plain_escapes::unaffected_plain_word_measurements_keep_their_existing_contract",
        "powershell_escaped_lines::adjacent_and_explicit_continuation_pipes_remain_valid",
    ]:
        run("control-" + name.split("::")[0], [*args, name, "--", "--exact", "--nocapture"], expected=None)
    fixture = run("red-cli-fixture", ["cargo", "test", "--locked", "-p", "sloplint", "--test", "fixtures", "fixtures_match_expected", "--", "--exact", "--nocapture"], expected=101)
    assert "contiguous-log-literal__powershell" in fixture and "i.ps1" in fixture
finally:
    for name, content in saved.items():
        path = SOURCE / name
        if content is None:
            path.unlink()
        else:
            path.write_bytes(content)
assert source_identity() == initial
assert next(item for item in RESULTS if item["name"] == "native-new-cases")["exit"] == 0, "A frozen native hypothesis needs revision; inspect native-cases.json."
print("POWERSHELL_THREE_FINDINGS_BASELINE_COMPLETE " + json.dumps({"source": initial, "native_cases": len(native), "results": RESULTS}, sort_keys=True), flush=True)
