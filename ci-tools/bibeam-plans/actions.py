from __future__ import annotations

import base64
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

ROOT = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = ROOT / "source"
PROOF = ROOT / "probe"
EVIDENCE = ROOT / "evidence"
EVIDENCE.mkdir(exist_ok=True)
ALLOWED = {"dist-workspace.toml", ".github/workflows/release.yml", "Cargo.lock"}
EXPECTED = {
    "x86_64-unknown-linux-gnu": ("depot-ubuntu-22.04-4", "x86_64-unknown-linux-gnu"),
    "aarch64-unknown-linux-gnu": ("depot-ubuntu-22.04-arm-4", "aarch64-unknown-linux-gnu"),
    "x86_64-apple-darwin": ("depot-macos-latest", "aarch64-apple-darwin"),
    "aarch64-apple-darwin": ("depot-macos-latest", "aarch64-apple-darwin"),
    "x86_64-pc-windows-msvc": ("depot-windows-2022-4", "x86_64-pc-windows-msvc"),
}


@dataclass(frozen=True)
class Record:
    number: int
    head: str
    input_tree: str
    action: str
    version: str
    input_config_sha256: str
    commit_message: str


def git(*arguments: str) -> str:
    return subprocess.check_output(["git", *arguments], cwd=SOURCE, text=True).strip()


def run(destination: Path, label: str, command: list[str], timeout: int = 3600) -> int:
    print(f"RUN pr={destination.name} gate={label} command={json.dumps(command)}", flush=True)
    output = destination / f"{label}.log"
    with output.open("w") as stream:
        try:
            status = subprocess.run(
                command, cwd=SOURCE, stdout=stream, stderr=subprocess.STDOUT,
                check=False, timeout=timeout,
            ).returncode
        except subprocess.TimeoutExpired:
            stream.write(f"\nTimed out after {timeout} seconds.\n")
            status = 124
    print(f"EXIT pr={destination.name} gate={label} status={status}", flush=True)
    text = output.read_text(errors="replace")
    print(text[-20000:] if status else text[-3000:], flush=True)
    return status


def check_lock(before: str, after: str) -> None:
    old = tomllib.loads(before)
    new = tomllib.loads(after)
    assert old["version"] == new["version"]
    old_packages = {(p["name"], p["version"], p.get("source", "")): p for p in old["package"]}
    new_packages = {(p["name"], p["version"], p.get("source", "")): p for p in new["package"]}
    assert old_packages.keys() == new_packages.keys(), "lockfile package resolution changed"
    for key, package in old_packages.items():
        replacement = new_packages[key]
        if key[2]:
            assert package == replacement, f"third-party package changed: {key}"
        else:
            assert {k: v for k, v in package.items() if k != "dependencies"} == {
                k: v for k, v in replacement.items() if k != "dependencies"
            }, f"workspace identity changed: {key}"


def plan_check(destination: Path) -> tuple[int, list[dict[str, object]]]:
    output = destination / "plan.json"
    errors = destination / "plan-stderr.log"
    with output.open("w") as stdout, errors.open("w") as stderr:
        status = subprocess.run(
            ["dist", "plan", "--output-format=json"], cwd=SOURCE, stdout=stdout,
            stderr=stderr, check=False, timeout=600,
        ).returncode
    print(f"EXIT pr={destination.name} gate=plan status={status}", flush=True)
    print(errors.read_text(errors="replace")[-10000:], flush=True)
    if status:
        return status, []
    plan = json.loads(output.read_text())
    jobs = plan["ci"]["github"]["artifacts_matrix"]["include"]
    seen: set[str] = set()
    for job in jobs:
        runner = job["runner"]
        assert runner.startswith("depot-") and "ubuntu-26.04" not in runner, job
        if "macos" in runner:
            assert runner == "depot-macos-latest", job
        for target in job["targets"]:
            assert runner == EXPECTED[target][0], job
            seen.add(target)
    assert seen == set(EXPECTED), seen
    return status, jobs


def verify(record: Record) -> dict[str, object]:
    destination = EVIDENCE / f"pr-{record.number}"
    destination.mkdir()
    git("fetch", "--depth=1", "origin", record.head)
    git("checkout", "--detach", record.head)
    assert git("rev-parse", "HEAD") == record.head
    git("diff", "--exit-code")
    git("diff", "--cached", "--exit-code")
    config_path = SOURCE / "dist-workspace.toml"
    original_config = config_path.read_text()
    assert "[dist.github-action-commits]" not in original_config
    config = original_config.rstrip() + (
        f'\n\n[dist.github-action-commits]\n"{record.action}" = "{record.version}"\n'
    )
    assert hashlib.sha256(config.encode()).hexdigest() == record.input_config_sha256
    config_path.write_text(config)
    git("add", "--", "dist-workspace.toml")
    assert git("write-tree") == record.input_tree
    before_lock = (SOURCE / "Cargo.lock").read_text()
    assert run(destination, "generate", ["dist", "generate"]) == 0
    assert run(destination, "prepare-toml-format", ["taplo", "fmt", "dist-workspace.toml"]) == 0
    assert run(destination, "regenerate", ["dist", "generate"]) == 0
    check_lock(before_lock, (SOURCE / "Cargo.lock").read_text())
    config_data = tomllib.loads(config_path.read_text())["dist"]
    assert config_data["github-action-commits"] == {record.action: record.version}
    assert not config_data.get("allow-dirty")
    assert config_data["pr-run-mode"] == "skip"
    assert set(config_data["targets"]) == set(EXPECTED)
    custom = config_data["github-custom-runners"]
    assert custom["global"] == {"runner": "depot-ubuntu-22.04-4", "host": "x86_64-unknown-linux-gnu"}
    for target, (runner, host) in EXPECTED.items():
        assert custom[target] == {"runner": runner, "host": host}
    release = (SOURCE / ".github/workflows/release.yml").read_text()
    uses = re.findall(r"uses:\s*" + re.escape(record.action) + r"@(\S+)", release)
    assert uses and set(uses) == {record.version}, uses
    changed = git("diff", "HEAD", "--name-only").splitlines()
    assert set(changed) <= ALLOWED, changed
    git("add", "--", *changed)
    git("diff", "--cached", "--check")
    git("diff", "--exit-code")
    final_tree = git("write-tree")
    hashes = {name: hashlib.sha256((SOURCE / name).read_bytes()).hexdigest() for name in changed}
    identity = {
        "number": record.number, "head": record.head, "input_tree": record.input_tree,
        "final_tree": final_tree, "files": hashes,
        "rustc": subprocess.check_output(["rustc", "--version"], cwd=SOURCE, text=True).strip(),
        "action": record.action, "version": record.version,
    }
    (destination / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print("BIBEAM_ACTION_IDENTITY " + json.dumps(identity, sort_keys=True), flush=True)
    for name in changed:
        data = (SOURCE / name).read_bytes()
        output = destination / "source" / name
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SOURCE / name, output)
        encoded = base64.b64encode(data).decode("ascii")
        for offset in range(0, len(encoded), 4000):
            print(f"BIBEAM_SOURCE_B64 {record.number} {name} {offset // 4000} {encoded[offset:offset + 4000]}", flush=True)
    message_path = destination / "commit-message.txt"
    message_path.write_text(record.commit_message)
    gates: dict[str, int] = {}
    commands = [
        ("format", ["cargo", "fmt", "--all", "--", "--check"]),
        ("taplo", ["taplo", "fmt", "--check"]),
        ("typos", ["typos"]),
        ("readmes", ["cargo", "run", "-p", "xtask", "--release", "--", "gen-readmes", "--check"]),
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]),
        ("nextest", ["cargo", "nextest", "run", "--workspace", "--all-features", "--no-tests=warn"]),
        ("deny", ["cargo", "deny", "check"]),
        ("machete", ["cargo", "machete", "--skip-target-dir"]),
        ("doc", ["cargo", "doc", "--workspace", "--no-deps", "--all-features"]),
        ("commit-message", ["cog", "verify", "--file", str(message_path)]),
        ("pre-push", ["cargo", "check", "--workspace", "--all-targets", "--all-features"]),
        ("actionlint", ["actionlint"]),
        ("metadata-locked", ["cargo", "metadata", "--locked", "--format-version=1", "--no-deps"]),
    ]
    for label, command in commands:
        gates[label] = run(destination, label, command)
    plan_status, jobs = plan_check(destination)
    gates["plan"] = plan_status
    gates["generation-idempotent"] = run(destination, "generation-idempotent", ["dist", "generate"])
    gates["source-clean"] = run(destination, "source-clean", ["git", "diff", "--exit-code"])
    assert git("write-tree") == final_tree
    result = {"identity": identity, "gates": gates, "jobs": jobs, "success": all(code == 0 for code in gates.values())}
    (destination / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    print("BIBEAM_ACTION_RESULT " + json.dumps(result, sort_keys=True), flush=True)
    git("restore", "--source=HEAD", "--staged", "--worktree", "--", *changed)
    git("diff", "--exit-code")
    git("diff", "--cached", "--exit-code")
    return result


def main() -> int:
    records = [Record(**row) for row in json.loads((PROOF / "ci-tools/bibeam-plans/actions.json").read_text())]
    versions = [
        ["rustc", "--version"], ["cargo", "--version"], ["dist", "--version"],
        ["cargo", "nextest", "--version"], ["taplo", "--version"], ["typos", "--version"],
        ["cargo", "deny", "--version"], ["cargo", "machete", "--version"], ["cog", "--version"],
    ]
    for command in versions:
        subprocess.run(command, cwd=SOURCE, check=True)
    results: list[dict[str, object]] = []
    for record in records:
        results.append(verify(record))
        (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    return int(not all(result["success"] for result in results))


if __name__ == "__main__":
    raise SystemExit(main())
