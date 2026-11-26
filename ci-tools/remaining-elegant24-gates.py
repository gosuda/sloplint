from __future__ import annotations

import hashlib
import json
import os
import signal
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROBE = WORKSPACE / "probe"
EVIDENCE = WORKSPACE / "evidence"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def run(command: list[str], name: str, timeout: int = 3600) -> int:
    path = EVIDENCE / f"{name}.log"
    if os.name != "nt" and name in ("slop", "verify"):
        cpus = sorted(os.sched_getaffinity(0))[:2]
        command = ["taskset", "--cpu-list", ",".join(str(cpu) for cpu in cpus), *command]
    cgroup = Path("/sys/fs/cgroup")
    before = {name: (cgroup / name).read_text() for name in ("memory.max", "memory.events") if (cgroup / name).is_file()}
    with path.open("w", encoding="utf-8") as output:
        process = subprocess.Popen(command, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, start_new_session=os.name != "nt")
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], check=False, timeout=15)
            else:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=15)
            code = 124
    after = {name: (cgroup / name).read_text() for name in ("memory.max", "memory.events") if (cgroup / name).is_file()}
    resources = {"name": name, "command": command, "before": before, "after": after}
    with (EVIDENCE / "executions.jsonl").open("a", encoding="utf-8") as log:
        log.write(json.dumps(resources) + "\n")
    print("RESOURCE " + json.dumps(resources), flush=True)
    print(f"RESULT {name}: {code}", flush=True)
    if code:
        print(path.read_text(encoding="utf-8", errors="replace")[-12000:], flush=True)
    return code


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    mode = sys.argv[1]
    candidate = json.loads((PROBE / "ci-tools/remaining-elegant24-candidate.json").read_text())
    assert git("rev-parse", "HEAD") == os.environ.get("BASELINE_HEAD", candidate["head"])
    if mode in ("prepare", "baseline-prepare"):
        if mode == "prepare":
            assert git("status", "--porcelain") == ""
            assert git("rev-parse", "HEAD^{tree}") == candidate["source_tree"]
            patch = PROBE / "ci-tools" / candidate["patch"]
            assert hashlib.sha256(patch.read_bytes()).hexdigest() == candidate["patch_sha256"]
            git("apply", "--index", "--whitespace=error", str(patch))
            assert git("write-tree") == candidate["tree"]
            git("diff", "--cached", "--check")
            for entry in candidate["files"]:
                assert hashlib.sha256((SOURCE / entry["path"]).read_bytes()).hexdigest() == entry["sha256"], entry["path"]
            assert git("rev-parse", ":playground/src/routes/+page.svelte") == candidate["page_blob"]
        files = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in git("diff", "--cached", "--name-only").splitlines()}
        identity = {"candidate": candidate, "mode": mode, "tree": git("write-tree"), "source_head": git("rev-parse", "HEAD"), "files": files}
        (EVIDENCE / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        print("IDENTITY " + json.dumps(identity), flush=True)
        return 0
    identity = json.loads((EVIDENCE / "identity.json").read_text())
    assert git("write-tree") == identity["tree"]
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    os.environ["BASE_PATH"] = "/elegant-mermaid"
    if mode not in ("windows", "baseline", "markdown", "house"):
        assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    config = EVIDENCE / "actionlint.yaml"
    config.write_text("self-hosted-runner:\n  labels: [depot-ubuntu-24.04, depot-ubuntu-24.04-8, depot-windows-2025]\n")
    if mode == "full":
        commands = (
            ("workflow-lint", ["go", "run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.7", "-config-file", str(config), "-shellcheck=", "-pyflakes=", ".github/workflows/ci.yml", ".github/workflows/verify.yml", ".github/workflows/pages.yml", ".depot/workflows/ci.yml"]),
            ("gen-check", ["just", "gen-check"]),
            ("go-check", ["just", "go-check"]),
            ("rust-check", ["just", "rust-check"]),
            ("markdown-paths", ["just", "markdown-paths"]),
            ("build", ["just", "build"]),
            ("oracle", ["just", "oracle", "JOBS=4"]),
            ("parity", ["just", "parity", "JOBS=4"]),
            ("markdown-parity", ["just", "--no-deps", "markdown-parity"]),
            ("layered-random", ["just", "--no-deps", "layered-random-check", "JOBS=4"]),
            ("verify", ["just", "verify"]),
        )
    elif mode in ("quality", "baseline-quality"):
        commands = (
            ("build", ["just", "build"]),
            ("oracle-install", ["pnpm", "--dir", "tools/upstream-oracle", "install", "--frozen-lockfile"]),
            ("quality", ["just", "quality", "JOBS=4"]),
            ("theme-check", ["just", "--no-deps", "theme-check"]),
        )
    elif mode == "playground":
        commands = (
            ("playground-install", ["pnpm", "--dir", "playground", "install", "--frozen-lockfile"]),
            ("playground-types", ["pnpm", "--dir", "playground", "check"]),
            ("playground-lint", ["pnpm", "--dir", "playground", "exec", "biome", "check", "."]),
            ("playground-unit", ["pnpm", "--dir", "playground", "exec", "vitest", "run"]),
            ("build", ["just", "build"]),
            ("wasm-build", ["just", "playground-wasm"]),
            ("playground-build", ["just", "--no-deps", "playground-build"]),
            ("wasm-parity", ["just", "--no-deps", "wasm-parity"]),
            ("chromium", ["pnpm", "--dir", "playground", "exec", "playwright", "install", "--with-deps", "chromium"]),
            ("playground-browser", ["pnpm", "--dir", "playground", "exec", "playwright", "test"]),
            ("playground-worker", ["env", "PLAYGROUND_SERVER_MODE=dev", "pnpm", "--dir", "playground", "exec", "playwright", "test", "--grep", "both engines render the default source"]),
        )
    elif mode == "house":
        commands = (("slop", ["just", "slop"]), ("model", ["just", "model"]))
    elif mode == "markdown":
        commands = (("rustfmt", ["rustfmt", "--check", "--edition", "2024", "crates/elegant-mermaid-cli/src/render/markdown/links.rs"]), ("markdown-paths", ["just", "markdown-paths"]))
    elif mode == "windows":
        commands = (("windows-paths", ["just", "markdown-paths"]),)
    elif mode == "baseline":
        commands = (("baseline-proofs", ["just", "proofs"]),)
    else:
        raise ValueError(f"unknown mode {mode}")
    results = {name: run(command, name) for name, command in commands}
    quality_log = EVIDENCE / "quality.log"
    if quality_log.exists():
        rows = quality_log.read_text().splitlines()
        metrics = sorted(row for row in rows if row.startswith("corpus/") and " ours=" in row)
        summary = {"head": git("rev-parse", "HEAD"), "tree": git("write-tree"), "fixtures": len(metrics), "exceptions": sum("exception=true" in row for row in metrics), "metrics_sha256": hashlib.sha256(("\n".join(metrics) + "\n").encode()).hexdigest(), "gates": [row for row in rows if row.startswith("gate ")], "diagnostics": [row for row in rows if "differs from computed" in row]}
        (EVIDENCE / "quality-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print("QUALITY_SUMMARY " + json.dumps(summary), flush=True)
    results["worktree-clean"] = run(["git", "diff", "--exit-code"], "worktree-clean", 30)
    results["candidate-tree"] = 0 if git("write-tree") == identity["tree"] else 1
    (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print("GATE_RESULTS " + json.dumps(results, sort_keys=True), flush=True)
    return 0 if all(code == 0 for code in results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
