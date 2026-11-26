from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

proof = Path(__file__).resolve().parent
root = Path(os.environ["GITHUB_WORKSPACE"]) / "candidate"
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-widget-development"
evidence.mkdir()
manifest = json.loads((proof / "manifest.json").read_text())
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", GOMAXPROCS="8", GOFLAGS="-p=8", PYTHONDONTWRITEBYTECODE="1")
environment["GOCACHE"] = str(evidence / "go-build")
assert not Path(environment["GOCACHE"]).exists()
environment["FLATS_PLAYWRIGHT_MODULE"] = str(Path(os.environ["RUNNER_TEMP"]) / "flats-browser-tools/node_modules/playwright/index.mjs")
environment["FLATS_DOCS_BROWSER_EVIDENCE"] = str(evidence / "screenshots")
environment["FLATS_DOCS_BROWSER_TEST"] = "1"
environment["TMPDIR"] = "/tmp/fw273"
Path(environment["TMPDIR"]).mkdir(exist_ok=True)
Path(environment["FLATS_DOCS_BROWSER_EVIDENCE"]).mkdir()
results = []


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, env=environment, text=True).strip()


def identity(patched: bool) -> None:
    assert git("rev-parse", "HEAD") == manifest["base_head"]
    assert git("rev-parse", "HEAD^{tree}") == manifest["base_tree"]
    assert git("rev-parse", "HEAD^") == manifest["actual_main"]
    expected_tree = manifest["candidate_tree"] if patched else manifest["base_tree"]
    assert git("write-tree") == expected_tree
    expected_change = manifest["test_path"] if patched else ""
    assert git("diff", "--name-only", "HEAD") == expected_change
    assert git("diff", "--name-only") == ""
    assert git("ls-files", "--others", "--exclude-standard") == ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=root, env=environment, text=True).strip() == ""
    for item in manifest["unchanged_files"]:
        assert hashlib.sha256((root / item["path"]).read_bytes()).hexdigest() == item["sha256"]
    expected_hash = manifest["candidate_test_sha256"] if patched else manifest["base_test_sha256"]
    assert hashlib.sha256((root / manifest["test_path"]).read_bytes()).hexdigest() == expected_hash
    print("WIDGET_SOURCE_IDENTITY " + json.dumps({"base_head": manifest["base_head"], "tree": expected_tree, "patched": patched}), flush=True)


def run(name: str, args: list[str], cwd: Path = root) -> str:
    start = time.monotonic()
    result = subprocess.run(args, cwd=cwd, env=environment, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=900, check=False)
    (evidence / (name + ".log")).write_text(result.stdout)
    results.append({"name": name, "args": args, "exit": result.returncode, "seconds": time.monotonic() - start,
                    "log_sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")
    print(f"WIDGET_DEVELOPMENT_EXIT {name} {result.returncode}\n{result.stdout}", flush=True)
    assert result.returncode == 0
    return result.stdout


assert hashlib.sha256(Path(__file__).read_bytes()).hexdigest() == manifest["helper_sha256"]
patch = proof / "candidate.patch"
assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
identity(False)
try:
    run("apply-test-only-patch", ["git", "apply", "--index", str(patch)])
    identity(True)
    run("syntax", ["node", "--check", manifest["test_path"]])
    web = root / "internal/apps/docs/_web"
    run("locked-install", ["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], web)
    run("locked-rebuild", ["npm", "run", "build"], web)
    identity(True)
    run("docs-web-tests", ["npm", "test"], web)
    run("affected-vet", ["go", "vet", "./internal/apps/docs", "./internal/runtime"])
    run("build-inputs", ["go", "test", "-count=1", "-run", "^TestBuildInputs$", "./internal/apps/docs"])
    browser = run("complete-browser", ["go", "test", "-count=1", "-timeout=12m", "-run", "^TestBrowserCollaboration$", "-v", "./internal/apps/docs"])
    cases = [
        "last line", "middle line", "retained final newline", "empty first line", "empty final line",
        "empty middle line", "character at end", "character within line", "Unicode line", "trailing diagram widget",
    ]
    for name in cases:
        for operation in ("keyboard", "native"):
            assert browser.count("PASS: backward deletion " + name + " " + operation) == 1, (name, operation)
    assert browser.count("PASS: backward deletion") == 20
    for operation in ("keyboard", "native"):
        assert browser.count("PASS: trailing diagram widget " + operation) == 1
    assert "PASS: independent contexts" in browser and "--- PASS: TestBrowserCollaboration" in browser
    run("whitespace", ["git", "diff", "--check", manifest["base_head"]])
    identity(True)
finally:
    run("restore-test-source", ["git", "restore", "--source=HEAD", "--staged", "--worktree", "--", manifest["test_path"]])
    identity(False)
print("WIDGET_DEVELOPMENT_PASS " + json.dumps({"base_head": manifest["base_head"], "candidate_tree": manifest["candidate_tree"]}), flush=True)
