"""Verify exact public Charamel parser source with authentic native and Dune gates."""
from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from typing import Any
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent
WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
OUT = WORKSPACE / "evidence"
MANIFEST: dict[str, Any] = json.loads((ROOT / "gates.json").read_text("utf-8"))


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def save(name: str, value: Any) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")


class ArtifactRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(
        self, request: urllib.request.Request, response: Any, code: int,
        message: str, headers: Any, new_url: str,
    ) -> urllib.request.Request | None:
        redirected = super().redirect_request(request, response, code, message, headers, new_url)
        if redirected is not None:
            assert urllib.parse.urlparse(new_url).scheme == "https"
            redirected.remove_header("Authorization")
        return redirected


def authenticated_archive(archive: dict[str, Any]) -> dict[str, Any]:
    request = urllib.request.Request(
        f'https://api.github.com/repos/gosuda/sloplint/actions/artifacts/{archive["id"]}/zip',
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/vnd.github+json"},
    )
    opener = urllib.request.build_opener(ArtifactRedirect())
    with opener.open(request, timeout=90) as response:
        data = response.read()
    assert sha256(data) == archive["sha256"], archive["id"]
    with zipfile.ZipFile(io.BytesIO(data)) as contents:
        names = [name for name in contents.namelist() if name.endswith(archive["report"])]
        assert len(names) == 1
        report: dict[str, Any] = json.loads(contents.read(names[0]))
    assert report["producer"] == archive["producer"]
    save("authenticated-" + archive["report"], report)
    return report


def identity() -> dict[str, Any]:
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("rev-parse", "HEAD^{tree}") == MANIFEST["original_tree"]
    assert git("write-tree") == MANIFEST["candidate_tree"]
    assert git("diff", "--exit-code") == ""
    assert git("ls-files", "--others", "--exclude-standard") == ""
    changed = git("diff", "--cached", "--name-only").splitlines()
    assert changed == sorted(MANIFEST["source_sha256"])
    for path, expected in MANIFEST["source_sha256"].items():
        assert sha256((SOURCE / path).read_bytes()) == expected, path
    return {
        "head": MANIFEST["head"], "original_tree": MANIFEST["original_tree"],
        "candidate_tree": MANIFEST["candidate_tree"], "source_sha256": MANIFEST["source_sha256"],
        "changed_paths": changed, "unstaged_clean": True, "unexpected_untracked": [],
        "index_sha256": sha256(subprocess.check_output(["git", "-C", str(SOURCE), "ls-files", "-s"])),
    }


def preflight() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("rev-parse", "HEAD^{tree}") == MANIFEST["original_tree"]
    assert git("status", "--porcelain") == ""
    save("initial-source.json", {"head": MANIFEST["head"], "tree": MANIFEST["original_tree"],
                                "producer": os.environ["GITHUB_SHA"]})
    preparation = authenticated_archive(MANIFEST["preparation"])
    assert preparation["head"] == MANIFEST["head"]
    assert preparation["original_tree"] == MANIFEST["original_tree"]
    assert preparation["formatted_tree"] == MANIFEST["candidate_tree"]
    assert preparation["candidate_all_pass"] is True
    assert preparation["candidate_cases"] == 97
    assert preparation["source_restored"] is True
    assert preparation["baseline_actual_failures"] >= 8
    patch = ROOT / "candidate.patch"
    assert sha256(patch.read_bytes()) == MANIFEST["patch_sha256"]
    applied = subprocess.run(["git", "apply", "--index", str(patch)], cwd=SOURCE, capture_output=True, check=False)
    save("apply.json", {"actual_exit": applied.returncode,
                       "stdout": applied.stdout.decode("utf-8", errors="backslashreplace"),
                       "stderr": applied.stderr.decode("utf-8", errors="backslashreplace")})
    assert applied.returncode == 0
    current = identity()
    save("exact-candidate.json", current)
    print("EXACT_CHARAMEL_CANDIDATE " + json.dumps(current), flush=True)
    return 0


def native_windows() -> int:
    assert sys.platform == "win32"
    identity()
    expected: dict[str, Any] = json.loads((ROOT / "native-observed.json").read_text("utf-8"))
    assert sha256((ROOT / "native-observed.json").read_bytes()) == MANIFEST["native_observed_sha256"]
    comspec = os.environ["COMSPEC"]
    records: list[dict[str, Any]] = []
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="charamel32-native-") as temporary:
        cwd = Path(temporary)
        (cwd / "do").mkdir()
        (cwd / "do" / "note.txt").write_text("owned\n", encoding="ascii")
        (cwd / "notes.txt").write_text("alpha one\nbeta two\n", encoding="ascii")
        (cwd / "notes space.txt").write_text("gamma three\n", encoding="ascii")
        env = dict(os.environ)
        env["FILES"] = "alpha"
        env["PROGRAM"] = "echo"
        assert len(expected["records"]) == 37
        for case in expected["records"]:
            started = time.monotonic()
            result = subprocess.run(
                f'"{comspec}" /d /c {case["command"]}', executable=comspec,
                cwd=cwd, env=env, stdin=subprocess.DEVNULL, capture_output=True,
                timeout=10, check=False,
            )
            stdout = result.stdout.decode("utf-8", errors="backslashreplace")
            stderr = result.stderr.decode("utf-8", errors="backslashreplace")
            normalized_stdout = stdout.replace(str(cwd), "{cwd}")
            normalized_stderr = stderr.replace(str(cwd), "{cwd}")
            accepted = (result.returncode == case["actual_exit"]
                        and normalized_stdout == case["stdout_template"]
                        and normalized_stderr == case["stderr"])
            row = {
                "id": case["id"], "command": case["command"], "actual_exit": result.returncode,
                "seconds": time.monotonic() - started, "stdout": stdout, "stderr": stderr,
                "stdout_sha256": sha256(result.stdout), "stderr_sha256": sha256(result.stderr),
                "expected": case, "accepted": accepted,
            }
            records.append(row)
            print("NATIVE_CMD_REPLAY " + json.dumps(row), flush=True)
            if not accepted:
                failures.append(case["id"])
    report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
              "identity": identity(), "records": records, "failures": failures,
              "historical_baseline_conclusion": "failure",
              "historical_failure_scope": "three initial stdout hypotheses; all 37 observations completed"}
    save("native-replay.json", report)
    print("NATIVE_REPLAY_RESULT " + json.dumps(report), flush=True)
    return int(bool(failures))


def command(label: str, script: str, timeout: int) -> dict[str, Any]:
    started = time.monotonic()
    log = OUT / (label + ".log")
    timed_out = False
    with log.open("wb") as stream:
        try:
            result = subprocess.run(
                ["bash", "--noprofile", "--norc", "-c",
                 'set -euo pipefail\neval "$(opam env --switch=rd --set-switch)"\n' + script],
                cwd=SOURCE, stdout=stream, stderr=subprocess.STDOUT, timeout=timeout, check=False,
            )
            actual_exit: int | None = result.returncode
        except subprocess.TimeoutExpired:
            actual_exit = None
            timed_out = True
    record = {"label": label, "script": script, "bootstrap": "eval $(opam env --switch=rd --set-switch)",
              "actual_exit": actual_exit, "timeout": timed_out, "seconds": time.monotonic() - started,
              "log_sha256": sha256(log.read_bytes())}
    print("CHARAMEL_GATE_COMMAND " + json.dumps(record), flush=True)
    if actual_exit != 0:
        print(log.read_text("utf-8", errors="backslashreplace")[-32000:], flush=True)
    return record


def gates(lane: str) -> int:
    identity()
    records: list[dict[str, Any]] = []
    recipes = [
        ("compiler-and-formatter", 'test "$(ocamlc -version)" = 5.5.1\ntest "$(ocamlformat --version)" = 0.29.0\nopam list --installed', 60),
        ("format", "dune build @fmt", 600),
        ("release-build", "dune build --profile release", 1200),
    ]
    if lane == "linux":
        recipes.append(("release-tests", "dune runtest --profile release", 1200))
    elif lane == "windows":
        recipes.append(("native-shell-permission-tests",
                        "dune exec --profile release test/crush/test_crush.exe -- test '^(tools-shell|permission)$'", 600))
    else:
        raise ValueError(lane)
    for label, script, timeout in recipes:
        records.append(command(label, script, timeout))
        save("gate-commands.json", records)
    records.append(command("whitespace", "git diff --cached --check", 60))
    exact = identity()
    accepted = all(record["actual_exit"] == 0 and not record["timeout"] for record in records)
    report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
              "lane": lane, "identity": exact, "commands": records, "accepted": accepted,
              "native_windows_full_suite_claim": False}
    save("gates-result.json", report)
    print("CHARAMEL_GATES_RESULT " + json.dumps(report), flush=True)
    return 0 if accepted else 1


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    phase = sys.argv[1]
    if phase == "preflight":
        return preflight()
    if phase == "windows-native":
        return native_windows()
    if phase == "gates":
        return gates(sys.argv[2])
    if phase == "identity":
        save("final-source.json", identity())
        return 0
    raise ValueError(phase)


if __name__ == "__main__":
    sys.exit(main())
