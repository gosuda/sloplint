from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import cast

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "probe" / "ci-tools" / "spira-merge"
EVIDENCE = SOURCE.parent / "evidence"
MANIFEST = cast(dict[str, object], json.loads((PROOF / "manifest.json").read_text()))
FILES = cast(dict[str, str], MANIFEST["files"])
RESULTS: dict[str, int] = {}

def git(*args: str, cwd: Path = SOURCE) -> str:
    return subprocess.check_output(["git", *args], cwd=cwd, text=True).strip()

def run(name: str, args: list[str], required: str = "") -> str:
    print(f"RUN {name}: {args!r}", flush=True)
    result = subprocess.run(args, cwd=SOURCE, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=3600)
    (EVIDENCE / f"{name}.log").write_text(result.stdout)
    RESULTS[name] = result.returncode
    (EVIDENCE / "results.json").write_text(json.dumps({"manifest": MANIFEST, "exits": RESULTS}, indent=2) + "\n")
    print(f"EXIT {name}: {result.returncode}\n{result.stdout[-30000:]}", flush=True)
    assert result.returncode == 0, (name, result.returncode)
    if required:
        assert required in result.stdout, (name, required)
    return result.stdout

def identity() -> None:
    assert git("rev-parse", "HEAD") == MANIFEST["head"]
    assert git("write-tree") == MANIFEST["tree"]
    for path, digest in FILES.items():
        assert hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() == digest, path
    test = (SOURCE / "http3/http3_test.lgo").read_text()
    added = test[test.index("func openH3PartialUni("):test.index("type h3ConflictProbe struct {")]
    assert hashlib.sha256(added.encode()).hexdigest() == MANIFEST["feature_sha256"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=SOURCE, check=True)
    logos = Path(os.environ["LOGOS_ROOT"])
    assert git("rev-parse", "HEAD", cwd=logos) == MANIFEST["logos"]
    assert git("rev-parse", "HEAD^{tree}", cwd=logos) == MANIFEST["logos_tree"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=logos, check=True)
    print("SPIRA_MERGE_IDENTITY " + json.dumps(MANIFEST, sort_keys=True), flush=True)

assert git("rev-parse", "HEAD") == MANIFEST["head"]
assert git("status", "--porcelain") == ""
patch = PROOF / "candidate.patch"
assert hashlib.sha256(patch.read_bytes()).hexdigest() == MANIFEST["patch_sha256"]
run("apply-check", ["git", "apply", "--check", str(patch)])
run("apply", ["git", "apply", "--index", str(patch)])
identity()
run("stream-cap-repeated", ["logos", "test", "-race", "-count=3", "-v", "-run", "^TestH3PublicCapsIncomingUniStreamsAt100$", "./http3"], "--- PASS: TestH3PublicCapsIncomingUniStreamsAt100")
run("just-gate", ["just", "gate"])
identity()
run("just-eject", ["just", "eject"])
run("whitespace", ["git", "diff", "--cached", "--check"])
identity()
print("SPIRA_MERGE_GATES_COMPLETE " + json.dumps({"tree": MANIFEST["tree"], "exits": RESULTS}, sort_keys=True), flush=True)
