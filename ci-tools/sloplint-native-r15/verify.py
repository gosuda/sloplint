from __future__ import annotations

import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import time

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "proof" / "ci-tools" / "sloplint-native-r15"
MODE = sys.argv[1]
assert MODE in ("arm64", "lint-grammar"), MODE
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / f"sloplint-native-r15-{MODE}"
EVIDENCE.mkdir(exist_ok=True)
EXPECTED_HEAD = "31bbd8486f485f20c93ab1511d039744339ec670"
EXPECTED_TREE = "589aff2ec97f6e6f8179453ff6882fe19b3b65b2"
EXPECTED_PARENT = "95f8d650552bd31fd5f5737bd9e457fec8ef8a55"
PATCH_PATH = Path("vendor/tree-sitter-powershell/sloplint-grammar.patch")
PATCH_SHA = "f7eb65fb4164e6e0ef75adbd89dde3560c781a3ec1b3c666e13812f923f70630"
EXITS: dict[str, int] = {}
DURATIONS: dict[str, float] = {}

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()

def run(name: str, args: list[str], *, cwd: Path | None = None, limit: int = 1800) -> str:
    started = time.monotonic()
    print(f"RUN {name}: {args!r}", flush=True)
    process = subprocess.Popen(args, cwd=cwd or SOURCE, env=os.environ.copy(), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
    timed_out = False
    try:
        output, _ = process.communicate(timeout=limit)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        output, _ = process.communicate()
    (EVIDENCE / f"{name}.log").write_text(output)
    EXITS[name] = process.returncode
    DURATIONS[name] = time.monotonic() - started
    (EVIDENCE / "results.json").write_text(json.dumps({"mode": MODE, "source": EXPECTED_HEAD, "exits": EXITS, "seconds": DURATIONS}, indent=2, sort_keys=True) + "\n")
    print(f"EXIT {name}: {process.returncode} timed_out={timed_out} seconds={DURATIONS[name]:.3f}\n{output[-24000:]}", flush=True)
    assert not timed_out, f"{name}: exceeded {limit} seconds; actual exit {process.returncode}"
    assert process.returncode == 0, f"{name}: actual exit {process.returncode}"
    return output

def identity(label: str) -> None:
    assert git("rev-parse", "HEAD") == EXPECTED_HEAD
    assert git("rev-parse", "HEAD^{tree}") == EXPECTED_TREE
    assert git("rev-parse", "HEAD^") == EXPECTED_PARENT
    assert git("write-tree") == EXPECTED_TREE
    assert not git("status", "--porcelain=v1", "--untracked-files=all"), "checkout is not clean"
    entries = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=SOURCE).split(b"\0")
    count = 0
    aggregate = hashlib.sha256()
    for entry in entries:
        if not entry:
            continue
        fields, raw_path = entry.split(b"\t", 1)
        mode, expected_blob, stage = fields.split()
        assert stage == b"0"
        path = SOURCE / os.fsdecode(raw_path)
        if mode == b"120000":
            data = os.fsencode(os.readlink(path))
            observed = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        else:
            assert mode in (b"100644", b"100755"), (mode, raw_path)
            digest = hashlib.sha1(f"blob {path.stat().st_size}\0".encode())
            with path.open("rb") as stream:
                while chunk := stream.read(1024 * 1024):
                    digest.update(chunk)
            observed = digest.hexdigest()
        assert observed == expected_blob.decode(), f"tracked source drift: {os.fsdecode(raw_path)}"
        aggregate.update(entry + b"\0")
        count += 1
    assert hashlib.sha256((SOURCE / PATCH_PATH).read_bytes()).hexdigest() == PATCH_SHA
    report = {"head": EXPECTED_HEAD, "tree": EXPECTED_TREE, "parent": EXPECTED_PARENT, "tracked_files_verified": count, "tracked_index_sha256": aggregate.hexdigest(), "patch_sha256": PATCH_SHA, "machine": platform.machine(), "mode": MODE}
    (EVIDENCE / f"identity-{label}.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print("SLOPLINT_SOURCE_IDENTITY " + json.dumps(report, sort_keys=True), flush=True)

def download(name: str, url: str, expected_sha: str) -> Path:
    target = EVIDENCE / name
    run(f"download-{name}", ["curl", "--fail", "--silent", "--show-error", "--location", "--retry", "3", "--connect-timeout", "15", "--max-time", "180", url, "--output", str(target)], limit=600)
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    assert digest == expected_sha, f"{name}: SHA-256 mismatch: {digest}"
    print(f"PINNED_DOWNLOAD {name} sha256={digest}", flush=True)
    return target

def directory_hashes(root: Path) -> dict[str, str]:
    return {path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(root.rglob("*")) if path.is_file()}

identity("before")
try:
    run("whitespace", ["git", "diff", "--check", EXPECTED_PARENT, EXPECTED_HEAD])
    version = run("rust-version", ["rustc", "+1.100.0", "-vV"], limit=120)
    assert "release: 1.100.0\n" in version
    if MODE == "arm64":
        assert platform.machine() == "aarch64", platform.machine()
        assert "host: aarch64-unknown-linux-gnu\n" in version
        target = "aarch64-unknown-linux-gnu"
        run("clippy-arm64", ["cargo", "+1.100.0", "clippy", "--workspace", "--all-targets", "--locked", "--target", target, "--", "-D", "warnings"])
        run("workspace-arm64", ["cargo", "+1.100.0", "test", "--workspace", "--locked", "--no-fail-fast", "--target", target])
        run("release-arm64", ["cargo", "+1.100.0", "build", "--release", "--locked", "--target", target])
    else:
        assert platform.machine() == "x86_64", platform.machine()
        assert "host: x86_64-unknown-linux-gnu\n" in version
        run("fmt", ["cargo", "+1.100.0", "fmt", "--all", "--check"], limit=180)
        run("grammar-manifest", ["bash", "scripts/grammars.sh", "check"], limit=180)
        run("release-script-selftests", ["bash", "scripts/selftest-release-scripts.sh"], limit=600)
        deny_name = "cargo-deny-0.20.2-x86_64-unknown-linux-musl"
        deny_archive = download("cargo-deny.tar.gz", f"https://github.com/EmbarkStudios/cargo-deny/releases/download/0.20.2/{deny_name}.tar.gz", "9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f")
        run("cargo-deny-extract", ["tar", "-xzf", str(deny_archive), "-C", str(EVIDENCE), f"{deny_name}/cargo-deny"], limit=120)
        deny = EVIDENCE / deny_name / "cargo-deny"
        os.environ["PATH"] = f"{deny.parent}{os.pathsep}{os.environ['PATH']}"
        deny_version = run("cargo-deny-version", [str(deny), "--version"], limit=120)
        assert deny_version.split()[1] == "0.20.2"
        run("cargo-deny", ["cargo", "+1.100.0", "deny", "check"], limit=600)
        archive = download("tree-sitter.gz", "https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz", "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d")
        tree_sitter = EVIDENCE / "tree-sitter"
        tree_sitter.write_bytes(gzip.decompress(archive.read_bytes()))
        tree_sitter.chmod(0o755)
        cli_version = run("tree-sitter-version", [str(tree_sitter), "--version"], limit=120)
        assert cli_version.split()[1] == "0.27.1"
        run("node-version", ["node", "--version"], limit=120)
        old_patch = (PROOF / "prior-grammar.patch").read_bytes()
        new_patch = (SOURCE / PATCH_PATH).read_bytes()
        assert hashlib.sha256(old_patch).hexdigest() == "c36dc9cf2c4da3bee0328335a4195433b5c153be50e4fa3c397aede55e6b820d"
        old_lines = old_patch.splitlines(keepends=True)
        assert sum(line == b" \n" for line in old_lines) == 75
        assert b"".join(b"\n" if line == b" \n" else line for line in old_lines) == new_patch
        print("PATCH_NORMALIZATION_VERIFIED removed_context_spaces=75", flush=True)
        os.environ["GRAMMARS_TS_BIN"] = str(tree_sitter)
        os.environ["GRAMMARS_SCRATCH"] = str(EVIDENCE / "upstream-verify")
        os.environ["XDG_CACHE_HOME"] = str(EVIDENCE / "tree-sitter-cache")
        os.environ["TREE_SITTER_LIBDIR"] = str(EVIDENCE / "tree-sitter-lib")
        Path(os.environ["XDG_CACHE_HOME"]).mkdir(exist_ok=True)
        Path(os.environ["TREE_SITTER_LIBDIR"]).mkdir(exist_ok=True)
        run("grammar-authoritative", ["bash", "scripts/grammars.sh", "verify", "tree-sitter-powershell"], limit=600)
        vendor = SOURCE / "vendor/tree-sitter-powershell"
        generated = EVIDENCE / "regenerated-powershell"
        shutil.copytree(vendor, generated)
        before = directory_hashes(generated)
        assert len(before) == 42, f"unexpected source grammar file count: {len(before)}"
        run("grammar-generate", [str(tree_sitter), "generate"], cwd=generated, limit=300)
        after = directory_hashes(generated)
        assert before == after, f"fresh generation drift: {sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))}"
        (EVIDENCE / "grammar-files.json").write_text(json.dumps(after, indent=2, sort_keys=True) + "\n")
        print(f"FRESH_GENERATION_IDENTICAL files={len(after)}", flush=True)
        run("grammar-corpus", [str(tree_sitter), "test"], cwd=generated, limit=300)
        assert directory_hashes(generated) == after, "corpus command changed the copied source"
finally:
    identity("after")
print("SLOPLINT_GATES_COMPLETE " + json.dumps({"mode": MODE, "source": EXPECTED_HEAD, "exits": EXITS}, sort_keys=True), flush=True)
