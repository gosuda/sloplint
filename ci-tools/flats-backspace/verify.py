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
evidence = Path(os.environ["RUNNER_TEMP"]) / "flats-backspace-evidence"
evidence.mkdir(exist_ok=True)
manifest = cast(dict[str, object], json.loads((proof / "manifest.json").read_text()))
sources = cast(list[Source], manifest["sources"])
driver = (proof / "browser_test.mjs").read_bytes()
assert hashlib.sha256(driver).hexdigest() == manifest["driver_sha256"]
environment = dict(os.environ)
environment.update(GOENV="off", GOCACHEPROG="", GOMAXPROCS="8", GOFLAGS="-p=8", FLATS_DOCS_BROWSER_TEST="1")
environment["FLATS_PLAYWRIGHT_MODULE"] = str(Path(os.environ["RUNNER_TEMP"]) / "flats-browser-tools/node_modules/playwright/index.mjs")
environment["TMPDIR"] = str(Path(os.environ["RUNNER_TEMP"]) / "fbs")
Path(environment["TMPDIR"]).mkdir(exist_ok=True)
results: list[dict[str, object]] = []

def output(args: list[str], cwd: Path) -> str:
    return subprocess.check_output(args, cwd=cwd, env=environment, text=True).strip()

def identity(root: Path, source: Source) -> None:
    assert output(["git", "rev-parse", "HEAD"], root) == source["head"]
    assert output(["git", "write-tree"], root) == source["tree"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=root, check=True)
    assert output(["git", "status", "--porcelain", "--untracked-files=all"], root) == ""

for source in sources:
    root = workspace / source["name"]
    identity(root, source)
    assert output(["go", "env", "GOCACHEPROG"], root) == ""
    environment["GOCACHE"] = str(evidence / (source["name"] + "-go-cache"))
    assert not Path(environment["GOCACHE"]).exists()
    phase = evidence / source["name"]
    phase.mkdir()
    environment["FLATS_DOCS_BROWSER_EVIDENCE"] = str(phase)
    test_path = root / "internal/apps/docs/testdata/browser_test.mjs"
    original = test_path.read_bytes()
    record: dict[str, object] = {
        "source": source,
        "go": output(["go", "version"], root),
        "node": output(["node", "--version"], root),
        "cache": "empty phase-specific GOCACHE; Depot cache program explicitly disabled",
        "test_source_sha256": hashlib.sha256(original).hexdigest(),
        "driver_sha256": manifest["driver_sha256"],
    }
    command = ["go", "test", "-count=1", "-timeout=12m", "-run", "^TestBrowserCollaboration$", "-v", "./internal/apps/docs"]
    record["command"] = command
    saved: dict[Path, bytes] = {}
    try:
        if source["name"] == "candidate":
            web = root / "internal/apps/docs/_web"
            for relative in output(["git", "ls-files", "internal/apps/docs", "internal/design/web/wheel.js"], root).splitlines():
                file = root / relative
                saved[file] = file.read_bytes()
            build = proof / "candidate-build.mjs"
            assert hashlib.sha256(build.read_bytes()).hexdigest() == manifest["build_sha256"]
            (web / "build.mjs").write_bytes(build.read_bytes())
            for command_args in [
                ["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"],
                ["npm", "run", "build"],
            ]:
                result = subprocess.run(command_args, cwd=web, env=environment, text=True,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600, check=False)
                print(f"BUILD_EXIT {command_args} {result.returncode}\n{result.stdout[-4000:]}", flush=True)
                assert result.returncode == 0
            package_source = (web / "node_modules/@codemirror/view/dist/index.js").read_bytes()
            assert hashlib.sha256(package_source).hexdigest() == "350b1e5a052e2f016efa17791f3a56dfb1c849ae43df14b7d2f91fda100e3bc0"
            changes = output(["git", "diff", "--name-only"], root).splitlines()
            record["candidate_changes"] = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in changes}
            for name in changes:
                target = phase / "candidate-source" / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((root / name).read_bytes())
            print("CANDIDATE_SOURCE " + json.dumps(record["candidate_changes"], sort_keys=True), flush=True)
        test_path.write_bytes(driver)
        result = subprocess.run(command, cwd=root, env=environment, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                timeout=1000, check=False)
        (phase / "browser.log").write_text(result.stdout)
        record["exit"] = result.returncode
        print(f"BROWSER_EXIT {source['name']} {result.returncode}\n{result.stdout[-30000:]}", flush=True)
        observations: list[dict[str, object]] = []
        for line in result.stdout.splitlines():
            marker = "BACKSPACE_OBSERVATION "
            if marker in line:
                observations.append(cast(dict[str, object], json.loads(line.split(marker, 1)[1])))
        record["observations"] = observations
        record["complete"] = "BACKSPACE_PROBE_COMPLETE " in result.stdout and len(observations) == 18
        assert result.returncode in (0, 1), result.returncode
    finally:
        test_path.write_bytes(original)
        for file, original_bytes in saved.items():
            file.write_bytes(original_bytes)
        identity(root, source)
    results.append(record)
    (evidence / "results.json").write_text(json.dumps({"manifest": manifest, "results": results}, indent=2) + "\n")

assert len(results) == 3
assert all(record["complete"] for record in results), "native input diagnostic was incomplete"
print("BACKSPACE_DIAGNOSTIC_COMPLETE " + json.dumps(results, sort_keys=True), flush=True)
assert results[0]["exit"] == 0, "old package control failed"
assert results[1]["exit"] == 1, "current main did not reproduce"
assert results[2]["exit"] == 0, "candidate did not repair all native controls"
print("BACKSPACE_CANDIDATE_DIAGNOSTIC_PASS", flush=True)
