"""Authenticate Cmd ownership, prove maintained regressions, and format an exact candidate."""
from __future__ import annotations
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
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
MANIFEST: dict[str, Any] = json.loads((ROOT / "prepare.json").read_text("utf-8"))
COMMANDS: list[dict[str, Any]] = []


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def save(name: str, value: Any) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")


def run(label: str, argv: list[str], cwd: Path) -> subprocess.CompletedProcess[bytes]:
    started = time.monotonic()
    try:
        result = subprocess.run(argv, cwd=cwd, capture_output=True, timeout=300, check=False)
    except subprocess.TimeoutExpired as error:
        data = (error.stdout or b"") + (error.stderr or b"")
        (OUT / (label + ".log")).write_bytes(data)
        row = {"label": label, "argv": argv, "actual_exit": None, "timeout": True,
               "seconds": time.monotonic() - started, "log_sha256": digest(data)}
        COMMANDS.append(row)
        save("commands.json", COMMANDS)
        print("REDIRECTION_PREPARE_COMMAND " + json.dumps(row), flush=True)
        raise
    data = result.stdout + result.stderr
    (OUT / (label + ".log")).write_bytes(data)
    row = {"label": label, "argv": argv, "actual_exit": result.returncode, "timeout": False,
           "seconds": time.monotonic() - started, "log_sha256": digest(data)}
    COMMANDS.append(row)
    save("commands.json", COMMANDS)
    print("REDIRECTION_PREPARE_COMMAND " + json.dumps(row), flush=True)
    if result.returncode:
        print(data.decode("utf-8", errors="backslashreplace")[-24000:], flush=True)
    return result


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


def archive_files(archive: dict[str, Any]) -> dict[str, bytes]:
    request = urllib.request.Request(
        f'https://api.github.com/repos/gosuda/sloplint/actions/artifacts/{archive["id"]}/zip',
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/vnd.github+json"},
    )
    with urllib.request.build_opener(ArtifactRedirect()).open(request, timeout=90) as response:
        data = response.read()
    assert digest(data) == archive["sha256"], archive["id"]
    with zipfile.ZipFile(io.BytesIO(data)) as contents:
        result = {name: contents.read(name) for name in contents.namelist() if not name.endswith("/")}
    save("archive-" + str(archive["id"]) + ".json",
         {"artifact": archive, "verified_zip_sha256": digest(data),
          "files": {name: digest(content) for name, content in result.items()}})
    return result


def file_named(files: dict[str, bytes], name: str) -> bytes:
    matches = [content for path, content in files.items() if path.rsplit("/", 1)[-1] == name]
    assert len(matches) == 1, name
    return matches[0]



def authenticate() -> list[dict[str, Any]]:
    native: list[dict[str, Any]] = []
    for archive in MANIFEST["native_archives"]:
        files = archive_files(archive)
        report: dict[str, Any] = json.loads(file_named(files, archive["report"]))
        assert report["producer"] == archive["producer"]
        assert str(report["run"]) == str(archive["run"])
        assert report["before"] == report["after"]
        assert report["after"]["head"] == MANIFEST["head"]
        assert report["after"]["tree"] == MANIFEST["tree"]
        assert report["after"]["source_sha256"] == MANIFEST["native_source_sha256"]
        assert report["after"]["source_clean"] is True
        assert report["failures"] == [] and report["record_count"] == archive["records"]
        frozen = json.loads((ROOT / ("native-" + archive["label"] + ".json")).read_text("utf-8"))
        assert report == frozen
        assert all(row["required_pass"] and row["actual_exit"] == 0 for row in report["records"])
        save("authenticated-native-" + archive["label"] + ".json", report)
        native.append(report)
    previous = MANIFEST["retained_preparation"]
    files = archive_files(previous)
    result = json.loads(file_named(files, "prepare-result.json"))
    cases = json.loads(file_named(files, "candidate-classifier.json"))
    traces = json.loads(file_named(files, "candidate-trace.json"))
    assert result["producer"] == previous["producer"] and str(result["run"]) == str(previous["run"])
    assert result["formatted_tree"] == MANIFEST["tree"]
    assert result["accepted"] is True and result["source_restored"] is True
    assert len(cases) == 177 and all(row["actual_exit"] == 0 for row in cases)
    keys = ("id", "dialect", "command", "expected")
    assert [[row[key] for key in keys] for row in cases] == [
        [row[key] for key in keys] for row in MANIFEST["cases"][:177]]
    current = {row["id"]: row for row in MANIFEST["traces"]}
    assert len(traces) == 24 and all(row["actual_exit"] == 0 for row in traces)
    corrections: list[dict[str, Any]] = []
    for row in traces:
        actual = [entry["source"] for entry in row["actual_trace"]]
        updated = current[row["id"]]
        assert row["command"] == updated["command"]
        assert row["expected_kind"] == updated["expected_kind"]
        if row["id"] in MANIFEST["corrected_old_traces"]:
            assert actual != updated["expected_sources"]
            corrections.append({"id": row["id"], "historical": actual,
                                "native_justified": updated["expected_sources"]})
        else:
            assert actual == updated["expected_sources"]
    assert len(corrections) == 2
    save("historical-trace-corrections.json", corrections)
    save("authenticated-retained-preparation.json", result)
    return native


def instrument(source: str) -> str:
    needle = "let rec parse_cmd sink ~depth source ="
    assert source.count(needle) == 1
    start = source.index(needle)
    tail = source[start:]
    for name in ("parse_cmd", "end_segment", "for_command", "analyze_cmd"):
        tail = tail.replace(name + " sink ~depth", name + " sink ~trace ~depth")
    needle = "let rec parse_cmd sink ~trace ~depth source =\n"
    assert tail.count(needle) == 1
    tail = tail.replace(needle, needle + "  trace source;\n", 1)
    assert tail.count("let screen dialect command =") == 1
    tail = tail.replace("let screen dialect command =", "let screen_with_cmd_trace ~trace dialect command =", 1)
    tail += (
        "\nlet screen dialect command = screen_with_cmd_trace ~trace:ignore dialect command\n"
        "\nmodule For_test = struct\n"
        "  let screen_cmd command =\n"
        "    let sources = ref [] in\n"
        "    let trace source = sources := source :: !sources in\n"
        "    let verdict = screen_with_cmd_trace ~trace Cmd command in\n"
        "    (verdict, List.rev !sources)\n"
        "end\n"
    )
    return source[:start] + tail


def apply(elements: list[dict[str, str]]) -> None:
    for element in elements:
        data = element["content"].encode("utf-8")
        assert digest(data) == element["sha256"], element["path"]
        (SOURCE / element["path"]).write_bytes(data)
    assert git("add", "--", *[element["path"] for element in elements]) == ""


def compile_probe(label: str, directory: Path, trace: bool) -> Path:
    directory.mkdir(parents=True)
    source = (SOURCE / "bin/crush/lib/shell_screen.ml").read_bytes()
    interface = (SOURCE / "bin/crush/lib/shell_screen.mli").read_bytes()
    if trace:
        probe = (
            "let kind = function\n"
            '  | Shell_screen.Clear -> "clear"\n'
            '  | Shell_screen.Blocked _ -> "blocked"\n'
            '  | Shell_screen.Review _ -> "review"\n'
            "let () =\n"
            "  let verdict, sources = Shell_screen.For_test.screen_cmd Sys.argv.(1) in\n"
            "  print_endline (kind verdict);\n"
            "  List.iter (fun source ->\n"
            '    String.iter (fun c -> Printf.printf "%02x" (Char.code c)) source;\n'
            "    print_newline ()) sources\n"
        )
    else:
        probe = (
            "let () =\n"
            '  let dialect = if Sys.argv.(1) = "cmd" then Shell_screen.Cmd else Shell_screen.Posix in\n'
            "  let verdict = Shell_screen.screen dialect Sys.argv.(2) in\n"
            "  let kind = match verdict with\n"
            '    | Shell_screen.Clear -> "clear"\n'
            '    | Shell_screen.Blocked _ -> "blocked"\n'
            '    | Shell_screen.Review _ -> "review" in\n'
            "  print_endline kind;\n"
            "  if kind <> Sys.argv.(3) then exit 1\n"
        )
    (directory / "shell_screen.ml").write_bytes(source)
    (directory / "shell_screen.mli").write_bytes(interface)
    (directory / "probe.ml").write_text(probe, encoding="utf-8")
    save(label + "-source.json", {
        "source_sha256": digest(source), "interface_sha256": digest(interface),
        "probe_sha256": digest(probe.encode("utf-8")), "public_test_observer": trace,
    })
    binary = directory / "screen-probe"
    result = run(label, [
        "opam", "exec", "--switch=rd", "--", "ocamlc",
        "-strict-sequence", "-strict-formats", "-short-paths", "-principal",
        "-warn-error", "+a", "-o", str(binary), "shell_screen.mli", "shell_screen.ml", "probe.ml",
    ], directory)
    assert result.returncode == 0, label
    return binary


def classify(label: str, binary: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for case in MANIFEST["cases"]:
        result = subprocess.run(
            [str(binary), case["dialect"], case["command"], case["expected"]],
            capture_output=True, timeout=10, check=False)
        row = {**case, "actual_exit": result.returncode,
               "actual_kind": result.stdout.decode("utf-8").strip(),
               "stderr": result.stderr.decode("utf-8", errors="backslashreplace")}
        assert result.returncode in (0, 1), row
        rows.append(row)
        print("REDIRECTION_CLASSIFIER_" + label.upper() + " " + json.dumps(row), flush=True)
    save(label + "-classifier.json", rows)
    return rows


def traces(label: str, binary: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for case in MANIFEST["traces"]:
        result = subprocess.run([str(binary), case["command"]], capture_output=True, timeout=10, check=False)
        assert result.returncode == 0, (case["id"], result.stderr)
        lines = result.stdout.decode("utf-8").splitlines()
        actual = [bytes.fromhex(line).decode("utf-8") for line in lines[1:]]
        checked_kind = case["expected_kind"] if label in ("candidate", "restored") else None
        passed = actual == case["expected_sources"] and (
            checked_kind is None or lines[0] == checked_kind)
        row = {**case, "actual_exit": result.returncode, "actual_kind": lines[0],
               "actual_sources": actual, "checked_kind": checked_kind, "passed": passed,
               "stderr": result.stderr.decode("utf-8", errors="backslashreplace")}
        rows.append(row)
        print("REDIRECTION_TRACE_" + label.upper() + " " + json.dumps(row), flush=True)
    save(label + "-trace.json", rows)
    return rows


def maintained_test(label: str, expected_exit: int, expected_failure: str | None = None) -> dict[str, Any]:
    test = (SOURCE / "test/crush/test_tools_shell.ml").read_text("utf-8")
    cases = test[test.rindex("let cases ="):]
    import re
    names = re.findall(r'Test_tools_test_support\.case\s+"([^"]+)"', cases)
    assert names.index("Cmd payload source bytes") == MANIFEST["maintained_test_index"]
    result = run(label + "-maintained-test", [
        "opam", "exec", "--switch=rd", "--", "dune", "exec", "--profile", "release",
        "test/crush/test_crush.exe", "--", "test", "--show-errors", "^tools-shell$",
        str(MANIFEST["maintained_test_index"]),
    ], SOURCE)
    data = result.stdout + result.stderr
    assert result.returncode == expected_exit, (label, result.returncode)
    assert b"Cmd payload source bytes" in data
    if expected_failure is not None:
        assert expected_failure.encode("utf-8") in data, label
    return {"actual_exit": result.returncode, "expected_exit": expected_exit,
            "expected_failure": expected_failure, "log_sha256": digest(data)}


def stage(label: str, scratch: Path, expected_test_exit: int,
          failure: str | None = None) -> dict[str, Any]:
    plain = compile_probe(label + "-compile", scratch / (label + "-plain"), False)
    traced = compile_probe(label + "-trace-compile", scratch / (label + "-trace"), True)
    return {
        "tree": git("write-tree"), "classifier": classify(label, plain),
        "trace": traces(label, traced),
        "maintained_test": maintained_test(label, expected_test_exit, failure),
    }


def retained(state: dict[str, Any]) -> None:
    assert all(row["actual_exit"] == 0 for row in state["classifier"][:177])


def green(state: dict[str, Any]) -> None:
    assert all(row["actual_exit"] == 0 for row in state["classifier"])
    assert all(row["passed"] for row in state["trace"])
    assert state["maintained_test"]["actual_exit"] == 0


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    assert digest(Path(__file__).read_bytes()) == MANIFEST["driver_sha256"]
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("rev-parse", "HEAD^{tree}") == MANIFEST["tree"]
    assert git("status", "--porcelain") == ""
    originals = {path: (SOURCE / path).read_bytes() for path in MANIFEST["original_sha256"]}
    for path, expected in MANIFEST["original_sha256"].items():
        assert digest(originals[path]) == expected
    native = authenticate()
    assert run("compiler-version", ["opam", "exec", "--switch=rd", "--", "ocamlc", "-version"], SOURCE).stdout.strip() == b"5.5.1"
    assert run("formatter-version", ["opam", "exec", "--switch=rd", "--", "ocamlformat", "--version"], SOURCE).stdout.strip() == b"0.29.0"
    legacy = MANIFEST["legacy_source"]
    assert run("fetch-legacy-source", ["git", "fetch", "--no-tags", "--depth=1", "origin", legacy["head"]], SOURCE).returncode == 0
    assert git("rev-parse", legacy["head"] + "^{tree}") == legacy["tree"]
    legacy_source = subprocess.check_output(["git", "-C", str(SOURCE), "show", legacy["head"] + ":bin/crush/lib/shell_screen.ml"])
    assert digest(legacy_source) == legacy["sha256"]
    elements: list[dict[str, str]] = json.loads((ROOT / "candidate-elements.json").read_text("utf-8"))
    assert {entry["path"]: entry["sha256"] for entry in elements} == MANIFEST["candidate_sha256"]
    observed_source = instrument(originals["bin/crush/lib/shell_screen.ml"].decode("utf-8"))
    legacy_observed = instrument(legacy_source.decode("utf-8"))
    assert digest(observed_source.encode("utf-8")) == MANIFEST["observer_only_sha256"]
    assert digest(legacy_observed.encode("utf-8")) == legacy["observer_sha256"]
    observed = [{**entry, "content": observed_source, "sha256": MANIFEST["observer_only_sha256"]}
                if entry["path"] == "bin/crush/lib/shell_screen.ml" else entry
                for entry in elements]
    old_join = [{**entry, "content": legacy_observed, "sha256": legacy["observer_sha256"]}
                if entry["path"] == "bin/crush/lib/shell_screen.ml" else entry
                for entry in elements]
    states: dict[str, dict[str, Any]] = {}
    final: dict[str, Any] = {}
    accepted = False
    try:
        with tempfile.TemporaryDirectory(prefix="charamel32-redirection-compile-") as temporary:
            scratch = Path(temporary)
            original_binary = compile_probe("original-compile", scratch / "original", False)
            original_rows = classify("original", original_binary)
            assert all(row["actual_exit"] == 0 for row in original_rows[:177])
            original_by_id = {row["id"]: row for row in original_rows}
            assert original_by_id["redirect-reported_quoted_outer"]["actual_exit"] == 1
            apply(observed)
            states["observed_original"] = stage("observed_original", scratch, 1, "nested_redirect_equal")
            assert states["observed_original"]["classifier"] == original_rows
            retained(states["observed_original"])
            apply(old_join)
            states["old_join"] = stage("old_join", scratch, 1, "nested_equal")
            old_traces = {row["id"]: row for row in states["old_join"]["trace"]}
            for key in ("nested_equal", "nested_punctuation", "nested_tab", "trailing_plain"):
                assert old_traces[key]["passed"] is False, key
            apply(elements)
            assert git("write-tree") == MANIFEST["draft_tree"]
            paths = [entry["path"] for entry in elements]
            assert run("format-candidate", ["opam", "exec", "--switch=rd", "--", "ocamlformat", "-i", *paths], SOURCE).returncode == 0
            assert git("add", "--", *paths) == ""
            tree = git("write-tree")
            formatted = [{"path": path, "mode": "100644",
                          "content": (SOURCE / path).read_text("utf-8"),
                          "sha256": digest((SOURCE / path).read_bytes())} for path in paths]
            final = {"head": MANIFEST["head"], "tree": tree, "elements": formatted}
            save("formatted-candidate.json", final)
            print("REDIRECTION_FORMATTED_CANDIDATE " + json.dumps(final), flush=True)
            patch = subprocess.check_output(["git", "-C", str(SOURCE), "diff", "--cached", "--binary"])
            (OUT / "candidate.patch").write_bytes(patch)
            save("candidate-patch.json", {"sha256": digest(patch), "bytes": len(patch)})
            states["candidate"] = stage("candidate", scratch, 0)
            green(states["candidate"])
            apply([{**entry, "content": observed_source, "sha256": MANIFEST["observer_only_sha256"]}
                   if entry["path"] == "bin/crush/lib/shell_screen.ml" else entry
                   for entry in formatted])
            states["redirection_mutant"] = stage("redirection_mutant", scratch, 1, "nested_redirect_equal")
            assert states["redirection_mutant"]["classifier"] == original_rows
            assert states["redirection_mutant"]["trace"] == states["observed_original"]["trace"]
            retained(states["redirection_mutant"])
            apply(formatted)
            assert git("write-tree") == tree
            states["restored"] = stage("restored", scratch, 0)
            green(states["restored"])
            assert git("diff", "--exit-code") == ""
            assert git("ls-files", "--others", "--exclude-standard") == ""
            assert run("candidate-whitespace", ["git", "diff", "--cached", "--check"], SOURCE).returncode == 0
            accepted = True
    finally:
        assert run("restore-original", ["git", "reset", "--hard", MANIFEST["head"]], SOURCE).returncode == 0
        assert git("rev-parse", "HEAD^{tree}") == MANIFEST["tree"]
        assert git("status", "--porcelain") == ""
        for path, data in originals.items():
            assert (SOURCE / path).read_bytes() == data
        summaries = {
            label: {"tree": state["tree"], "classifier_cases": len(state["classifier"]),
                    "classifier_failures": [row["id"] for row in state["classifier"] if row["actual_exit"]],
                    "trace_cases": len(state["trace"]),
                    "trace_failures": [row["id"] for row in state["trace"] if not row["passed"]],
                    "maintained_test": state["maintained_test"]}
            for label, state in states.items()
        }
        result = {
            "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
            "head": MANIFEST["head"], "original_tree": MANIFEST["tree"],
            "formatted_tree": final.get("tree"), "accepted": accepted,
            "native_counts": [len(report["records"]) for report in native],
            "source_restored": True, "stages": summaries, "commands": COMMANDS,
            "scope": "Original parser and exact historical joining implementation receive only the same local observation adapter and the new maintained test; original classifier outputs are compared before and after that adapter. Two historical outer-redirection traces are explicitly corrected.",
        }
        save("prepare-result.json", result)
        print("REDIRECTION_PREPARE_RESULT " + json.dumps(result), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
