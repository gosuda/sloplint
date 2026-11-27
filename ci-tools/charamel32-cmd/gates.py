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



def tree_entries(tree: str) -> dict[str, tuple[str, str, str]]:
    raw = subprocess.check_output(["git", "-C", str(SOURCE), "ls-tree", "-r", "-z", tree])
    entries: dict[str, tuple[str, str, str]] = {}
    for item in raw.split(b"\0"):
        if not item:
            continue
        metadata, path = item.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        entries[path.decode("utf-8")] = (mode, kind, object_id)
    return entries


def windows_fixture(text: str) -> str:
    start = text.index("let test_cmd_loop_permissions () =")
    end = text.index("\nlet test_foreground_capture", start)
    return text[start:end]


def authenticated_windows() -> dict[str, Any]:
    archive: dict[str, Any] = MANIFEST["windows_proof"]
    request = urllib.request.Request(
        f'https://api.github.com/repos/gosuda/sloplint/actions/artifacts/{archive["id"]}/zip',
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/vnd.github+json"},
    )
    opener = urllib.request.build_opener(ArtifactRedirect())
    with opener.open(request, timeout=90) as response:
        data = response.read()
    assert sha256(data) == archive["sha256"], "Windows archive digest"
    reports: dict[str, dict[str, Any]] = {}
    with zipfile.ZipFile(io.BytesIO(data)) as contents:
        for filename in ("gates-result.json", "native-replay.json", "native-selection.json"):
            matches = [name for name in contents.namelist() if name.endswith(filename)]
            assert len(matches) == 1, filename
            reports[filename] = json.loads(contents.read(matches[0]))
    result = reports["gates-result.json"]
    assert result["producer"] == archive["producer"]
    assert str(result["run"]) == str(archive["run"])
    assert result["lane"] == "windows" and result["accepted"] is True
    assert result["identity"]["candidate_tree"] == MANIFEST["windows_tree"]
    assert result["identity"]["source_sha256"] == MANIFEST["previous_sha256"]
    assert result["identity"]["unstaged_clean"] is True
    assert result["identity"]["unexpected_untracked"] == []
    required = {"compiler-and-formatter", "format", "release-build",
                "native-shell-permission-tests", "whitespace"}
    assert {command["label"] for command in result["commands"]} == required
    assert all(command["actual_exit"] == 0 and not command["timeout"]
               for command in result["commands"])
    replay = reports["native-replay.json"]
    assert replay["producer"] == archive["producer"]
    assert str(replay["run"]) == str(archive["run"])
    assert replay["identity"]["candidate_tree"] == MANIFEST["windows_tree"]
    assert len(replay["records"]) == 55 and replay["failures"] == []
    assert all(record["accepted"] for record in replay["records"])
    assert reports["native-selection.json"]["selected_and_passed"] is True
    save("authenticated-windows-reports.json", reports)
    return result


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
    assert preparation["candidate_all_pass"] is True
    assert preparation["candidate_cases"] == 124
    assert preparation["intermediate_tree"] == MANIFEST["windows_tree"]
    assert preparation["intermediate_actual_failures"] == []
    assert preparation["native_record_counts"] == [37, 50, 18, 14]
    assert preparation["source_restored"] is True
    assert preparation["baseline_actual_failures"] >= 4
    windows = authenticated_windows()
    previous: list[dict[str, str]] = json.loads((ROOT / "previous-elements.json").read_text("utf-8"))
    assert {entry["path"] for entry in previous} == set(MANIFEST["previous_sha256"])
    for entry in previous:
        content = entry["content"].encode("utf-8")
        assert sha256(content) == entry["sha256"] == MANIFEST["previous_sha256"][entry["path"]]
        (SOURCE / entry["path"]).write_bytes(content)
    assert git("add", "--", *[entry["path"] for entry in previous]) == ""
    assert git("write-tree") == MANIFEST["windows_tree"]
    prior_entries = tree_entries(MANIFEST["windows_tree"])
    git("reset", "--hard", MANIFEST["head"])
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("status", "--porcelain") == ""
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
    current_entries = tree_entries(MANIFEST["candidate_tree"])
    test_path = "test/crush/test_tools_shell.ml"
    assert prior_entries[test_path][:2] == current_entries[test_path][:2]
    assert {path: entry for path, entry in prior_entries.items() if path != test_path} == {
        path: entry for path, entry in current_entries.items() if path != test_path
    }, "a non-test blob or mode differs from the native Windows source"
    assert git("diff", "--name-only", MANIFEST["windows_tree"], MANIFEST["candidate_tree"]).splitlines() == [test_path]
    old_test = next(entry["content"] for entry in previous if entry["path"] == test_path)
    new_test = (SOURCE / test_path).read_text("utf-8")
    assert windows_fixture(old_test) == windows_fixture(new_test)
    assert sha256(windows_fixture(new_test).encode("utf-8")) == MANIFEST["windows_fixture_sha256"]
    carry = {
        "from_tree": MANIFEST["windows_tree"], "current_tree": MANIFEST["candidate_tree"],
        "windows_producer": windows["producer"], "windows_run": windows["run"],
        "windows_artifact": MANIFEST["windows_proof"], "non_test_entries_identical": len(prior_entries) - 1,
        "only_changed_path": test_path, "windows_specific_fixture_identical": True,
        "windows_fixture_sha256": MANIFEST["windows_fixture_sha256"],
        "scope": "Exact production and native Windows permission fixture evidence; the additional test-only current-head reconciliation runs full Linux gates separately.",
        "current_tree_executed_on_windows": False,
    }
    save("windows-carry-forward.json", carry)
    print("WINDOWS_CARRY_FORWARD " + json.dumps(carry), flush=True)
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
    assert len(records) == 55
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
                           "review argument boundaries"]
        selected = (all(any("[OK]" in line and label in line for line in summary)
                        for label in required_labels)
                    and any("[OK]" in line and re.search(r"\bpermission\b", line) for line in summary))
        save("native-selection.json", {"required_labels": required_labels,
                                     "selected_and_passed": selected})
        accepted = accepted and selected
    report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
              "lane": lane, "identity": exact, "commands": records, "accepted": accepted,
              "native_windows_full_suite_claim": False,
              "windows_carry_forward": json.loads((OUT / "windows-carry-forward.json").read_text("utf-8"))}
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
