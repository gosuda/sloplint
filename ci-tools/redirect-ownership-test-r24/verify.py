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
    CARGO_TARGET_DIR=str(EVIDENCE / "target"),
    CARGO_BUILD_TARGET="x86_64-unknown-linux-gnu",
    SLOPLINT_REQUIRE_FS_CAPS="1",
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


def main() -> None:
    expected_files = cast(dict[str, str], MANIFEST["proof_files_sha256"])
    for name, digest in expected_files.items():
        assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
    baseline_head = str(MANIFEST["baseline"])
    candidate_head = str(MANIFEST["candidate"])
    baseline_tree = str(MANIFEST["baseline_tree"])
    candidate_tree = str(MANIFEST["candidate_tree"])
    identity("baseline-before", BASELINE, baseline_head, baseline_tree, set())
    identity("candidate-before", SOURCE, candidate_head, candidate_tree, set())
    assert output(["git", "diff", "--name-only", str(MANIFEST["published_parent"]), "HEAD"], SOURCE).splitlines() == [str(TEST_PATH)]
    original = (PROOF / "original-test.rs").read_bytes()
    candidate = (PROOF / "candidate-test.rs").read_bytes()
    assert (SOURCE / TEST_PATH).read_bytes() == candidate
    frozen = json.loads((PROOF / "frozen-captures.json").read_text())
    assert len(frozen["cases"]) == 3
    print("FROZEN_HISTORICAL_CAPTURES " + json.dumps(frozen, sort_keys=True), flush=True)
    run("install-rust", ["rustup", "toolchain", "install", "1.100.0", "--profile", "minimal", "--component", "rustfmt,clippy"])
    assert output(["rustc", "--version"], SOURCE).startswith("rustc 1.100.0 ")

    module = BASELINE / MODULE_PATH
    module_original = module.read_bytes()
    test = BASELINE / TEST_PATH
    assert not test.exists()
    assert b"mod powershell_separator_ownership;" not in module_original
    try:
        module.write_bytes(module_original + MODULE_SUFFIX.encode())
        test.write_bytes(original)
        overlay = {str(MODULE_PATH), str(TEST_PATH)}
        identity("baseline-original-test", BASELINE, baseline_head, None, overlay)
        weak = run("baseline-original-comment-only", focused(), BASELINE)
        check_one_test(weak, True)
        test.write_bytes(candidate)
        identity("baseline-strengthened-test", BASELINE, baseline_head, None, overlay)
        run("baseline-strengthened-compile", focused(["--no-run"]), BASELINE)
        red = run("baseline-strengthened-test", focused(), BASELINE, expected=101)
        check_one_test(red, False)
        assert "assertion \x60left == right\x60 failed" in red
        assert "> <# note #> -output.log" in red
    finally:
        module.write_bytes(module_original)
        if test.exists():
            test.unlink()
        identity("baseline-restored", BASELINE, baseline_head, baseline_tree, set())

    green = run("candidate-exact-test", focused())
    check_one_test(green, True)
    focused_all = run(
        "candidate-powershell-regressions",
        ["cargo", "test", "--locked", "-p", "sloplint-syntax", "--test", "grammar_gaps", "powershell", "--", "--nocapture"],
    )
    assert re.search(r"test result: ok\. 116 passed; 0 failed;", focused_all)
    run("candidate-fmt", ["cargo", "fmt", "--all", "--check"])
    run("candidate-clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
    run("candidate-workspace", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"])
    run("candidate-release", ["cargo", "build", "--release", "--locked"])
    run("candidate-self-lint", [str(EVIDENCE / "target/x86_64-unknown-linux-gnu/release/sloplint"), "check", "crates", "--format", "agent", "--strictness", "standard", "--mode", "threshold", "--fail-on", "minor"])
    run("candidate-whitespace", ["git", "diff", "--check"])
    identity("candidate-after", SOURCE, candidate_head, candidate_tree, set())
    save_report()
    report = (EVIDENCE / "results.json").read_bytes()
    print("FINAL_GATE_RECEIPT " + report.decode(), flush=True)
    print("FINAL_GATE_RECEIPT_SHA256 " + hashlib.sha256(report).hexdigest(), flush=True)


if __name__ == "__main__":
    main()
