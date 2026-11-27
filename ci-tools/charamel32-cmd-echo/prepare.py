"""Prove native-bound Cmd repairs, preserve existing controls, and format an isolated candidate."""
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
        print("ECHO_PREPARE_COMMAND " + json.dumps(row), flush=True)
        raise
    data = result.stdout + result.stderr
    (OUT / (label + ".log")).write_bytes(data)
    row = {"label": label, "argv": argv, "actual_exit": result.returncode, "timeout": False,
           "seconds": time.monotonic() - started, "log_sha256": digest(data)}
    COMMANDS.append(row)
    save("commands.json", COMMANDS)
    print("ECHO_PREPARE_COMMAND " + json.dumps(row), flush=True)
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
    for archive in MANIFEST["archives"]:
        files = archive_files(archive)
        report: dict[str, Any] = json.loads(file_named(files, archive["report"]))
        assert report["producer"] == archive["producer"]
        assert str(report["run"]) == str(archive["run"])
        assert report["identity"]["head"] == MANIFEST["head"]
        assert report["identity"]["tree"] == MANIFEST["tree"]
        assert report["identity"]["source_sha256"] == MANIFEST["source_sha256"]
        assert report["identity"]["source_clean"] is True
        assert report["before"] == report["identity"]
        assert report["failures"] == [] and len(report["records"]) == archive["records"]
        frozen: dict[str, Any] = json.loads(
            (ROOT / ("native-" + str(archive["records"]) + "-observed.json")).read_text("utf-8"))
        assert report == frozen
        save("authenticated-native-" + str(archive["records"]) + ".json", report)
        native.append(report)
    previous = MANIFEST["retained_preparation"]
    files = archive_files(previous)
    result: dict[str, Any] = json.loads(file_named(files, "prepare-result.json"))
    cases: list[dict[str, Any]] = json.loads(file_named(files, "candidate-classifier.json"))
    assert result["producer"] == previous["producer"] and str(result["run"]) == str(previous["run"])
    assert result["formatted_tree"] == MANIFEST["tree"] == previous["tree"]
    assert result["candidate_cases"] == MANIFEST["retained_cases"] == 124
    assert result["candidate_all_pass"] is True and result["source_restored"] is True
    keys = ("id", "dialect", "command", "expected")
    assert [[row[key] for key in keys] for row in cases] == [
        [row[key] for key in keys] for row in MANIFEST["cases"][:124]]
    assert all(row["actual_exit"] == 0 for row in cases)
    save("authenticated-retained-preparation.json", result)
    return native


def compile_probe(label: str, switch: str, directory: Path, trace: bool = False) -> Path:
    directory.mkdir(parents=True)
    source = (SOURCE / "bin/crush/lib/shell_screen.ml").read_text("utf-8")
    interface = (SOURCE / "bin/crush/lib/shell_screen.mli").read_text("utf-8")
    if trace:
        needle = "let rec parse_cmd sink ~depth source =\n"
        assert source.count(needle) == 1
        source = source.replace(
            needle, "let charamel_review_trace = ref []\n\n" + needle
            + "  charamel_review_trace := (depth, source) :: !charamel_review_trace;\n", 1)
        interface += "\nval charamel_review_trace : (int * string) list ref\n"
        probe = (
            'let () =\n'
            '  let kind = match Shell_screen.screen Shell_screen.Cmd Sys.argv.(1) with\n'
            '    | Shell_screen.Clear -> "clear"\n'
            '    | Shell_screen.Blocked _ -> "blocked"\n'
            '    | Shell_screen.Review _ -> "review" in\n'
            '  let output = Buffer.create 256 in\n'
            '  List.iter (fun (depth, source) ->\n'
            '    Printf.bprintf output "%d:" depth;\n'
            '    String.iter (fun c -> Printf.bprintf output "%02x" (Char.code c)) source;\n'
            "    Buffer.add_char output '\\n')\n"
            '    (List.rev !(Shell_screen.charamel_review_trace));\n'
            '  let rendered = Buffer.contents output in\n'
            '  Printf.printf "%s\\n%s" kind rendered;\n'
            '  if rendered <> Sys.argv.(2) || (Sys.argv.(3) <> "*" && kind <> Sys.argv.(3)) then exit 1\n'
        )
    else:
        probe = (
            'let () =\n'
            '  let dialect = if Sys.argv.(1) = "cmd" then Shell_screen.Cmd else Shell_screen.Posix in\n'
            '  let kind = match Shell_screen.screen dialect Sys.argv.(2) with\n'
            '    | Shell_screen.Clear -> "clear"\n'
            '    | Shell_screen.Blocked _ -> "blocked"\n'
            '    | Shell_screen.Review _ -> "review" in\n'
            '  print_endline kind;\n'
            '  if kind <> Sys.argv.(3) then exit 1\n'
        )
    (directory / "shell_screen.ml").write_text(source, encoding="utf-8")
    (directory / "shell_screen.mli").write_text(interface, encoding="utf-8")
    (directory / "probe.ml").write_text(probe, encoding="utf-8")
    save(label + "-source.json", {
        "production_sha256": digest((SOURCE / "bin/crush/lib/shell_screen.ml").read_bytes()),
        "compiled_source_sha256": digest(source.encode("utf-8")), "trace_copy": trace,
        "interface_sha256": digest(interface.encode("utf-8")),
        "probe_sha256": digest(probe.encode("utf-8")),
    })
    binary = directory / "screen-probe"
    result = run(label, [
        "opam", "exec", "--switch=" + switch, "--", "ocamlc",
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
        print("ECHO_CLASSIFIER_" + label.upper() + " " + json.dumps(row), flush=True)
    save(label + "-classifier.json", rows)
    return rows


def traces(label: str, binary: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for case in MANIFEST["traces"]:
        expected = "".join(str(row["depth"]) + ":" + row["source"].encode("utf-8").hex() + "\n"
                           for row in case["expected_trace"])
        checked_kind = case["expected_kind"] if label in ("candidate", "restored") else "*"
        result = subprocess.run(
            [str(binary), case["command"], expected, checked_kind],
            capture_output=True, timeout=10, check=False)
        lines = result.stdout.decode("utf-8").splitlines()
        actual: list[dict[str, Any]] = []
        for line in lines[1:]:
            depth, encoded = line.split(":", 1)
            actual.append({"depth": int(depth), "source": bytes.fromhex(encoded).decode("utf-8")})
        row = {**case, "actual_exit": result.returncode, "checked_kind": checked_kind,
               "actual_kind": lines[0] if lines else None, "actual_trace": actual,
               "stderr": result.stderr.decode("utf-8", errors="backslashreplace")}
        assert result.returncode in (0, 1), row
        rows.append(row)
        print("ECHO_TRACE_" + label.upper() + " " + json.dumps(row), flush=True)
    save(label + "-trace.json", rows)
    return rows


def require_red(rows: list[dict[str, Any]], expected: list[str]) -> None:
    by_id = {row["id"]: row for row in rows}
    for key in expected:
        assert by_id[key]["actual_exit"] == 1, (key, by_id[key])


def require_retained(rows: list[dict[str, Any]]) -> None:
    assert len(rows) == len(MANIFEST["cases"])
    assert all(row["actual_exit"] == 0 for row in rows[:MANIFEST["retained_cases"]])


def apply(elements: list[dict[str, str]]) -> None:
    for element in elements:
        data = element["content"].encode("utf-8")
        assert digest(data) == element["sha256"], element["path"]
        (SOURCE / element["path"]).write_bytes(data)
    assert git("add", "--", *[element["path"] for element in elements]) == ""


def stage(label: str, switch: str, scratch: Path) -> dict[str, Any]:
    binary = compile_probe(label + "-compile", switch, scratch / (label + "-plain"))
    trace_binary = compile_probe(label + "-trace-compile", switch, scratch / (label + "-trace"), True)
    return {"classifier": classify(label, binary), "trace": traces(label, trace_binary),
            "tree": git("write-tree")}


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("rev-parse", "HEAD^{tree}") == MANIFEST["tree"]
    assert git("status", "--porcelain") == ""
    for path, expected in MANIFEST["source_sha256"].items():
        assert digest((SOURCE / path).read_bytes()) == expected
    save("initial-source.json", {"head": MANIFEST["head"], "tree": MANIFEST["tree"],
                                "producer": os.environ["GITHUB_SHA"]})
    native = authenticate()
    switch = subprocess.check_output(["opam", "switch", "show"], cwd=WORKSPACE, text=True).strip()
    assert run("compiler-version", ["opam", "exec", "--switch=" + switch, "--", "ocamlc", "-version"], SOURCE).stdout.strip() == b"5.5.1"
    assert run("formatter-version", ["opam", "exec", "--switch=" + switch, "--", "ocamlformat", "--version"], SOURCE).stdout.strip() == b"0.29.0"
    first: list[dict[str, str]] = json.loads((ROOT / "stage1-elements.json").read_text("utf-8"))
    final_elements: list[dict[str, str]] = json.loads((ROOT / "candidate-elements.json").read_text("utf-8"))
    assert len(first) == 1 and first[0]["sha256"] == MANIFEST["stage1_sha256"]
    states: dict[str, dict[str, Any]] = {}
    final: dict[str, Any] = {}
    accepted = False
    try:
        with tempfile.TemporaryDirectory(prefix="charamel32-echo-compile-") as temporary:
            scratch = Path(temporary)
            states["baseline"] = stage("baseline", switch, scratch)
            require_retained(states["baseline"]["classifier"])
            require_red(states["baseline"]["classifier"], MANIFEST["must_fail"])
            require_red(states["baseline"]["trace"], MANIFEST["trace_must_fail"])
            old_trace = {row["id"]: row for row in states["baseline"]["trace"]}
            assert all(old_trace[key]["actual_exit"] == 0 for key in MANIFEST["trace_old_controls"])
            apply(first)
            assert git("write-tree") == MANIFEST["stage1_tree"]
            states["payload"] = stage("payload", switch, scratch)
            require_retained(states["payload"]["classifier"])
            require_red(states["payload"]["classifier"], MANIFEST["must_fail"])
            assert all(row["actual_exit"] == 0 for row in states["payload"]["trace"])
            apply(final_elements)
            assert git("write-tree") == MANIFEST["draft_tree"]
            paths = [element["path"] for element in final_elements]
            assert run("format-candidate", ["opam", "exec", "--switch=" + switch, "--", "ocamlformat", "-i", *paths], SOURCE).returncode == 0
            assert git("add", "--", *paths) == ""
            tree = git("write-tree")
            formatted = [{"path": path, "mode": "100644",
                          "content": (SOURCE / path).read_text("utf-8"),
                          "sha256": digest((SOURCE / path).read_bytes())} for path in paths]
            final = {"head": MANIFEST["head"], "tree": tree, "elements": formatted}
            save("formatted-candidate.json", final)
            print("ECHO_FORMATTED_CANDIDATE " + json.dumps(final), flush=True)
            states["candidate"] = stage("candidate", switch, scratch)
            assert all(row["actual_exit"] == 0 for row in states["candidate"]["classifier"])
            assert all(row["actual_exit"] == 0 for row in states["candidate"]["trace"])
            apply(first)
            states["echo_mutant"] = stage("echo_mutant", switch, scratch)
            require_retained(states["echo_mutant"]["classifier"])
            require_red(states["echo_mutant"]["classifier"], MANIFEST["must_fail"])
            assert all(row["actual_exit"] == 0 for row in states["echo_mutant"]["trace"])
            apply(formatted)
            assert git("write-tree") == tree
            states["restored"] = stage("restored", switch, scratch)
            assert all(row["actual_exit"] == 0 for row in states["restored"]["classifier"])
            assert all(row["actual_exit"] == 0 for row in states["restored"]["trace"])
            assert git("diff", "--exit-code") == ""
            assert git("ls-files", "--others", "--exclude-standard") == ""
            assert git("write-tree") == tree
            accepted = True
    finally:
        restore = run("restore-original", ["git", "reset", "--hard", MANIFEST["head"]], SOURCE)
        assert restore.returncode == 0
        assert git("rev-parse", "HEAD^{tree}") == MANIFEST["tree"]
        assert git("status", "--porcelain") == ""
        summaries = {
            name: {"tree": state["tree"], "classifier_cases": len(state["classifier"]),
                   "classifier_failures": [row["id"] for row in state["classifier"] if row["actual_exit"]],
                   "trace_cases": len(state["trace"]),
                   "trace_failures": [row["id"] for row in state["trace"] if row["actual_exit"]]}
            for name, state in states.items()
        }
        result = {"producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
                  "head": MANIFEST["head"], "original_tree": MANIFEST["tree"],
                  "formatted_tree": final.get("tree"), "accepted": accepted,
                  "native_counts": [len(report["records"]) for report in native],
                  "source_restored": True, "stages": summaries, "commands": COMMANDS}
        save("prepare-result.json", result)
        print("ECHO_PREPARE_RESULT " + json.dumps(result), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
