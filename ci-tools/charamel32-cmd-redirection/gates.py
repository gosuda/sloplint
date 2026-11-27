"""Verify exact public Charamel parser source with authentic native and Dune gates."""
from __future__ import annotations

import base64
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
import observe as redirection_observer
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
    prefix = MANIFEST["posix_prefix"]
    assert sha256((SOURCE / "bin/crush/lib/shell_screen.ml").read_bytes()[:prefix["bytes"]]) == prefix["sha256"]
    return {
        "head": MANIFEST["head"], "original_tree": MANIFEST["original_tree"],
        "candidate_tree": MANIFEST["candidate_tree"], "source_sha256": MANIFEST["source_sha256"],
        "changed_paths": changed, "unstaged_clean": True, "unexpected_untracked": [],
        "index_sha256": sha256(subprocess.check_output(["git", "-C", str(SOURCE), "ls-files", "-s"])),
    }


def preflight() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    assert sha256(Path(__file__).read_bytes()) == MANIFEST["driver_sha256"]
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
    assert preparation["native_counts"] == [31, 10, 10]
    stages = preparation["stages"]
    assert list(stages) == ["observed_original", "old_join", "candidate", "redirection_mutant", "restored"]
    assert all(stage["classifier_cases"] == 203 and stage["trace_cases"] == 48
               for stage in stages.values())
    for label in ("candidate", "restored"):
        assert stages[label]["classifier_failures"] == []
        assert stages[label]["trace_failures"] == []
        assert stages[label]["tree"] == MANIFEST["candidate_tree"]
        assert stages[label]["maintained_test"]["actual_exit"] == 0
    for label in ("observed_original", "redirection_mutant"):
        assert "redirect-reported_quoted_outer" in stages[label]["classifier_failures"]
        assert "nested_redirect_equal" in stages[label]["trace_failures"]
        assert stages[label]["maintained_test"]["actual_exit"] == 1
    assert stages["old_join"]["maintained_test"]["actual_exit"] == 1
    assert "nested_equal" in stages["old_join"]["trace_failures"]
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



def normalized_capture(row: dict[str, Any], capture: dict[str, Any]) -> Any:
    data = base64.b64decode(capture["base64"], validate=True)
    assert len(data) == capture["bytes"] and sha256(data) == capture["sha256"]
    if not data:
        return {"empty": True}
    if row["kind"] == "child":
        record = json.loads(data.decode("utf-8-sig"))
        assert set(record) == {"observer_pid", "parent_pid", "parent_name",
                               "parent_command_line", "arguments"}
        assert isinstance(record["observer_pid"], int) and record["observer_pid"] > 0
        assert isinstance(record["parent_pid"], int) and record["parent_pid"] > 0
        assert record["parent_name"].lower() == "cmd.exe"
        return {key: value for key, value in record.items()
                if key not in ("observer_pid", "parent_pid")}
    if row["id"] == "for_body_outer" and b">cmd /d /c " in data:
        match = re.search(rb"(?m)^([^\r\n]+)>cmd /d /c ", data)
        assert match is not None and match.group(1).endswith(b"\\for_body_outer")
        data = data[:match.start(1)] + b"{owned-cwd}" + data[match.end(1):]
    return {"bytes": base64.b64encode(data).decode("ascii")}


def native_redirection() -> int:
    assert sys.platform == "win32"
    identity()
    assert Path(redirection_observer.__file__).resolve() == (ROOT / "observe.py").resolve()
    assert sha256((ROOT / "observe.py").read_bytes()) == MANIFEST["observer_driver_sha256"]
    helper = (ROOT / "observe-child.ps1").read_bytes()
    assert sha256(helper) == MANIFEST["observer_sha256"]
    rows: list[dict[str, Any]] = []
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="charamel32-redirect-replay-") as temporary:
        for group in ("primary", "adjacent", "descriptor"):
            path = ROOT / ("native-" + group + ".json")
            assert sha256(path.read_bytes()) == MANIFEST["native_redirection_sha256"][group]
            frozen = json.loads(path.read_text("utf-8"))
            for expected in frozen["records"]:
                case = {"id": expected["id"], "kind": expected["kind"],
                        "command": expected["command"], "require_exit": expected["actual_exit"]}
                actual = redirection_observer.observe(case, Path(temporary), helper)
                comparison_error: str | None = None
                try:
                    passed = actual["required_pass"] and (
                        actual["actual_exit"] == expected["actual_exit"]
                        and normalized_capture(actual, actual["stdout"]) == normalized_capture(expected, expected["stdout"])
                        and normalized_capture(actual, actual["stderr"]) == normalized_capture(expected, expected["stderr"])
                        and set(actual["files"]) == set(expected["files"])
                        and all(normalized_capture(actual, actual["files"][name])
                                == normalized_capture(expected, expected["files"][name])
                                for name in actual["files"])
                    )
                except (AssertionError, ValueError, KeyError) as error:
                    passed = False
                    comparison_error = repr(error)
                row = {"group": group, "actual": actual, "expected": expected,
                       "accepted": passed, "comparison_error": comparison_error}
                rows.append(row)
                if not passed:
                    failures.append(expected["id"])
                print("NATIVE_REDIRECTION_REPLAY " + json.dumps(row), flush=True)
    assert len(rows) == 51
    result = {
        "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "identity": identity(), "record_count": len(rows), "records": rows, "failures": failures,
        "normalization": "Only the two owned process identifiers in child-observer JSON and the owned directory in the for_body_outer command-echo line are normalized. Every other output byte, filename, child command line, argument and actual exit is compared.",
    }
    save("native-redirection-replay.json", result)
    print("NATIVE_REDIRECTION_REPLAY_RESULT " + json.dumps(result), flush=True)
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
                           "Cmd echo approval boundary", "review argument boundaries",
                            "Cmd payload source bytes", "Cmd redirection approval boundary"]
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
        retained = native_windows()
        redirection = native_redirection()
        return max(retained, redirection)
    if phase == "gates":
        return gates(sys.argv[2])
    if phase == "identity":
        save("final-source.json", identity())
        return 0
    raise ValueError(phase)


if __name__ == "__main__":
    sys.exit(main())
