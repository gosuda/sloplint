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
    Candidate(20, "0e6dc5a27a7d269bc987e520ab70a6c1f03fd9db", "ff6995809e4bf80073035e8a424a8706aacef2d1bf9e89052560fb67488472f2"),
    Candidate(25, "7ff215abf9c7bddb3a75ba172428cc8bbed824f5", "f77cf0363c1938a1cce2126803b28dd8943dff0ff0d4ae2fd0408970ede81a77"),
    Candidate(26, "3f09ab25b4d102778ab02a7d1bb86a7a2c1e6601", "d8b5c6454261166f212d61d510534002b59b8908a46f0faacaef0afa88c1445b"),
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


def count_screen_visits(candidate: Candidate, folder: Path) -> bool:
    module = SOURCE / "bin/crush/lib/shell_screen.ml"
    repaired = module.read_text()
    old = subprocess.check_output(["git", "show", f"{candidate.head}:bin/crush/lib/shell_screen.ml"], cwd=SOURCE, text=True)
    outputs: dict[str, int] = {}
    for phase, source in (("red", old), ("green", repaired)):
        work = folder / f"screen-{phase}"
        work.mkdir()
        prefix = 'and analyze_program sink ~depth program args =\n'
        assert source.count(prefix) == 1
        instrumented = 'let analysis_visits = ref 0\n' + source.replace(prefix, prefix + '  incr analysis_visits;\n')
        (work / "shell_screen.ml").write_text(instrumented)
        (work / "shell_screen.mli").write_text((SOURCE / "bin/crush/lib/shell_screen.mli").read_text() + '\nval analysis_visits : int ref\n')
        (work / "driver.ml").write_text('''let () =
  let depth = 12 in
  let command = String.concat "" (List.init depth (fun _ -> "fd --color never -X ")) ^ "printf ready" in
  (match Shell_screen.screen Shell_screen.Posix command with
  | Shell_screen.Review _ -> ()
  | _ -> failwith "nested unknown-option actions require review");
  let visits = !(Shell_screen.analysis_visits) in
  Printf.printf "depth=%d visits=%d expected=%d\\n" depth visits (depth + 1);
  if visits <> depth + 1 then exit 1
''')
        subprocess.run(["ocamlc", "-strict-sequence", "-strict-formats", "-short-paths", "-principal", "-o", "check", "shell_screen.mli", "shell_screen.ml", "driver.ml"], cwd=work, check=True)
        outputs[phase] = run([str(work / "check")], folder / f"screen-visits-{phase}.log")
    return outputs == {"red": 1, "green": 0}


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
        result["screen_visits"] = count_screen_visits(candidate, folder)
    if candidate.number == 25 and result["build"] == 0:
        module = SOURCE / "bin/crush/lib/hooks.ml"
        repaired = module.read_bytes()
        test_source = (SOURCE / "test/crush/test_hooks.ml").read_text().split("let cases =", 1)[1]
        names = re.findall(r'Test_tools_test_support.case\s+"([^"]+)"', test_source)
        case_index = names.index("successful hook preserves detached worker")
        args = ["dune", "exec", "--profile", "release", "test/crush/test_crush.exe", "--", "test", "hooks", str(case_index), "--verbose"]
        try:
            module.write_bytes(subprocess.check_output(["git", "show", f"{candidate.head}:bin/crush/lib/hooks.ml"], cwd=SOURCE))
            code = run(args, folder / "detached-worker-red.log")
            red_log = (folder / "detached-worker-red.log").read_text(errors="replace")
            result["detached_worker_red"] = code != 0 and "Lwt_unix.Timeout" in red_log
        finally:
            module.write_bytes(repaired)
        result["detached_worker_green"] = run(args, folder / "detached-worker-green.log")
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
    passed = passed and results["20"].get("screen_visits") is True
    final = results["25"]
    passed = passed and final.get("detached_worker_red") is True and final.get("detached_worker_green") == 0
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
