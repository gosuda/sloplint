"""Verify exact public Charamel parser source with authentic native and Dune gates."""
from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import re
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


def authenticated_archive(archive: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
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
        formatted_names = [name for name in contents.namelist()
                           if name.endswith("formatted-candidate.json")]
        assert len(formatted_names) == 1
        formatted: dict[str, Any] = json.loads(contents.read(formatted_names[0]))
    assert report["producer"] == archive["producer"]
    assert formatted["tree"] == report["formatted_tree"]
    save("authenticated-" + archive["report"], report)
    save("authenticated-formatted-candidate.json", formatted)
    return report, formatted


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
    preparation, formatted = authenticated_archive(MANIFEST["preparation"])
    assert preparation["head"] == MANIFEST["head"]
    assert preparation["original_tree"] == MANIFEST["original_tree"]
    assert preparation["formatted_tree"] == MANIFEST["prepared_tree"]
    assert str(preparation["run"]) == str(MANIFEST["preparation"]["run"])
    assert preparation["accepted"] is True and preparation["source_restored"] is True
    assert preparation["native_counts"] == [40, 6]
    stages = preparation["stages"]
    assert list(stages) == ["baseline", "payload", "candidate", "echo_mutant", "restored"]
    assert all(stage["classifier_cases"] == 177 and stage["trace_cases"] == 24
               for stage in stages.values())
    for label in ("candidate", "restored"):
        assert stages[label]["classifier_failures"] == []
        assert stages[label]["trace_failures"] == []
        assert stages[label]["tree"] == MANIFEST["candidate_tree"]
    assert stages["baseline"]["classifier_failures"]
    assert stages["baseline"]["trace_failures"]
    assert stages["payload"]["classifier_failures"] and stages["payload"]["trace_failures"] == []
    assert stages["echo_mutant"]["classifier_failures"] and stages["echo_mutant"]["trace_failures"] == []
    patch = ROOT / "candidate.patch"
    assert sha256(patch.read_bytes()) == MANIFEST["patch_sha256"]
    applied = subprocess.run(["git", "apply", "--index", str(patch)], cwd=SOURCE, capture_output=True, check=False)
    save("apply.json", {"actual_exit": applied.returncode,
                       "stdout": applied.stdout.decode("utf-8", errors="backslashreplace"),
                       "stderr": applied.stderr.decode("utf-8", errors="backslashreplace")})
    assert applied.returncode == 0
    current = identity()
    assert {entry["path"] for entry in formatted["elements"]} == set(MANIFEST["source_sha256"])
    for entry in formatted["elements"]:
        before = entry["content"]
        assert sha256(before.encode("utf-8")) == entry["sha256"]
        assert (SOURCE / entry["path"]).read_bytes() == before.encode("utf-8"), entry["path"]
    save("exact-candidate.json", current)
    print("EXACT_CHARAMEL_CANDIDATE " + json.dumps(current), flush=True)
    return 0


def native_windows() -> int:
    assert sys.platform == "win32"
    identity()
    expected: dict[str, Any] = json.loads((ROOT / "native-observed.json").read_text("utf-8"))
    assert sha256((ROOT / "native-observed.json").read_bytes()) == MANIFEST["native_observed_sha256"]
    option_expected: dict[str, Any] = json.loads((ROOT / "native-options-observed.json").read_text("utf-8"))
    assert sha256((ROOT / "native-options-observed.json").read_bytes()) == MANIFEST["native_options_sha256"]
    assert option_expected["producer"] == "2786b929fed8e9f74702a07c6ffceadbebbf6b95"
    assert len(option_expected["records"]) == 18
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
        for case in option_expected["records"]:
            started = time.monotonic()
            result = subprocess.run(
                f'"{comspec}" /d /c {case["command"]}', executable=comspec,
                cwd=cwd, env=env, stdin=subprocess.DEVNULL, capture_output=True,
                timeout=10, check=False,
            )
            stdout = result.stdout.decode("utf-8", errors="backslashreplace")
            stderr = result.stderr.decode("utf-8", errors="backslashreplace")
            accepted = (result.returncode == case["actual_exit"]
                        and stdout == case["stdout"] and stderr == case["stderr"])
            row = {
                "id": case["id"], "command": case["command"], "actual_exit": result.returncode,
                "seconds": time.monotonic() - started, "stdout": stdout, "stderr": stderr,
                "stdout_sha256": sha256(result.stdout), "stderr_sha256": sha256(result.stderr),
                "expected": case, "accepted": accepted, "group": "attached-option-observations",
            }
            records.append(row)
            print("NATIVE_CMD_REPLAY " + json.dumps(row), flush=True)
            if not accepted:
                failures.append(case["id"])
    for record_count in (40, 6):
        path = ROOT / ("native-" + str(record_count) + "-observed.json")
        assert sha256(path.read_bytes()) == MANIFEST["echo_native_sha256"][str(record_count)]
        echo_expected: dict[str, Any] = json.loads(path.read_text("utf-8"))
        assert len(echo_expected["records"]) == record_count
        with tempfile.TemporaryDirectory(prefix="charamel32-echo-replay-") as temporary:
            cwd = Path(temporary)
            for case in echo_expected["records"]:
                capture = cwd / "capture.txt"
                capture.unlink(missing_ok=True)
                started = time.monotonic()
                result = subprocess.run(
                    f'"{comspec}" /d /c {case["command"]}', executable=comspec,
                    cwd=cwd, env=dict(os.environ), stdin=subprocess.DEVNULL,
                    capture_output=True, timeout=10, check=False)
                stdout = result.stdout.decode("utf-8", errors="backslashreplace").replace(str(cwd), "{cwd}")
                stderr = result.stderr.decode("utf-8", errors="backslashreplace").replace(str(cwd), "{cwd}")
                saved = capture.read_bytes() if capture.exists() else None
                capture_text = None if saved is None else saved.decode("utf-8", errors="backslashreplace")
                accepted = (result.returncode == case["actual_exit"]
                            and stdout == case["stdout"] and stderr == case["stderr"]
                            and capture_text == case["capture"])
                row = {"id": case["id"], "command": case["command"],
                       "actual_exit": result.returncode, "seconds": time.monotonic() - started,
                       "stdout": stdout, "stderr": stderr,
                       "stdout_sha256": sha256(result.stdout), "stderr_sha256": sha256(result.stderr),
                       "capture": capture_text,
                       "capture_sha256": None if saved is None else sha256(saved),
                       "expected": case, "accepted": accepted,
                       "group": "echo-" + str(record_count)}
                records.append(row)
                print("NATIVE_CMD_REPLAY " + json.dumps(row), flush=True)
                if not accepted:
                    failures.append(case["id"])
    assert len(records) == 101
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
    content = log.read_text("utf-8", errors="backslashreplace")
    if label in ("release-tests", "native-shell-permission-tests"):
        clean = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", content)
        selected = [line for line in clean.splitlines()
                    if (("tools-shell" in line or "permission" in line)
                        and any(mark in line for mark in ("[OK]", "[SKIP]", "[FAIL]")))
                    or "tests run" in line]
        record["test_summary"] = selected
        for line in selected:
            print("CHARAMEL_TEST_RESULT " + line, flush=True)
    if actual_exit != 0:
        print(content[-32000:], flush=True)
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
    if lane == "windows":
        focused = next(record for record in records if record["label"] == "native-shell-permission-tests")
        summary = focused.get("test_summary", [])
        required_labels = ["conservative shell classifier", "Cmd loop approval boundary",
                           "Cmd echo approval boundary", "review argument boundaries"]
        selected = (all(any("[OK]" in line and label in line for line in summary)
                        for label in required_labels)
                    and any("[OK]" in line and re.search(r"\bpermission\b", line) for line in summary))
        save("native-selection.json", {"required_labels": required_labels,
                                     "selected_and_passed": selected})
        accepted = accepted and selected
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
