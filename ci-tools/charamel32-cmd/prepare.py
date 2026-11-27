"""Compile baseline controls and format an isolated, exact Charamel candidate."""
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
import urllib.error
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent
WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
OUT = WORKSPACE / "evidence"
COMMANDS: list[dict[str, Any]] = []


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


def run(label: str, args: list[str], cwd: Path) -> subprocess.CompletedProcess[bytes]:
    started = time.monotonic()
    result = subprocess.run(args, cwd=cwd, capture_output=True, check=False, timeout=300)
    path = OUT / (label + ".log")
    path.write_bytes(result.stdout + result.stderr)
    record = {
        "label": label, "argv": args, "cwd": str(cwd), "actual_exit": result.returncode,
        "seconds": time.monotonic() - started, "log_sha256": digest(path.read_bytes()),
    }
    COMMANDS.append(record)
    print("PREPARE_COMMAND " + json.dumps(record), flush=True)
    if result.returncode:
        print(path.read_text("utf-8", errors="backslashreplace")[-24000:], flush=True)
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


def authenticate(archive: dict[str, Any]) -> dict[str, Any]:
    request = urllib.request.Request(
        f'https://api.github.com/repos/gosuda/sloplint/actions/artifacts/{archive["id"]}/zip',
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/vnd.github+json"},
    )
    opener = urllib.request.build_opener(ArtifactRedirect())
    with opener.open(request, timeout=90) as response:
        data = response.read()
    assert digest(data) == archive["sha256"], archive["id"]
    with zipfile.ZipFile(io.BytesIO(data)) as contents:
        names = [name for name in contents.namelist() if name.endswith(archive["report"])]
        assert len(names) == 1
        report: dict[str, Any] = json.loads(contents.read(names[0]))
    assert report["producer"] == archive["producer"]
    assert report["source"] == archive["source"]
    assert report["source_clean"] is True
    assert len(report["records"]) == archive["records"]
    assert report["failures"] == archive["failures"]
    (OUT / ("authenticated-" + archive["report"])).write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    return report


def compile_probe(label: str, switch: str, directory: Path) -> Path:
    directory.mkdir(parents=True)
    for name in ("shell_screen.mli", "shell_screen.ml"):
        shutil.copyfile(SOURCE / "bin/crush/lib" / name, directory / name)
    (directory / "probe.ml").write_text(
        'let () =\n'
        '  let dialect = if Sys.argv.(1) = "cmd" then Shell_screen.Cmd else Shell_screen.Posix in\n'
        '  let kind = match Shell_screen.screen dialect Sys.argv.(2) with\n'
        '    | Shell_screen.Clear -> "clear"\n'
        '    | Shell_screen.Blocked _ -> "blocked"\n'
        '    | Shell_screen.Review _ -> "review" in\n'
        '  print_endline kind;\n'
        '  if kind <> Sys.argv.(3) then exit 1\n',
        encoding="utf-8",
    )
    binary = directory / "screen-probe"
    result = run(label, [
        "opam", "exec", "--switch=" + switch, "--", "ocamlc",
        "-strict-sequence", "-strict-formats", "-short-paths", "-principal",
        "-warn-error", "+a", "-o", str(binary),
        "shell_screen.mli", "shell_screen.ml", "probe.ml",
    ], directory)
    assert result.returncode == 0, label
    return binary


def classifications(
    label: str, binary: Path, cases: list[dict[str, str]],
) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for case in cases:
        result = subprocess.run(
            [str(binary), case["dialect"], case["command"], case["expected"]],
            capture_output=True, timeout=10, check=False,
        )
        record = {
            **case, "actual_exit": result.returncode,
            "actual_kind": result.stdout.decode("utf-8").strip(),
            "stderr": result.stderr.decode("utf-8", errors="backslashreplace"),
        }
        assert result.returncode in (0, 1), record
        records.append(record)
        print("CLASSIFIER_" + label.upper() + " " + json.dumps(record), flush=True)
    (OUT / (label + "-classifier.json")).write_text(
        json.dumps(records, indent=2) + "\n", encoding="utf-8"
    )
    return records


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, Any] = json.loads((ROOT / "prepare.json").read_text("utf-8"))
    payload: list[dict[str, str]] = json.loads((ROOT / "candidate-elements.json").read_text("utf-8"))
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("rev-parse", "HEAD^{tree}") == manifest["tree"]
    assert git("status", "--porcelain") == ""
    (OUT / "preflight.json").write_text(
        json.dumps({"source": manifest["head"], "tree": manifest["tree"],
                    "producer": os.environ["GITHUB_SHA"], "source_clean": True}) + "\n",
        encoding="utf-8",
    )
    archives = [authenticate(item) for item in manifest["archives"]]
    switch = subprocess.check_output(
        ["opam", "switch", "show"], cwd=WORKSPACE, text=True
    ).strip()
    assert run("compiler-version", ["opam", "exec", "--switch=" + switch, "--", "ocamlc", "-version"], SOURCE).stdout.strip() == b"5.5.1"
    assert run("formatter-version", ["opam", "exec", "--switch=" + switch, "--", "ocamlformat", "--version"], SOURCE).stdout.strip() == b"0.29.0"
    baseline: list[dict[str, Any]] = []
    candidate: list[dict[str, Any]] = []
    final: dict[str, Any] = {}
    try:
        with tempfile.TemporaryDirectory(prefix="charamel32-compile-") as temporary:
            scratch = Path(temporary)
            old_binary = compile_probe("baseline-compile", switch, scratch / "old")
            baseline = classifications("baseline", old_binary, manifest["cases"])
            for wanted in manifest["must_fail"]:
                found = [r for r in baseline if r["dialect"] == wanted["dialect"] and r["command"] == wanted["command"]]
                assert len(found) == 1 and found[0]["actual_exit"] == 1, wanted
            for element in payload:
                data = element["content"].encode("utf-8")
                assert digest(data) == element["sha256"], element["path"]
                (SOURCE / element["path"]).write_bytes(data)
            paths = [element["path"] for element in payload]
            assert git("add", "--", *paths) == ""
            assert git("write-tree") == manifest["draft_tree"]
            formatted = run("format-authored-candidate", [
                "opam", "exec", "--switch=" + switch, "--", "ocamlformat", "-i", *paths
            ], SOURCE)
            assert formatted.returncode == 0
            assert git("add", "--", *paths) == ""
            final_tree = git("write-tree")
            final_elements = []
            for path in paths:
                content = (SOURCE / path).read_bytes()
                final_elements.append({"path": path, "mode": "100644", "content": content.decode("utf-8"),
                                       "sha256": digest(content)})
            final = {"head": manifest["head"], "tree": final_tree, "elements": final_elements}
            (OUT / "formatted-candidate.json").write_text(
                json.dumps(final, indent=2) + "\n", encoding="utf-8"
            )
            print("FORMATTED_CANDIDATE " + json.dumps(final), flush=True)
            binary = compile_probe("candidate-compile", switch, scratch / "candidate")
            candidate = classifications("candidate", binary, manifest["cases"])
            assert all(row["actual_exit"] == 0 for row in candidate), "candidate classifier mismatch"
            assert git("diff", "--exit-code") == ""
            assert git("write-tree") == final_tree
    finally:
        restored = run("restore-original", ["git", "reset", "--hard", manifest["head"]], SOURCE)
        assert restored.returncode == 0
        assert git("rev-parse", "HEAD^{tree}") == manifest["tree"]
        assert git("status", "--porcelain") == ""
        result = {
            "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
            "head": manifest["head"], "original_tree": manifest["tree"],
            "formatted_tree": final.get("tree"),
            "native_archives": manifest["archives"], "native_record_counts": [len(a["records"]) for a in archives],
            "baseline_cases": len(baseline),
            "baseline_actual_failures": sum(row["actual_exit"] != 0 for row in baseline),
            "candidate_cases": len(candidate),
            "candidate_actual_failures": sum(row["actual_exit"] != 0 for row in candidate),
            "candidate_all_pass": len(candidate) == len(manifest["cases"]) and all(row["actual_exit"] == 0 for row in candidate),
            "source_restored": True, "commands": COMMANDS,
        }
        (OUT / "prepare-result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print("PREPARE_RESULT " + json.dumps(result), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
