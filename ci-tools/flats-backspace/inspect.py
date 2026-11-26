from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import TypedDict, cast

class Source(TypedDict):
    name: str
    head: str
    tree: str

proof = Path(__file__).resolve().parent
workspace = Path(os.environ["GITHUB_WORKSPACE"])
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-codemirror-source"
evidence.mkdir(exist_ok=True)
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
records: list[dict[str, object]] = []
for source in cast(list[Source], manifest["sources"]):
    root = workspace / source["name"]
    def git(*args: str) -> str:
        return subprocess.check_output(["git", *args], cwd=root, text=True).strip()
    assert git("rev-parse", "HEAD") == source["head"]
    assert git("write-tree") == source["tree"]
    web = root / "internal/apps/docs/_web"
    result = subprocess.run(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"],
                            cwd=web, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            check=False, timeout=300)
    print(f"NPM_EXIT {source['name']} {result.returncode}\n{result.stdout[-3000:]}", flush=True)
    assert result.returncode == 0
    package = cast(dict[str, object], json.loads((web / "node_modules/@codemirror/view/package.json").read_text()))
    lock = cast(dict[str, object], json.loads((web / "package-lock.json").read_text()))
    packages = cast(dict[str, dict[str, object]], lock["packages"])
    raw = (web / "node_modules/@codemirror/view/dist/index.js").read_bytes()
    text = raw.decode()
    start = text.index("class DOMReader")
    end = text.index("\n}\n", start) + 3
    reader = text[start:end]
    (evidence / (source["name"] + "-DOMReader.js.txt")).write_text(reader)
    record = {"source": source, "version": package["version"], "source_sha256": hashlib.sha256(raw).hexdigest(),
              "reader_sha256": hashlib.sha256(reader.encode()).hexdigest(), "package": packages["node_modules/@codemirror/view"]}
    records.append(record)
    print("CODEMIRROR_IDENTITY " + json.dumps(record, sort_keys=True), flush=True)
    method = reader.index("    readRange(")
    method_end = reader.index("\n    }\n", method) + len("\n    }\n")
    print("DOMREADER_COMPLETE " + source["name"] + "\n" + reader, flush=True)
    for term in ("function isEmptyToEnd(", "new DOMReader("):
        pos = text.index(term)
        print("DOM_READER_CONTEXT " + source["name"] + " " + term + "\n" + text[max(0, pos-600):pos+1800], flush=True)
    assert git("status", "--porcelain", "--untracked-files=all") == ""
    assert git("write-tree") == source["tree"]
(evidence / "identities.json").write_text(json.dumps(records, indent=2) + "\n")
print("CODEMIRROR_SOURCE_INSPECTION_COMPLETE", flush=True)
