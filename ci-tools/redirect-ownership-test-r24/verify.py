from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
import zipfile
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
PROOF = WORKSPACE / "proof/ci-tools/redirect-ownership-test-r24"
SOURCE = WORKSPACE / "source"
BASELINE = WORKSPACE / "baseline"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "redirect-ownership-test-r24"
EVIDENCE.mkdir(exist_ok=True)
MANIFEST = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
ENVIRONMENT = dict(os.environ)
ENVIRONMENT.update(
    RUSTUP_TOOLCHAIN="1.100.0",
    CARGO_INCREMENTAL="0",
    CARGO_BUILD_JOBS="8",
    CARGO_TERM_COLOR="never",
    CARGO_PROFILE_DEV_DEBUG="line-tables-only",
    CARGO_PROFILE_TEST_DEBUG="line-tables-only",
    CARGO_TARGET_DIR=str(EVIDENCE / "candidate-fresh-target"),
    CARGO_BUILD_TARGET="x86_64-unknown-linux-gnu",
    SLOPLINT_REQUIRE_FS_CAPS="1",
    RUSTC_WRAPPER="",
    RUSTC_WORKSPACE_WRAPPER="",
    CARGO_BUILD_RUSTC_WRAPPER="",
    CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER="",
    CCACHE_DISABLE="1",
    SCCACHE_DISABLE="1",
    CC="/usr/bin/cc",
    CXX="/usr/bin/c++",
    CC_x86_64_unknown_linux_gnu="/usr/bin/cc",
    CXX_x86_64_unknown_linux_gnu="/usr/bin/c++",
)
TEST_PATH = Path("crates/sloplint-syntax/tests/grammar_gaps/powershell_separator_ownership.rs")
MODULE_PATH = Path("crates/sloplint-syntax/tests/grammar_gaps.rs")
TEST_NAME = "powershell_separator_ownership::comments_before_attached_redirect_words_stay_outside_separators"
MODULE_SUFFIX = '\n#[path = "grammar_gaps/powershell_separator_ownership.rs"]\nmod powershell_separator_ownership;\n'


@dataclass(frozen=True)
class Result:
    name: str
    args: tuple[str, ...]
    cwd: str
    exit: int
    expected: int
    timed_out: bool
    seconds: float
    log_sha256: str


RESULTS: list[Result] = []
IDENTITIES: list[dict[str, object]] = []


def save_report() -> None:
    report = {"manifest": MANIFEST, "results": [asdict(result) for result in RESULTS], "identities": IDENTITIES}
    (EVIDENCE / "results.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")


def output(args: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=cwd, env=ENVIRONMENT if env is None else env, text=True).strip()


def run(name: str, args: list[str], cwd: Path = SOURCE, expected: int = 0, timeout: int = 2400) -> str:
    started = time.monotonic()
    process = subprocess.Popen(
        args, cwd=cwd, env=ENVIRONMENT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    timed_out = False
    try:
        content, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        content, _ = process.communicate()
    (EVIDENCE / f"{name}.log").write_text(content)
    result = Result(
        name, tuple(args), str(cwd), process.returncode, expected, timed_out,
        time.monotonic() - started, hashlib.sha256(content.encode()).hexdigest(),
    )
    RESULTS.append(result)
    save_report()
    print("ACTUAL_EXIT " + json.dumps(asdict(result), sort_keys=True), flush=True)
    print(content[-20000:], flush=True)
    assert not timed_out and process.returncode == expected, result
    return content


def identity(label: str, cwd: Path, head: str, tree: str | None, changed: set[str]) -> dict[str, object]:
    assert output(["git", "rev-parse", "HEAD"], cwd) == head
    index_env = dict(ENVIRONMENT)
    index_env["GIT_INDEX_FILE"] = str(EVIDENCE / f"{label}.index")
    output(["git", "read-tree", "HEAD"], cwd, index_env)
    output(["git", "add", "-A"], cwd, index_env)
    actual_tree = output(["git", "write-tree"], cwd, index_env)
    if tree is not None:
        assert actual_tree == tree, (label, actual_tree, tree)
    index = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=cwd, env=index_env)
    count = 0
    for entry in index.split(b"\0"):
        if not entry:
            continue
        metadata, relative = entry.split(b"\t", 1)
        mode, expected, stage = metadata.split()
        assert stage == b"0"
        path = cwd / os.fsdecode(relative)
        if mode == b"120000":
            content = os.fsencode(os.readlink(path))
        else:
            assert mode in (b"100644", b"100755"), mode
            content = path.read_bytes()
            assert bool(path.stat().st_mode & 0o111) == (mode == b"100755")
        digest = hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest()
        assert digest == expected.decode(), str(path)
        count += 1
    actual_changed = set(output(["git", "diff", "--name-only"], cwd).splitlines())
    actual_changed.update(output(["git", "ls-files", "--others", "--exclude-standard"], cwd).splitlines())
    assert actual_changed == changed, (label, sorted(actual_changed), sorted(changed))
    result = {
        "label": label, "head": head, "tree": actual_tree, "tracked_blobs": count,
        "index_sha256": hashlib.sha256(index).hexdigest(), "overlay_paths": sorted(changed),
    }
    IDENTITIES.append(result)
    save_report()
    print("EXACT_SOURCE_IDENTITY " + json.dumps(result, sort_keys=True), flush=True)
    return result


def focused(args: list[str] | None = None) -> list[str]:
    command = ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", TEST_NAME]
    return command + (["--", "--exact", "--nocapture", "--test-threads=1"] if args is None else args)


def check_one_test(content: str, expected: bool) -> None:
    result = "ok" if expected else "FAILED"
    counts = "1 passed; 0 failed" if expected else "0 passed; 1 failed"
    assert "running 1 test" in content
    assert f"test result: {result}. {counts};" in content
    assert TEST_NAME in content


def verified_baseline() -> None:
    receipt = cast(dict[str, object], json.loads((PROOF / "failed-probe.json").read_text()))
    archives = [path for path in (EVIDENCE / "baseline-archive").rglob("*") if path.is_file() and zipfile.is_zipfile(path)]
    assert len(archives) == 1, [str(path) for path in archives]
    archive = archives[0]
    actual_zip = hashlib.sha256(archive.read_bytes()).hexdigest()
    assert actual_zip == receipt["zip_sha256"]
    with zipfile.ZipFile(archive) as handle:
        def read(name: str) -> bytes:
            matches = [member for member in handle.namelist() if Path(member).name == name]
            assert len(matches) == 1, (name, matches)
            return handle.read(matches[0])
        report_bytes = read("results.json")
        report = json.loads(report_bytes)
        assert report["manifest"]["candidate"] == MANIFEST["candidate"]
        assert report["manifest"]["candidate_tree"] == MANIFEST["candidate_tree"]
        assert report["manifest"]["candidate_test_sha256"] == MANIFEST["candidate_test_sha256"]
        assert report["results"] == receipt["results"]
        assert report["identities"] == receipt["identities"]
        assert [(result["name"], result["exit"]) for result in report["results"]] == [
            ("install-rust", 0),
            ("baseline-original-comment-only", 0),
            ("baseline-strengthened-compile", 0),
            ("baseline-strengthened-test", 101),
            ("candidate-exact-test", 101),
        ]
        for result in report["results"]:
            content = read(result["name"] + ".log")
            assert hashlib.sha256(content).hexdigest() == result["log_sha256"], result["name"]
        check_one_test(read("baseline-original-comment-only.log").decode(), True)
        check_one_test(read("baseline-strengthened-test.log").decode(), False)
        assert report["identities"][-1] == {
            "label": "baseline-restored",
            "head": MANIFEST["baseline"],
            "tree": MANIFEST["baseline_tree"],
            "tracked_blobs": 4906,
            "index_sha256": "149f82ee0bcf0b5a4fd3505b465d8e324df6b09ce48cf4b2221bd66308b824a8",
            "overlay_paths": [],
        }
    verified = {
        "historical_run": receipt["run"], "historical_conclusion": receipt["conclusion"],
        "artifact": receipt["artifact"], "zip_sha256": actual_zip,
        "report_sha256": hashlib.sha256(report_bytes).hexdigest(),
        "baseline_weak_exit": 0, "baseline_compile_exit": 0, "baseline_strong_exit": 101,
        "historical_candidate_exit": 101,
    }
    (EVIDENCE / "verified-baseline.json").write_text(json.dumps(verified, indent=2, sort_keys=True) + "\n")
    print("AUTHENTICATED_HISTORICAL_RECEIPT " + json.dumps(verified, sort_keys=True), flush=True)


def main() -> None:
    expected_files = cast(dict[str, str], MANIFEST["proof_files_sha256"])
    for name, digest in expected_files.items():
        assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
    candidate_head = str(MANIFEST["candidate"])
    candidate_tree = str(MANIFEST["candidate_tree"])
    identity("candidate-before", SOURCE, candidate_head, candidate_tree, set())
    assert output(["git", "diff", "--name-only", str(MANIFEST["published_parent"]), "HEAD"], SOURCE).splitlines() == [str(TEST_PATH)]
    assert (SOURCE / TEST_PATH).read_bytes() == (PROOF / "candidate-test.rs").read_bytes()
    verified_baseline()
    target = Path(ENVIRONMENT["CARGO_TARGET_DIR"])
    assert not target.exists(), "Candidate target must be initially absent"
    run("install-rust", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
    ENVIRONMENT["RUSTC"] = output(["rustup", "which", "--toolchain", "1.100.0", "rustc"], SOURCE)
    compiler = {
        key: ENVIRONMENT[key]
        for key in (
            "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "CCACHE_DISABLE", "SCCACHE_DISABLE", "CC", "CXX",
            "CC_x86_64_unknown_linux_gnu", "CXX_x86_64_unknown_linux_gnu",
            "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET",
        )
    }
    print("EXPLICIT_COMPILER_SELECTION " + json.dumps(compiler, sort_keys=True), flush=True)
    (EVIDENCE / "compiler-selection.json").write_text(json.dumps(compiler, indent=2, sort_keys=True) + "\n")
    run("rustc-identity", [ENVIRONMENT["RUSTC"], "-vV"])
    try:
        green = run("candidate-exact-test", focused())
        check_one_test(green, True)
        expected_compile = "Compiling tree-sitter-powershell v0.26.6 (" + str(SOURCE / "vendor/tree-sitter-powershell") + ")"
        assert expected_compile in green, "Fresh candidate grammar dependency compilation was not observed"
        focused_all = run(
            "candidate-powershell-regressions",
            ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "powershell", "--", "--nocapture"],
        )
        assert re.search(r"test result: ok\. 116 passed; 0 failed;", focused_all)
        run("candidate-fmt", ["cargo", "fmt", "--all", "--check"])
        run("candidate-clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
        run("candidate-workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"])
        run("candidate-release", ["cargo", "build", "--release", "--locked"])
        run("candidate-self-lint", [str(target / "x86_64-unknown-linux-gnu/release/sloplint"), "check", "crates", "--format", "agent", "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
        run("candidate-whitespace", ["git", "diff", "--check"])
    finally:
        identity("candidate-after", SOURCE, candidate_head, candidate_tree, set())
        save_report()
    report = (EVIDENCE / "results.json").read_bytes()
    print("FINAL_GATE_RECEIPT " + report.decode(), flush=True)
    print("FINAL_GATE_RECEIPT_SHA256 " + hashlib.sha256(report).hexdigest(), flush=True)


if __name__ == "__main__":
    main()
