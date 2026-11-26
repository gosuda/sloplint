from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import cast

proof = Path(__file__).resolve().parent
workspace = Path(os.environ["GITHUB_WORKSPACE"])
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-editor-final"
evidence.mkdir(exist_ok=True)
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", GOMAXPROCS="8", GOFLAGS="-p=8", PYTHONDONTWRITEBYTECODE="1")
environment["FLATS_PLAYWRIGHT_MODULE"] = str(Path(os.environ["RUNNER_TEMP"]) / "flats-browser-tools/node_modules/playwright/index.mjs")
environment["TMPDIR"] = "/tmp/fef"
Path(environment["TMPDIR"]).mkdir(exist_ok=True)
results: list[dict[str, object]] = []

def output(args: list[str], root: Path, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=root, env=environment if env is None else env, text=True).strip()

def identity(root: Path) -> None:
    assert output(["git", "rev-parse", "HEAD"], root) == manifest["base"]
    assert output(["git", "write-tree"], root) == manifest["base_tree"]
    assert output(["git", "status", "--porcelain", "--untracked-files=all"], root) == ""
    assert output(["go", "env", "GOCACHEPROG"], root) == ""

def run(args: list[str], root: Path, name: str, timeout: int = 900) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=root, env=environment, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout, check=False)
    (evidence / (name + ".log")).write_text(result.stdout)
    results.append({"name": name, "args": args, "exit": result.returncode,
                    "log_sha256": hashlib.sha256(result.stdout.encode()).hexdigest()})
    print(f"EDITOR_GATE_EXIT {name} {result.returncode}\n{result.stdout[-14000:]}", flush=True)
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")
    return result

for name, digest in cast(dict[str, str], manifest["proof_sha256"]).items():
    assert hashlib.sha256((proof / name).read_bytes()).hexdigest() == digest

browser_args = ["go", "test", "-count=1", "-timeout=12m", "-run", "^TestBrowserCollaboration$", "-v", "./internal/apps/docs"]
for phase in ("baseline", "candidate"):
    root = workspace / phase
    identity(root)
    saved = {root / name: (root / name).read_bytes() for name in output(["git", "ls-files", "internal/apps/docs"], root).splitlines()}
    environment["GOCACHE"] = str(evidence / (phase + "-go-cache"))
    assert not Path(environment["GOCACHE"]).exists()
    screenshots = evidence / (phase + "-screenshots")
    screenshots.mkdir()
    environment["FLATS_DOCS_BROWSER_EVIDENCE"] = str(screenshots)
    environment["FLATS_DOCS_BROWSER_TEST"] = "1"
    try:
        (root / "internal/apps/docs/testdata/browser_test.mjs").write_bytes((proof / "repository-browser.mjs").read_bytes())
        if phase == "candidate":
            web = root / "internal/apps/docs/_web"
            (web / "build.mjs").write_bytes((proof / "candidate-build.mjs").read_bytes())
            assert run(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], web, "npm-ci", 300).returncode == 0
            assert run(["npm", "run", "build"], web, "locked-build", 600).returncode == 0
            assert hashlib.sha256((web / "node_modules/@codemirror/view/dist/index.js").read_bytes()).hexdigest() == manifest["upstream_source_sha256"]
            changed = output(["git", "diff", "--name-only"], root).splitlines()
            assert sorted(changed) == sorted(cast(list[str], manifest["paths"]))
            index_environment = dict(environment)
            index_environment["GIT_INDEX_FILE"] = str(evidence / "candidate.index")
            output(["git", "read-tree", "HEAD"], root, index_environment)
            output(["git", "add", "--", *changed], root, index_environment)
            candidate_tree = output(["git", "write-tree"], root, index_environment)
            print("EDITOR_CANDIDATE_TREE " + candidate_tree, flush=True)
            (evidence / "candidate-tree.txt").write_text(candidate_tree + "\n")
            assert run(["git", "diff", "--check"], root, "whitespace").returncode == 0
            assert run(["node", "--check", "build.mjs"], web, "build-syntax").returncode == 0
            assert run(["node", "--check", "../testdata/browser_test.mjs"], web, "browser-syntax").returncode == 0
        result = run(browser_args, root, phase + "-browser")
        if phase == "baseline":
            assert result.returncode == 1
            assert "PASS: backward deletion last line keyboard" in result.stdout
            assert "last line: native" in result.stdout and "AssertionError" in result.stdout
            continue
        assert result.returncode == 0
        assert result.stdout.count("PASS: backward deletion") == 18
        assert "PASS: independent contexts" in result.stdout
        environment.pop("FLATS_DOCS_BROWSER_TEST")
        assert run(["npm", "test"], web, "docs-web", 600).returncode == 0
        assert run(["go", "vet", "./internal/apps/docs", "./internal/runtime"], root, "affected-vet").returncode == 0
        for lane in ("docs-budget", "docs-soak", "docs", "runtime-codec", "runtime-soak", "runtime"):
            assert run(["bash", "scripts/ci-test.sh", lane, "-count=1", "-timeout=15m"], root, lane, 1000).returncode == 0
        assert run(["make", "docs-audit"], root, "docs-audit", 600).returncode == 0
        assert run(["make", "docs-verify"], root, "docs-verify", 1200).returncode == 0
        assert run(["go", "test", "-count=1", "-run", "^TestBuildInputs$", "./internal/apps/docs"], root, "build-inputs").returncode == 0
        for name in changed:
            source = (root / name).read_text()
            print("GENERATED_SOURCE_JSON " + json.dumps({"path": name, "sha256": hashlib.sha256(source.encode()).hexdigest(),
                                                        "content": source}), flush=True)
        print("EDITOR_IMPLEMENTATION_DEVELOPMENT_GATES_PASS", flush=True)
    finally:
        for file, original in saved.items():
            file.write_bytes(original)
        identity(root)
