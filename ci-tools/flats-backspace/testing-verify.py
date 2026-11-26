from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from typing import cast

proof = Path(__file__).resolve().parent
root = Path(os.environ["GITHUB_WORKSPACE"]) / "candidate"
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-editor-acceptance"
evidence.mkdir()
manifest = cast(dict[str, object], json.loads((proof / "testing-manifest.json").read_text()))
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", GOMAXPROCS="8", GOFLAGS="-p=8", PYTHONDONTWRITEBYTECODE="1")
environment["GOCACHE"] = str(evidence / "go-build")
assert not Path(environment["GOCACHE"]).exists()
environment["FLATS_PLAYWRIGHT_MODULE"] = str(Path(os.environ["RUNNER_TEMP"]) / "flats-browser-tools/node_modules/playwright/index.mjs")
environment["FLATS_DOCS_BROWSER_EVIDENCE"] = str(evidence / "screenshots")
environment["FLATS_DOCS_BROWSER_TEST"] = "1"
environment["TMPDIR"] = "/tmp/fea"
Path(environment["TMPDIR"]).mkdir(exist_ok=True)
Path(environment["FLATS_DOCS_BROWSER_EVIDENCE"]).mkdir()
results: list[dict[str, object]] = []

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, env=environment, text=True).strip()

def identity() -> None:
    assert git("rev-parse", "HEAD") == manifest["head"]
    assert git("write-tree") == manifest["tree"]
    assert git("rev-parse", "HEAD^") == manifest["base"]
    assert git("status", "--porcelain", "--untracked-files=all") == ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=root, env=environment, text=True).strip() == ""
    for item in cast(list[dict[str, str]], manifest["files"]):
        assert hashlib.sha256((root / item["path"]).read_bytes()).hexdigest() == item["sha256"]

def run(name: str, args: list[str], cwd: Path = root) -> str:
    start = time.monotonic()
    result = subprocess.run(args, cwd=cwd, env=environment, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=900, check=False)
    (evidence / (name + ".log")).write_text(result.stdout)
    results.append({"name": name, "args": args, "exit": result.returncode, "seconds": time.monotonic() - start,
                    "log_sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")
    print(f"FINAL_EDITOR_EXIT {name} {result.returncode}\n{result.stdout}", flush=True)
    assert result.returncode == 0
    return result.stdout

assert hashlib.sha256(Path(__file__).read_bytes()).hexdigest() == manifest["helper_sha256"]
identity()
try:
    web = root / "internal/apps/docs/_web"
    run("locked-install", ["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], web)
    run("locked-rebuild", ["npm", "run", "build"], web)
    identity()
    run("build-inputs", ["go", "test", "-count=1", "-run", "^TestBuildInputs$", "./internal/apps/docs"])
    browser = run("complete-browser", ["go", "test", "-count=1", "-timeout=12m", "-run", "^TestBrowserCollaboration$", "-v", "./internal/apps/docs"])
    assert browser.count("PASS: backward deletion") == 18
    assert "PASS: independent contexts" in browser and "--- PASS: TestBrowserCollaboration" in browser
    run("whitespace", ["git", "diff", "--check", str(manifest["base"])])
finally:
    identity()
print("EDITOR_FINAL_TESTING_PASS " + json.dumps({"head": manifest["head"], "tree": manifest["tree"]}), flush=True)
