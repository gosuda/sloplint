"""Exact source and outcome identities for the frozen reference baseline."""
from __future__ import annotations

import hashlib
import json
import re
import subprocess

import baseline_support as base

def test_outcomes(log: str, test: str, names: set[str]) -> dict[str, list[str]]:
    started = set(re.findall(r"^=== RUN   " + re.escape(test) + r"/([a-z0-9-]+)$", log, re.MULTILINE))
    passed = set(re.findall(r"^\s*--- PASS: " + re.escape(test) + r"/([a-z0-9-]+) ", log, re.MULTILINE))
    failed = set(re.findall(r"^\s*--- FAIL: " + re.escape(test) + r"/([a-z0-9-]+) ", log, re.MULTILINE))
    if started != names or passed & failed or passed | failed != names:
        raise RuntimeError("missing or duplicate frozen test outcomes: " + test)
    return {"passed": sorted(passed), "failed": sorted(failed)}


def source_snapshot(manifest: dict[str, object], key: str) -> dict[str, object]:
    spec = base.object_value(manifest[key])
    entries: list[dict[str, str]] = json.loads((base.PROOF / base.text(spec["index_file"])).read_bytes())
    tracked = subprocess.run(["git", "ls-files", "-s", "-z"], cwd=base.SOURCE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout
    index: list[dict[str, str]] = []
    for entry in tracked.split(b"\0"):
        if not entry:
            continue
        header, path = entry.split(b"\t", 1)
        mode, blob, stage = header.decode().split()
        if stage != "0":
            raise RuntimeError("unmerged source index")
        index.append({"path": path.decode(), "mode": mode, "sha": blob})
    if sorted(index, key=lambda row: row["path"]) != sorted(entries, key=lambda row: row["path"]):
        raise RuntimeError("full native index differs: " + key)
    actual: list[dict[str, str]] = []
    for row in entries:
        path = base.SOURCE / row["path"]
        data = path.read_bytes()
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if blob != row["sha"] or bool(path.stat().st_mode & 0o111) != (row["mode"] == "100755"):
            raise RuntimeError("source bytes or mode differ: " + row["path"])
        actual.append({**row, "sha256": base.digest(data)})
    tree = subprocess.run(["git", "write-tree"], cwd=base.SOURCE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, check=True).stdout.decode().strip()
    if tree != spec["tree"] or len(actual) != spec["blobs"]:
        raise RuntimeError("native source tree/count differs")
    record: dict[str, object] = {"label": key, "tree": tree, "blobs": len(actual),
                                "index_sha256": base.digest(json.dumps(actual, sort_keys=True,
                                                             separators=(",", ":")).encode())}
    print("SPIRA_R42_SOURCE_IDENTITY " + json.dumps(record), flush=True)
    return record


