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
    Candidate(25, "d7a01482cc11ab2408b9b8fff9a40ab8c597a525", "d95199a70ebbfbced46b47a1c3b5798775a30ede65919f41d1b33f804fc97027"),
    Candidate(27, "abd498274ad77511b584229c12102493946a24d2", "20ecb00e258e4b9b98d478a82a9e266e23cdce10ec18e84d5374d0c3664674ff"),
    Candidate(28, "514a5720a788747b1fb1f7c1281a311013e40fd4", "8de4b5916018b79fb8e46e92549d40e8fc94969397104be93778dfdebcd22919"),
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
    if candidate.number == 28 and result["build"] == 0:
        module = SOURCE / "bin/crush/lib/catalog_cache.ml"
        repaired = module.read_bytes()
        test_source = (SOURCE / "test/crush/test_cli.ml").read_text().split("let cases =", 1)[1]
        names = re.findall(r'Test_tools_test_support.case\s+"([^"]+)"', test_source)
        case_index = names.index("trust works without a cache directory")
        args = ["dune", "exec", "--profile", "release", "test/crush/test_crush.exe", "--", "test", "cli", str(case_index), "--verbose"]
        try:
            module.write_bytes(subprocess.check_output(["git", "show", f"{candidate.head}:bin/crush/lib/catalog_cache.ml"], cwd=SOURCE))
            assert run(["dune", "build", "--profile", "release", "bin/crush/main.exe"], folder / "cache-path-red-build.log") == 0
            code = run(args, folder / "cache-path-red.log")
            red_log = (folder / "cache-path-red.log").read_text(errors="replace")
            result["cache_path_red"] = code != 0 and "trust succeeds without a cache path" in red_log
        finally:
            module.write_bytes(repaired)
        assert run(["dune", "build", "--profile", "release", "bin/crush/main.exe"], folder / "cache-path-green-build.log") == 0
        result["cache_path_green"] = run(args, folder / "cache-path-green.log")
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
    final = results["28"]
    passed = passed and final.get("cache_path_red") is True and final.get("cache_path_green") == 0
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
