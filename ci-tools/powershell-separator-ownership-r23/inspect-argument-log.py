from __future__ import annotations

import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import zipfile

root = Path(os.environ["RUNNER_TEMP"]) / "powershell-separator-ownership-r23/argument-log"
root.mkdir(exist_ok=True, parents=True)
archives = [path for path in root.rglob("*") if path.is_file() and zipfile.is_zipfile(path)]
assert len(archives) == 1
archive = archives[0]
assert hashlib.sha256(archive.read_bytes()).hexdigest() == "e9321f407ec0417aba597885bbdfa6e5eb2508d37fdc7157f7aa1052958861bd"
with zipfile.ZipFile(archive) as handle:
    def member(name: str) -> bytes:
        names = [path for path in handle.namelist() if Path(path).name == name]
        assert len(names) == 1, (name, names)
        return handle.read(names[0])
    packed = member("experiment-packed.json.gz")
    assert hashlib.sha256(packed).hexdigest() == "4f9b7c390dd938ce2391163284911a8d961a47c218813d56887c355cd71ce4f1"
    payload = gzip.decompress(packed)
    assert hashlib.sha256(payload).hexdigest() == "db94e0276d22a8ec1cb27f5c73acece3366b6d8d607c368c4794f27edfa6442c"
    data = json.loads(payload)
    result = next(item for item in data["results"] if item["name"] == "last-comment-and-redirect-guards-focused-powershell")
    assert result["exit"] == 0 and not result["timed_out"]
    log = member(result["name"] + ".log")
    assert hashlib.sha256(log).hexdigest() == result["log_sha256"]
text = log.decode()
lines = text.splitlines()
matches = re.findall(r"\bGUARD_NATIVE_ARG_CASE ([a-z0-9-]+) ([0-9]+)(?=\s|$)", text)
report = {"producer": "519e18f939cb8a9796f81ac399096083dbb7995c", "run": 45913796869, "artifact": 15160844099,
          "zip_sha256": "e9321f407ec0417aba597885bbdfa6e5eb2508d37fdc7157f7aa1052958861bd",
          "source": data["outcomes"][0]["source"], "focused_result": result,
          "line_prefix_count": sum(line.startswith("GUARD_NATIVE_ARG_CASE ") for line in lines),
          "complete_matches": matches, "marker_lines": [line for line in lines if "GUARD" in line or "comment-guard-" in line]}
print("ARGUMENT_LOG_INSPECTION " + json.dumps(report), flush=True)
(root / "inspection.json").write_text(json.dumps(report, indent=2) + "\n")
(root / "focused-original.log").write_bytes(log)
encoded = base64.b64encode(gzip.compress(log, mtime=0)).decode()
print("FOCUSED_RAW_LOG_IDENTITY " + json.dumps({"sha256": result["log_sha256"], "bytes": len(log), "base64_bytes": len(encoded)}), flush=True)
for part, offset in enumerate(range(0, len(encoded), 16000)):
    print("FOCUSED_RAW_LOG_CHUNK " + json.dumps({"part": part, "data": encoded[offset:offset + 16000]}), flush=True)
