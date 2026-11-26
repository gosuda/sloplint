from __future__ import annotations

from collections import defaultdict
import hashlib
import io
import json
import os
from pathlib import Path
import urllib.error
import urllib.request
import zipfile
from typing import cast

proof = Path(__file__).resolve().parent
artifacts = cast(list[dict[str, str | int]], json.loads((proof / "artifacts.json").read_text()))
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-sheet-artifact-summary"
evidence.mkdir()
token = os.environ["GH_TOKEN"]
summaries: list[dict[str, object]] = []

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req: urllib.request.Request, fp: io.BufferedIOBase, code: int,
                         msg: str, headers: object, newurl: str) -> None:
        return None

def fetch(artifact: dict[str, str | int]) -> Path:
    request = urllib.request.Request(
        f"https://api.github.com/repos/gosuda/sloplint/actions/artifacts/{artifact['id']}/zip",
        headers={"Authorization": "Bearer " + token, "Accept": "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28"})
    try:
        response = urllib.request.build_opener(NoRedirect).open(request, timeout=60)
    except urllib.error.HTTPError as error:
        if error.code != 302:
            raise
        location = error.headers["Location"]
        assert location.startswith("https://")
        response = urllib.request.urlopen(location, timeout=120)
    path = evidence / f"{artifact['id']}.zip"
    with response, path.open("wb") as output:
        while chunk := response.read(1024 * 1024):
            output.write(chunk)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == artifact["sha256"]
    return path

def objects(archive: zipfile.ZipFile, name: str) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    with archive.open(name) as raw, io.TextIOWrapper(raw, encoding="utf-8", errors="replace") as lines:
        for line in lines:
            if not line.startswith("{"):
                continue
            try:
                rows.append(cast(dict[str, object], json.loads(line)))
            except json.JSONDecodeError:
                continue
    return rows

for artifact in artifacts:
    path = fetch(artifact)
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        prepared = next(name for name in names if name.endswith("prepared.json"))
        setup = cast(dict[str, object], json.loads(archive.read(prepared)))
        source_manifest = cast(dict[str, object], setup["manifest"])
        assert source_manifest["candidate"] == "fa3138995337182fa03087ec0f3514d3fda87a3e"
        assert source_manifest["tree"] == "f485609d7cdd5a9bda962ce964643adeae868bfb"
        assert cast(dict[str, object], setup["identity"])["index_tree"] == source_manifest["tree"]
        commands: list[dict[str, object]] = []
        for name in names:
            if not name.endswith(".json") or name.endswith("-test-events.json"):
                continue
            data = json.loads(archive.read(name))
            if not isinstance(data, dict) or "exit_code" not in data or "command" not in data:
                continue
            state = cast(dict[str, object], data)
            commands.append({key: state[key] for key in ("name", "exit_code", "elapsed_seconds", "test_events", "environment_overrides") if key in state})
            if state["exit_code"] == 0:
                continue
            log_name = name[:-5] + ".log"
            rows = objects(archive, log_name)
            failures = [(str(row.get("Package", "")), str(row.get("Test", "")))
                        for row in rows if row.get("Action") == "fail" and row.get("Test")]
            if failures:
                for package, test in failures:
                    output = "".join(str(row.get("Output", "")) for row in rows
                                     if row.get("Package") == package and
                                     (row.get("Test") == test or str(row.get("Test", "")).startswith(test + "/")))
                    print("FAILED_TEST " + json.dumps({"artifact": artifact["id"], "command": state["name"],
                          "package": package, "test": test, "output": output[-18000:]}), flush=True)
            else:
                output = archive.read(log_name).decode("utf-8", errors="replace")
                print("FAILED_COMMAND " + json.dumps({"artifact": artifact["id"], "command": state["name"],
                      "exit": state["exit_code"], "output": output[-18000:]}), flush=True)
        summary = {"artifact": artifact, "identity": setup["identity"], "resources": setup["resources"],
                   "runner": setup["runner"], "commands": commands}
        summaries.append(summary)
        print("LANE_SUMMARY " + json.dumps({"artifact": artifact["id"], "name": artifact["name"],
              "runner": setup["runner"], "resources": setup["resources"], "commands": commands}), flush=True)
    path.unlink()
(evidence / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")
print("ARTIFACT_SUMMARY_COMPLETE " + str(len(summaries)), flush=True)
