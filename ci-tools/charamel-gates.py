from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class Candidate:
    number: int
    head: str
    patch_sha256: str | None = None


CANDIDATES = (
    Candidate(21, "7d2975c753810d99746a88c8ba6b43be921c16fc"),
    Candidate(25, "28fb308e469cf687b6ed7b553a23fd266c38977c", "86b30bf59a7036663698b0d63a59f2a9bfa42f4fb896d86432a789d68707be19"),
)
BASE = "84e5ff153f781c7e2d02804a5c24997858f618ab"
WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "candidate"
PROBE = WORKSPACE / "probe"
EVIDENCE = WORKSPACE / "evidence"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def run(args: list[str], output: Path, timeout: int = 1200) -> int:
    print(f"Running {args!r}; full log: {output}", flush=True)
    with output.open("w") as stream:
        try:
            code = subprocess.run(args, cwd=SOURCE, stdout=stream, stderr=subprocess.STDOUT, check=False, timeout=timeout).returncode
        except subprocess.TimeoutExpired:
            stream.write(f"\nCommand exceeded {timeout} seconds.\n")
            code = 124
    print(f"Exit {code}: {output.name}", flush=True)
    if code:
        print(output.read_text(errors="replace")[-12000:], flush=True)
    return code


def save_candidate(candidate: Candidate, folder: Path) -> str:
    patch = subprocess.check_output(["git", "diff", "--binary", candidate.head], cwd=SOURCE)
    (folder / "candidate.patch").write_bytes(patch)
    changed = git("diff", "--name-only", candidate.head).splitlines()
    hashes: dict[str, str] = {}
    for relative in changed:
        source = SOURCE / relative
        target = folder / "source" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        hashes[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
    git("add", "--all")
    tree = git("write-tree")
    (folder / "identity.json").write_text(json.dumps({"candidate": asdict(candidate), "tree": tree, "patch_sha256": hashlib.sha256(patch).hexdigest(), "files": hashes}, indent=2) + "\n")
    return tree


def case_command(group: str, name: str) -> list[str]:
    source = (SOURCE / f"test/crush/test_{group}.ml").read_text().split("let cases =", 1)[1]
    names = re.findall(r'Test_tools_test_support.case\s+"([^"]+)"', source)
    index = names.index(name)
    return ["dune", "exec", "--profile", "release", "test/crush/test_crush.exe", "--", "test", group.replace("_", "-"), str(index), "--verbose"]


def check_regression(candidate: Candidate, folder: Path, module: str, group: str, name: str, label: str, failure: str, *, rebuild_cli: bool = False) -> bool:
    path = SOURCE / module
    repaired = path.read_bytes()
    args = case_command(group, name)
    rebuild = ["dune", "build", "--profile", "release", "bin/crush/main.exe"]
    try:
        path.write_bytes(subprocess.check_output(["git", "show", f"{candidate.head}:{module}"], cwd=SOURCE))
        if rebuild_cli and run(rebuild, folder / f"{label}-red-cli-build.log") != 0:
            return False
        red = run(args, folder / f"{label}-red.log")
        log = (folder / f"{label}-red.log").read_text(errors="replace")
        reproduced = red != 0 and failure in log
    finally:
        path.write_bytes(repaired)
    if rebuild_cli and run(rebuild, folder / f"{label}-green-cli-build.log") != 0:
        return False
    green = run(args, folder / f"{label}-green.log")
    return reproduced and green == 0


def check(candidate: Candidate) -> dict[str, object]:
    folder = EVIDENCE / f"pr{candidate.number}"
    folder.mkdir(parents=True, exist_ok=True)
    git("fetch", "--no-tags", "--depth=1", "origin", candidate.head)
    git("checkout", "--detach", candidate.head)
    assert git("rev-parse", "HEAD") == candidate.head
    if candidate.patch_sha256 is not None:
        patch = PROBE / "ci-tools" / f"charamel{candidate.number}.patch"
        assert hashlib.sha256(patch.read_bytes()).hexdigest() == candidate.patch_sha256
        git("apply", "--index", "--whitespace=error", str(patch))
    paths = set(git("diff", "--name-only", BASE, candidate.head).splitlines())
    paths.update(git("diff", "--name-only", candidate.head).splitlines())
    sources = sorted(path for path in paths if Path(path).suffix in {".ml", ".mli"} and (SOURCE / path).is_file())
    result: dict[str, object] = {"head": candidate.head}
    result["format_sources"] = run(["ocamlformat", "--inplace", *sources], folder / "format-sources.log", 120) if sources else 0
    result["tree"] = save_candidate(candidate, folder)
    result["fmt"] = run(["dune", "build", "@fmt"], folder / "fmt.log")
    result["build"] = run(["dune", "build", "--profile", "release"], folder / "build.log")
    result["tests"] = run(["dune", "runtest", "--profile", "release"], folder / "tests.log")
    if candidate.number == 20 and result["build"] == 0:
        result["fd_operands"] = check_regression(candidate, folder, "bin/crush/lib/tools_shell.ml", "tools_shell", "conservative shell classifier", "fd-operands", "fd -- -X has only literal data operands")
    if candidate.number == 25 and result["build"] == 0:
        path = SOURCE / "bin/crush/lib/hooks.ml"
        repaired = path.read_text()
        original = "if code = 0 then completed := true;"
        assert repaired.count(original) == 1
        args = case_command("hooks", "failing hook kills detached child")
        try:
            path.write_text(repaired.replace(original, "completed := true;", 1))
            red = run(args, folder / "detached-failure-red.log")
            reproduced = red != 0 and "Lwt_unix.Timeout" in (folder / "detached-failure-red.log").read_text()
        finally:
            path.write_text(repaired)
        green = run(args, folder / "detached-failure-green.log")
        result["detached_failure"] = reproduced and green == 0
        path = SOURCE / "test/crush/test_hooks.ml"
        repaired = path.read_text()
        start = repaired.index("let failing_hook_kills_detached_child () =")
        end = repaired.index("\nlet cases =", start)
        fixture = repaired[start:end]
        assert fixture.count("exit 2") == 1
        assert fixture.count("hook ~event:Config.Pre_tool ~command ()") == 1
        blocked = fixture.replace("exit 2", "read timeout < worker-release", 1).replace("hook ~event:Config.Pre_tool ~command ()", "hook ~event:Config.Pre_tool ~command ~timeout_s:1 ()", 1)
        try:
            path.write_text(repaired[:start] + blocked + repaired[end:])
            red = run(args, folder / "exit-two-timeout-red.log")
            reproduced = red != 0 and "nonzero hook did not report its exit-two denial" in (folder / "exit-two-timeout-red.log").read_text()
        finally:
            path.write_text(repaired)
        green = run(args, folder / "exit-two-timeout-green.log")
        result["exit_two_timeout"] = reproduced and green == 0
    git("diff", "--exit-code")
    assert git("write-tree") == result["tree"]
    (folder / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    git("restore", "--source=HEAD", "--staged", "--worktree", "--", ".")
    return result


def main() -> int:
    EVIDENCE.mkdir(exist_ok=True)
    git("fetch", "--no-tags", "--depth=1", "origin", BASE)
    results: dict[str, dict[str, object]] = {}
    for candidate in CANDIDATES:
        results[str(candidate.number)] = check(candidate)
        (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    passed = all(all(result[name] == 0 for name in ("format_sources", "fmt", "build", "tests")) for result in results.values())
    required = {"20": ("fd_operands",), "25": ("exit_two_timeout", "detached_failure")}
    passed = passed and all(all(result.get(name) is True for name in required.get(number, ())) for number, result in results.items())
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
