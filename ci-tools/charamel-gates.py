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
    Candidate(25, "8a03c878c30036add4e5b20e87f24c35c907bfd3", "902e828c784bd4f1c4610284bd1f781a47bf51c88d3d06bfdf3fd9fd530623ff"),
    Candidate(28, "38bbbb8dc176fe5013b9b7f7d281533d39bafa2d", "baab8a3ea9490104cc6f18d66ef6643bbf082152e2e4e62e7411198acac89e13"),
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
    return ["dune", "exec", "--profile", "release", "test/crush/test_crush.exe", "--", "test", group, str(index), "--verbose"]


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
    if candidate.number == 25 and result["build"] == 0:
        result["pending_approval"] = check_regression(candidate, folder, "bin/crush/lib/agent.ml", "agent", "stopped turn does not await sibling approval", "pending-approval", "Lwt_unix.Timeout")
        result["explicit_cancel"] = check_regression(candidate, folder, "bin/crush/lib/agent.ml", "agent", "explicit cancel joins hook cleanup", "explicit-cancel", "Canceled")
    if candidate.number == 28 and result["build"] == 0:
        result["cache_destination"] = check_regression(candidate, folder, "bin/crush/lib/models.ml", "cli", "update-providers needs a cache directory", "cache-destination", "refresh reports the unavailable cache path", rebuild_cli=True)
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
    passed = passed and results["25"].get("pending_approval") is True
    passed = passed and results["25"].get("explicit_cancel") is True
    passed = passed and results["28"].get("cache_destination") is True
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
