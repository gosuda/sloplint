from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

root = Path.cwd()
records = json.loads((root / "probe/ci-tools/bibeam-plans/heads.json").read_text())
source = root / "source"
evidence = root / "evidence"
evidence.mkdir(exist_ok=True)
expected = {
    "x86_64-unknown-linux-gnu": ("depot-ubuntu-22.04-4", "x86_64-unknown-linux-gnu"),
    "aarch64-unknown-linux-gnu": ("depot-ubuntu-22.04-arm-4", "aarch64-unknown-linux-gnu"),
    "x86_64-apple-darwin": ("depot-macos-latest", "aarch64-apple-darwin"),
    "aarch64-apple-darwin": ("depot-macos-latest", "aarch64-apple-darwin"),
    "x86_64-pc-windows-msvc": ("depot-windows-2022-4", "x86_64-pc-windows-msvc"),
}
results: list[dict[str, object]] = []

def run(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=source, text=True).strip()

for record in records:
    sha = record["head"]
    subprocess.run(["git", "fetch", "--depth=1", "origin", sha], cwd=source, check=True)
    subprocess.run(["git", "checkout", "--detach", sha], cwd=source, check=True)
    assert run(["git", "rev-parse", "HEAD"]) == sha
    config_bytes = (source / "dist-workspace.toml").read_bytes()
    assert hashlib.sha256(config_bytes).hexdigest() == "771324f468689fa7b07c309d41cafd02012f0d190b214d5cd3c88083d4581d66"
    config = tomllib.loads(config_bytes.decode())["dist"]
    assert set(config["targets"]) == set(expected)
    custom = config["github-custom-runners"]
    assert custom["global"] == {"runner": "depot-ubuntu-22.04-4", "host": "x86_64-unknown-linux-gnu"}
    for target, (runner, host) in expected.items():
        assert custom[target] == {"runner": runner, "host": host}
    destination = evidence / f"pr-{record['number']}-plan.json"
    with destination.open("w") as handle:
        process = subprocess.run(["dist", "plan", "--output-format=json"], cwd=source, stdout=handle, text=True, check=False)
    if process.returncode != 0:
        drift = run(["git", "diff"])
        (evidence / f"pr-{record['number']}-planning-drift.patch").write_text(drift + "\n")
        changed = run(["git", "diff", "--name-only"]).splitlines()
        assert set(changed) <= {"Cargo.lock"}, changed
        subprocess.run(["git", "restore", "--", "Cargo.lock"], cwd=source, check=True)
        result = {**record, "tree": run(["git", "rev-parse", "HEAD^{tree}"]), "exit": process.returncode, "runner_config_verified": True, "plan_completed": False}
        results.append(result)
        (evidence / "results.json").write_text(json.dumps(results, sort_keys=True, indent=2) + "\n")
        print("BIBEAM_PLAN_FAILURE " + json.dumps(result, sort_keys=True), flush=True)
        continue
    plan = json.loads(destination.read_text())
    jobs = plan["ci"]["github"]["artifacts_matrix"]["include"]
    seen: set[str] = set()
    for job in jobs:
        runner = job["runner"]
        assert runner.startswith("depot-"), job
        assert "ubuntu-26.04" not in runner, job
        if "macos" in runner:
            assert runner == "depot-macos-latest", job
        for target in job["targets"]:
            assert runner == expected[target][0], job
            seen.add(target)
    assert seen == set(expected), seen
    changed = run(["git", "diff", "--name-only"]).splitlines()
    assert set(changed) <= {"Cargo.lock"}, changed
    lock_drift = run(["git", "diff", "--", "Cargo.lock"])
    if lock_drift:
        (evidence / f"pr-{record['number']}-lock-drift.patch").write_text(lock_drift + "\n")
    lock_sha256 = hashlib.sha256((source / "Cargo.lock").read_bytes()).hexdigest()
    subprocess.run(["git", "restore", "--", "Cargo.lock"], cwd=source, check=True)
    subprocess.run(["git", "diff", "--exit-code"], cwd=source, check=True)
    result = {**record, "tree": run(["git", "rev-parse", "HEAD^{tree}"]), "plan_sha256": hashlib.sha256(destination.read_bytes()).hexdigest(), "jobs": jobs, "exit": process.returncode, "planning_changed_lockfile": bool(lock_drift), "resolved_lock_sha256": lock_sha256}
    results.append(result)
    print("BIBEAM_PLAN " + json.dumps(result, sort_keys=True), flush=True)
(evidence / "results.json").write_text(json.dumps(results, sort_keys=True, indent=2) + "\n")
failed = [record["number"] for record in results if record["exit"] != 0]
print(f"BIBEAM_PLANS_COMPLETE heads={len(results)} targets_per_head={len(expected)} failed={failed}", flush=True)
raise SystemExit(bool(failed))
