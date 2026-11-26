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
MANIFEST = cast(dict[str, object], json.loads((PROOF / "trace-manifest.json").read_text()))
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



for name, digest in cast(dict[str, str], MANIFEST["proof_sha256"]).items():
    assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
binary = EVIDENCE / "tree-sitter"
binary.write_bytes(gzip.decompress(archive.read_bytes()))
binary.chmod(0o755)
run("parse-help", [str(binary), "parse", "--help"])
cases = json.loads((PROOF / "trace-cases.json").read_text())
root = EVIDENCE / "trace-cases"
root.mkdir()
records: list[dict[str, object]] = []
identities: list[dict[str, object]] = []

def parse_tree(text: str) -> str:
    start = text.index("(program")
    depth = 0
    for index in range(start, len(text)):
        depth += int(text[index] == "(") - int(text[index] == ")")
        if depth == 0:
            return text[start:index + 1]
    raise AssertionError("unbalanced parser tree")

for snapshot in MANIFEST["snapshots"]:
    source = WORKSPACE / snapshot["path"]
    identity = unchanged_identity(source, snapshot["head"], snapshot["tree"])
    identities.append(identity)
    library = EVIDENCE / ("trace-library-" + snapshot["id"])
    library.mkdir()
    ENVIRONMENT["TREE_SITTER_LIBDIR"] = str(library)
    for index, case in enumerate(cases):
        assert hashlib.sha256(case["source"].encode()).hexdigest() == case["source_sha256"]
        path = root / f"case-{index}.ps1"
        path.write_text(case["source"])
        name = snapshot["id"] + "-" + case["id"]
        expected = case["expected_exits"][snapshot["id"]]
        normal = run(name + "-normal", [str(binary), "parse", str(path)], expected=expected, cwd=source / "vendor/tree-sitter-powershell")
        traced = run(name + "-trace", [str(binary), "parse", "--debug=normal", str(path)], expected=expected, cwd=source / "vendor/tree-sitter-powershell")
        assert parse_tree(normal) == parse_tree(traced), name
        record = {"snapshot": snapshot["id"], "source": identity, "id": case["id"],
                  "expected_exit": expected, "normal_exit": RESULTS[-2]["exit"], "trace_exit": RESULTS[-1]["exit"],
                  "tree_sha256": hashlib.sha256(parse_tree(normal).encode()).hexdigest(),
                  "normal": normal, "trace": traced}
        records.append(record)
    assert unchanged_identity(source, snapshot["head"], snapshot["tree"]) == identity
payload = json.dumps({"manifest": MANIFEST, "sources": identities, "records": records, "results": RESULTS},
                     separators=(",", ":"), ensure_ascii=True).encode()
packed = gzip.compress(payload, mtime=0)
encoded = base64.b64encode(packed).decode()
(EVIDENCE / "trace-complete.json").write_bytes(payload)
(EVIDENCE / "trace-packed.json.gz").write_bytes(packed)
identity = {"payload_sha256": hashlib.sha256(payload).hexdigest(), "gzip_sha256": hashlib.sha256(packed).hexdigest(),
            "payload_bytes": len(payload), "gzip_bytes": len(packed), "base64_bytes": len(encoded)}
print("OWNERSHIP_TRACE_IDENTITY " + json.dumps(identity), flush=True)
for index in range(0, len(encoded), 24000):
    print("OWNERSHIP_TRACE_CHUNK " + json.dumps({"part": index // 24000, "data": encoded[index:index + 24000]}), flush=True)
print("OWNERSHIP_TRACE_COMPLETE records=" + str(len(records)), flush=True)
