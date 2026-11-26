from __future__ import annotations

import base64
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import urllib.error
import urllib.request
import zipfile
from typing import cast

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROOF = WORKSPACE / "proof/ci-tools/powershell-terminator-controls-r20"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "powershell-terminator-controls-r20"
EVIDENCE.mkdir()
MANIFEST = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
VENDOR = SOURCE / "vendor/tree-sitter-powershell"
ENVIRONMENT = dict(os.environ)
ENVIRONMENT["GIT_INDEX_FILE"] = str(EVIDENCE / "candidate.index")
RESULTS: list[dict[str, object]] = []

def output(args: list[str], cwd: Path = SOURCE) -> str:
    return subprocess.check_output(args, cwd=cwd, env=ENVIRONMENT, text=True).strip()

def run(name: str, args: list[str], cwd: Path = SOURCE) -> str:
    result = subprocess.run(args, cwd=cwd, env=ENVIRONMENT, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=300, check=False)
    (EVIDENCE / f"{name}.log").write_text(result.stdout)
    RESULTS.append({"name": name, "exit": result.returncode,
                    "log_sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    print(f"CONTROL_EXIT {name} {result.returncode}\n{result.stdout[-50000:]}", flush=True)
    assert result.returncode == 0, name
    return result.stdout

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req: urllib.request.Request, fp: object, code: int, msg: str, headers: object, newurl: str) -> None:
        return None

def api_request(url: str) -> urllib.request.Request:
    assert url.startswith("https://api.github.com/repos/gosuda/sloplint/")
    return urllib.request.Request(url, headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                                               "Accept": "application/vnd.github+json"})

def download(url: str, digest: str, name: str) -> Path:
    path = EVIDENCE / name
    with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    return path

for name, digest in cast(dict[str, str], MANIFEST["proof_sha256"]).items():
    assert hashlib.sha256((PROOF / name).read_bytes()).hexdigest() == digest, name
artifact_api = "https://api.github.com/repos/gosuda/sloplint/actions/artifacts/15143209008"
with urllib.request.urlopen(api_request(artifact_api), timeout=60) as response:
    metadata = json.load(response)
assert metadata["id"] == 15143209008
assert metadata["digest"] == "sha256:" + str(MANIFEST["zip_sha256"])
assert metadata["workflow_run"]["id"] == 45873249049
assert metadata["workflow_run"]["head_sha"] == "58f6f32eedf61e5bc2d1ddab3dae4aaf1b3722a1"
try:
    urllib.request.build_opener(NoRedirect).open(api_request(artifact_api + "/zip"), timeout=60)
except urllib.error.HTTPError as error:
    assert error.code == 302, error.code
    location = error.headers["Location"]
else:
    raise AssertionError("Artifact API did not return the expected download redirect")
assert location.startswith("https://")
archive = download(location, str(MANIFEST["zip_sha256"]), "candidate.zip")
with zipfile.ZipFile(io.BytesIO(archive.read_bytes())) as zipped:
    data = zipped.read("stages.json")
assert hashlib.sha256(data).hexdigest() == MANIFEST["stages_sha256"]
payload = cast(dict[str, object], json.loads(data))
stages = cast(list[dict[str, object]], payload["stages"])
assert stages[-1]["tree"] == "67485931a856bf4d46f79e14cd1c8885f8fa3105"
packed = gzip.compress(data, mtime=0)
(EVIDENCE / "stages.json.gz").write_bytes(packed)
encoded = base64.b64encode(packed).decode("ascii")
print("PACKED_SOURCE_IDENTITY " + json.dumps({"artifact": metadata, "payload_sha256": hashlib.sha256(data).hexdigest(),
      "gzip_sha256": hashlib.sha256(packed).hexdigest(), "gzip_bytes": len(packed),
      "payload_bytes": len(data), "base64_bytes": len(encoded), "chunks": (len(encoded) + 23999) // 24000}), flush=True)
for part, offset in enumerate(range(0, len(encoded), 24000)):
    print("PACKED_SOURCE_CHUNK " + json.dumps({"part": part, "content": encoded[offset:offset + 24000]}), flush=True)

assert output(["git", "rev-parse", "HEAD"]) == MANIFEST["base"]
assert output(["git", "status", "--porcelain", "--untracked-files=all"]) == ""
archive = download("https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.1/tree-sitter-linux-x64.gz",
                   "d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d", "tree-sitter.gz")
binary = EVIDENCE / "tree-sitter"
binary.write_bytes(gzip.decompress(archive.read_bytes()))
binary.chmod(0o755)
archive = download("https://github.com/PowerShell/PowerShell/releases/download/v7.7.0/powershell-7.7.0-linux-x64.tar.gz",
                   "c89d9c682d2c403dfe726f3df948e359675235a5bd6fab535776de69729ca0cb", "powershell.tar.gz")
powershell = EVIDENCE / "powershell"
powershell.mkdir()
run("extract-powershell", ["tar", "-xzf", str(archive), "-C", str(powershell)])
pwsh = powershell / "pwsh"
pwsh.chmod(0o755)
text = run("native-terminator-controls", [str(pwsh), "-NoLogo", "-NoProfile", "-File", str(PROOF / "oracle.ps1"), "-CasesPath", str(PROOF / "cases.json")])
native = [json.loads(line.removeprefix("TOKEN_CASE ")) for line in text.splitlines() if line.startswith("TOKEN_CASE ")]
cases = cast(list[dict[str, object]], json.loads((PROOF / "cases.json").read_text()))
assert len(native) == len(cases) == 24
for expected, actual in zip(cases, native, strict=True):
    assert expected["id"] == actual["id"] and expected["accepted"] == actual["accepted"]
    assert sum(item["clauses"] - 1 for item in actual["branches"]) == expected["elseifs"]
    assert sum(int(item["has_else"]) for item in actual["branches"]) == expected["elses"]
(EVIDENCE / "native-controls.json").write_text(json.dumps(native, indent=2) + "\n")

def compare(label: str) -> None:
    records = []
    case_root = EVIDENCE / "cases"
    case_root.mkdir(exist_ok=True)
    for index, case in enumerate(cases):
        content = str(case["source"])
        assert hashlib.sha256(content.encode()).hexdigest() == case["source_sha256"]
        path = case_root / f"case-{index}.ps1"
        path.write_text(content)
        result = subprocess.run([str(binary), "parse", str(path)], cwd=VENDOR, env=ENVIRONMENT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
        assert "Failed to load language" not in result.stdout
        record = {"id": case["id"], "exit": result.returncode,
                  "elseifs": len(re.findall(r"\(elseif_clause ", result.stdout)),
                  "elses": len(re.findall(r"\(else_clause ", result.stdout))}
        record["matches"] = result.returncode == 0 and record["elseifs"] == case["elseifs"] and record["elses"] == case["elses"]
        records.append(record)
        print("TERMINATOR_CONTROL " + json.dumps({"stage": label, **record}), flush=True)
        if not record["matches"]:
            print(result.stdout, flush=True)
    (EVIDENCE / f"{label}-controls.json").write_text(json.dumps(records, indent=2) + "\n")

compare("baseline")
for stage in stages:
    for item in cast(list[dict[str, str]], stage["elements"]):
        assert hashlib.sha256(item["content"].encode()).hexdigest() == item["sha256"]
        path = SOURCE / item["path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(item["content"])
output(["git", "read-tree", "HEAD"])
output(["git", "add", "--", *cast(list[str], stages[-1]["paths"])])
assert output(["git", "write-tree"]) == stages[-1]["tree"]
compare("candidate")
for entry in subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=SOURCE, env=ENVIRONMENT).split(b"\0"):
    if not entry:
        continue
    metadata, relative = entry.split(b"\t", 1)
    mode, expected, stage = metadata.split()
    assert stage == b"0"
    path = SOURCE / os.fsdecode(relative)
    content = os.fsencode(os.readlink(path)) if mode == b"120000" else path.read_bytes()
    assert hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest() == expected.decode(), str(path)
(EVIDENCE / "results.json").write_text(json.dumps({"manifest": MANIFEST, "results": RESULTS}, indent=2) + "\n")
print("TERMINATOR_CONTROL_OBSERVATIONS_COMPLETE " + str(stages[-1]["tree"]), flush=True)
