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
native_output = run("corrected-native-cases", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "native-oracle.ps1"), "-CasesPath", str(PROOF / "cases.json")])
native = [json.loads(line.removeprefix("NATIVE_CASE ")) for line in native_output.splitlines() if line.startswith("NATIVE_CASE ")]
assert len(native) == 243 and all(item["matches"] for item in native)
(EVIDENCE / "native-cases.json").write_text(json.dumps(native, indent=2, sort_keys=True) + "\n")
print("ALL_CORRECTED_NATIVE_CASES " + json.dumps(native), flush=True)
token_output = run("native-line-tokens", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "token-oracle.ps1"), "-CasesPath", str(PROOF / "token-cases.json")])
tokens = [json.loads(line.removeprefix("TOKEN_CASE ")) for line in token_output.splitlines() if line.startswith("TOKEN_CASE ")]
assert len(tokens) == 40
(EVIDENCE / "token-cases.json").write_text(json.dumps(tokens, indent=2, sort_keys=True) + "\n")
for record in tokens:
    print("ALL_TOKEN_CASE " + json.dumps(record), flush=True)
assert source_identity() == initial
print("POWERSHELL_NATIVE_BOUNDARY_OBSERVATIONS_COMPLETE", flush=True)
