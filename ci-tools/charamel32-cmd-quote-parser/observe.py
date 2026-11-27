"""Observe exact maintained and historical Cmd parser inputs without editing source."""
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
MANIFEST: dict[str, Any] = json.loads((ROOT / "parser-observe.json").read_text("utf-8"))
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
    result = subprocess.run(argv, cwd=cwd, capture_output=True, timeout=180, check=False)
    data = result.stdout + result.stderr
    (OUT / (label + ".log")).write_bytes(data)
    row = {"label": label, "argv": argv, "actual_exit": result.returncode,
           "seconds": time.monotonic() - started, "log_sha256": digest(data)}
    COMMANDS.append(row)
    save("commands.json", COMMANDS)
    print("QUOTES_PARSER_COMMAND " + json.dumps(row), flush=True)
    if result.returncode:
        print(data.decode("utf-8", errors="backslashreplace"), flush=True)
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




def instrument(source: str) -> str:
    needle = "let rec parse_cmd sink ~depth source ="
    assert source.count(needle) == 1
    start = source.index(needle)
    tail = source[start:]
    for name in ("parse_cmd", "cmd_char", "end_segment", "for_command", "analyze_cmd"):
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




def compile_probe(label: str, directory: Path, source: bytes, interface: bytes, trace: bool) -> Path:
    directory.mkdir(parents=True)
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
            "  print_endline kind\n"
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
        "opam", "exec", "--", "ocamlc", "-strict-sequence", "-strict-formats",
        "-short-paths", "-principal", "-warn-error", "+a", "-o", str(binary),
        "shell_screen.mli", "shell_screen.ml", "probe.ml",
    ], directory)
    assert result.returncode == 0, label
    return binary


def classify(binary: Path, dialect: str, command: str) -> str:
    result = subprocess.run([str(binary), dialect, command],
                            capture_output=True, timeout=10, check=False)
    assert result.returncode == 0 and result.stderr == b"", result.stderr
    kind = result.stdout.decode("utf-8").strip()
    assert kind in ("clear", "review", "blocked"), kind
    return kind


def trace_command(binary: Path, command: str) -> dict[str, Any]:
    result = subprocess.run([str(binary), command], capture_output=True, timeout=10, check=False)
    assert result.returncode == 0 and result.stderr == b"", result.stderr
    lines = result.stdout.decode("utf-8").splitlines()
    return {"actual_kind": lines[0],
            "actual_sources": [bytes.fromhex(line).decode("utf-8") for line in lines[1:]],
            "actual_exit": result.returncode}


def identity() -> dict[str, Any]:
    return {"head": git("rev-parse", "HEAD"), "tree": git("rev-parse", "HEAD^{tree}"),
            "source_sha256": {path: digest((SOURCE / path).read_bytes())
                              for path in MANIFEST["source_sha256"]},
            "index_sha256": digest(subprocess.check_output(
                ["git", "-C", str(SOURCE), "ls-files", "-s"])),
            "source_clean": git("status", "--porcelain") == ""}


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    assert digest(Path(__file__).read_bytes()) == MANIFEST["driver_sha256"]
    before = identity()
    save("initial-source.json", before)
    assert before["head"] == MANIFEST["head"] and before["tree"] == MANIFEST["tree"]
    assert before["source_clean"] and before["source_sha256"] == MANIFEST["source_sha256"]
    native_files = archive_files(MANIFEST["native"])
    native = json.loads(file_named(native_files, "native-quoted.json"))
    assert native["producer"] == MANIFEST["native"]["producer"]
    assert str(native["run"]) == str(MANIFEST["native"]["run"])
    assert native["before"] == native["after"] == before
    assert native["record_count"] == 20 and native["failures"] == []
    assert [row["id"] for row in native["records"]] == [row["id"] for row in MANIFEST["cases"]]
    assert [row["command"] for row in native["records"]] == [row["command"] for row in MANIFEST["cases"]]
    assert sum(row["actual_exit"] == 0 for row in native["records"]) == 16
    assert sum(row["actual_exit"] == 1 for row in native["records"]) == 4
    previous_files = archive_files(MANIFEST["preparation"])
    previous_result = json.loads(file_named(previous_files, "prepare-result.json"))
    assert previous_result["producer"] == MANIFEST["preparation"]["producer"]
    assert str(previous_result["run"]) == str(MANIFEST["preparation"]["run"])
    assert previous_result["accepted"] and previous_result["source_restored"]
    candidate = json.loads(file_named(previous_files, "formatted-candidate.json"))
    assert candidate["head"] == MANIFEST["prior_head"] and candidate["tree"] == MANIFEST["candidate_tree"]
    elements = {entry["path"]: entry for entry in candidate["elements"]}
    assert {path: entry["sha256"] for path, entry in elements.items()} == MANIFEST["candidate_sha256"]
    for entry in elements.values():
        assert digest(entry["content"].encode("utf-8")) == entry["sha256"]
    ml = "bin/crush/lib/shell_screen.ml"
    mli = "bin/crush/lib/shell_screen.mli"
    assert run("compiler-version", ["opam", "exec", "--", "ocamlc", "-version"], SOURCE).stdout.strip() == b"5.5.1"
    assert run("fetch-prior-source", ["git", "fetch", "--no-tags", "--depth=1", "origin",
               MANIFEST["prior_head"]], SOURCE).returncode == 0
    assert git("rev-parse", MANIFEST["prior_head"] + "^{tree}") == MANIFEST["prior_tree"]
    prior_source = subprocess.check_output(
        ["git", "-C", str(SOURCE), "show", MANIFEST["prior_head"] + ":" + ml])
    assert digest(prior_source) == MANIFEST["prior_source_sha256"]
    original_interface = (SOURCE / mli).read_bytes()
    candidate_interface = elements[mli]["content"].encode("utf-8")
    sources = {"current_d19f": (SOURCE / ml).read_bytes(), "prior_c009": prior_source,
               "verified_ecea": elements[ml]["content"].encode("utf-8")}
    observations: dict[str, list[dict[str, Any]]] = {}
    retained: dict[str, Any] = {}
    accepted = False
    try:
        with tempfile.TemporaryDirectory(prefix="charamel32-quoted-parser-") as temporary:
            scratch = Path(temporary)
            for label, source in sources.items():
                has_observer = label == "verified_ecea"
                interface = candidate_interface if has_observer else original_interface
                observed = source if has_observer else instrument(source.decode("utf-8")).encode("utf-8")
                plain = compile_probe(label + "-compile", scratch / (label + "-plain"),
                                      source, interface, False)
                adapter = compile_probe(label + "-adapter-compile", scratch / (label + "-adapter"),
                                        observed, candidate_interface, False)
                traced = compile_probe(label + "-trace-compile", scratch / (label + "-trace"),
                                       observed, candidate_interface, True)
                retained_rows: list[dict[str, Any]] = []
                for case in MANIFEST["retained_cases"]:
                    plain_kind = classify(plain, case["dialect"], case["command"])
                    adapter_kind = classify(adapter, case["dialect"], case["command"])
                    assert plain_kind == adapter_kind, (label, case["id"])
                    retained_rows.append({**case, "actual_kind": plain_kind,
                                          "passed": plain_kind == case["expected"]})
                if has_observer:
                    assert all(row["passed"] for row in retained_rows)
                retained[label] = {"classifier_count": len(retained_rows),
                                   "classifier_failures": [row["id"] for row in retained_rows if not row["passed"]]}
                save(label + "-retained-classifier.json", retained_rows)
                observations[label] = []
                for case, native_row in zip(MANIFEST["cases"], native["records"], strict=True):
                    plain_kind = classify(plain, "cmd", case["command"])
                    actual = trace_command(traced, case["command"])
                    assert actual["actual_kind"] == plain_kind, (label, case["id"])
                    row = {"id": case["id"], "command": case["command"], **actual,
                           "native_actual_exit": native_row["actual_exit"],
                           "native_required_case": case["require_exit"] is not None}
                    observations[label].append(row)
                    print("QUOTES_PARSER_OBSERVATION " + json.dumps({"source": label, **row}), flush=True)
                save(label + "-observations.json", observations[label])
                if has_observer:
                    retained_traces: list[dict[str, Any]] = []
                    for case in MANIFEST["retained_traces"]:
                        actual = trace_command(traced, case["command"])
                        assert actual["actual_kind"] == case["expected_kind"]
                        assert actual["actual_sources"] == case["expected_sources"], case["id"]
                        retained_traces.append({**case, **actual})
                    save(label + "-retained-traces.json", retained_traces)
                    retained[label]["trace_count"] = len(retained_traces)
            accepted = True
    finally:
        after = identity()
        assert after == before
        report = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
                  "before": before, "after": after, "accepted": accepted,
                  "native_artifact": MANIFEST["native"], "prior_preparation": MANIFEST["preparation"],
                  "retained": retained, "observations": observations, "commands": COMMANDS,
                  "scope": MANIFEST["scope"]}
        save("parser-observed.json", report)
        print("QUOTES_PARSER_RESULT " + json.dumps(report), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
