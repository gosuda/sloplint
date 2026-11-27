from __future__ import annotations

import hashlib
import json
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import cast

workspace = Path(os.environ["GITHUB_WORKSPACE"])
source = workspace / ".outline" / "worktree" / "dstack-skills"
proof = workspace / "proof" / "ci-tools" / "dstack-voice-runtime-r32"
output = Path(os.environ["RUNNER_TEMP"]) / "dstack-voice-runtime-r32"
output.mkdir()
environment: dict[str, str] = dict(os.environ)
environment["PYTHONDONTWRITEBYTECODE"] = "1"
environment["PREK_HOME"] = str(output / "prek")
tool_bin = subprocess.check_output(["uv", "tool", "dir", "--bin"], text=True).strip()
environment["PATH"] = tool_bin + os.pathsep + environment["PATH"]
results: list[dict[str, object]] = []
status = "incomplete"
candidate_tree = ""
source_restored = False


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mapping(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or not all(isinstance(k, str) for k in value):
        raise TypeError("expected string-key mapping")
    return cast(dict[str, object], value)


def string(value: object) -> str:
    if not isinstance(value, str):
        raise TypeError("expected string")
    return value


@dataclass(frozen=True)
class Change:
    path: str
    content: str
    sha256: str


manifest = mapping(json.loads((proof / "manifest.json").read_text(encoding="utf-8")))
proof_hashes = mapping(manifest["proof_sha256"])
for name, value in proof_hashes.items():
    if digest((proof / name).read_bytes()) != string(value):
        raise AssertionError(f"proof identity mismatch: {name}")
raw_changes: object = json.loads((proof / "authored.json").read_text(encoding="utf-8"))
if not isinstance(raw_changes, list):
    raise TypeError("expected change list")
changes: list[Change] = []
for raw in raw_changes:
    item = mapping(raw)
    change = Change(string(item["path"]), string(item["content"]), string(item["sha256"]))
    if digest(change.content.encode()) != change.sha256:
        raise AssertionError(f"authored identity mismatch: {change.path}")
    changes.append(change)
if {c.path for c in changes} != {".pre-commit-config.yaml", "scripts/check-voice.py"} or len(changes) != 2:
    raise AssertionError("unexpected source change set")
original_head = "3fd028e709641b72b75afe7bdb7785bedca2a959"
original_tree = "f465cbf542b6899d9aacbbab8d7f595ec582f074"
draw = source / "plugins/dstack-core/skills/let-fate-decide/scripts/draw.py"
original_draw = draw.read_bytes()
original_hashes = {
    c.path: digest((source / c.path).read_bytes()) for c in changes
}
original_hashes[str(draw.relative_to(source))] = digest(original_draw)
if original_hashes != mapping(manifest["source_guards"]):
    raise AssertionError("original source guard mismatch")
future = b"from __future__ import annotations\n"
if original_draw.count(future) != 1:
    raise AssertionError("ambiguous dependency mutation")


def run(name: str, command: list[str], expected: int = 0, required: tuple[str, ...] = (), timeout: int = 1800) -> str:
    started = time.monotonic()
    timed_out = False
    print("VOICE_COMMAND " + json.dumps({"name": name, "command": command}), flush=True)
    try:
        result = subprocess.run(command, cwd=source, env=environment, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                timeout=timeout, check=False)
        code, text = result.returncode, result.stdout
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        code = 124
        partial = exc.stdout
        text = partial.decode(errors="replace") if isinstance(partial, bytes) else partial or ""
    print(text, end="" if text.endswith("\n") else "\n", flush=True)
    (output / f"{name}.log").write_text(text, encoding="utf-8")
    accepted = not timed_out and code == expected and all(marker in text for marker in required)
    receipt: dict[str, object] = {
        "name": name, "command": command, "raw_exit": code, "expected_exit": expected,
        "accepted": accepted, "timed_out": timed_out, "seconds": time.monotonic() - started,
        "log_sha256": digest(text.encode()),
    }
    results.append(receipt)
    print("VOICE_RESULT " + json.dumps(receipt, sort_keys=True), flush=True)
    if not accepted:
        raise RuntimeError(f"unexpected command outcome: {name}")
    return text


def git(*args: str, index: Path | None = None) -> str:
    env = dict(environment)
    if index is not None:
        env["GIT_INDEX_FILE"] = str(index)
    return subprocess.check_output(["git", *args], cwd=source, env=env, text=True).strip()


def original_identity() -> None:
    if git("rev-parse", "HEAD") != original_head or git("rev-parse", "HEAD^{tree}") != original_tree:
        raise AssertionError("original commit identity changed")
    if git("status", "--porcelain"):
        raise AssertionError("source checkout is not clean")
    for path, value in original_hashes.items():
        if digest((source / path).read_bytes()) != value:
            raise AssertionError(f"original source changed: {path}")


def candidate_identity() -> None:
    if git("rev-parse", "HEAD") != original_head:
        raise AssertionError("checkout head changed")
    for c in changes:
        if digest((source / c.path).read_bytes()) != c.sha256:
            raise AssertionError(f"candidate changed: {c.path}")
    if draw.read_bytes() != original_draw:
        raise AssertionError("dependency mutation was not restored")
    if git("diff", "--name-only", candidate_tree):
        raise AssertionError("unreviewed source changes remain")


host310 = ["uv", "run", "--no-project", "--python", "3.10", "--"]
host315 = ["uv", "run", "--no-project", "--python", "3.15", "--"]
hook = ["prek", "run", "--all-files", "--verbose", "check-voice-self-test"]

try:
    original_identity()
    run("host-310-identity", host310 + ["python", "-c", "import sys; print(sys.version); assert sys.version_info[:2] == (3, 10)"])
    run("host-315-identity", host315 + ["python", "-c", "import sys; print(sys.version); assert sys.version_info[:2] == (3, 15)"])
    run("prek-identity", ["prek", "--version"], required=("0.5.12",))
    run("original-hook-control", host310 + hook, required=("check-voice self-test:",))
    try:
        draw.write_bytes(original_draw.replace(future, b"", 1))
        run("original-hook-misses-dependency-fault", host310 + hook, required=("check-voice self-test:",))
    finally:
        draw.write_bytes(original_draw)
    original_identity()

    for c in changes:
        (source / c.path).write_text(c.content, encoding="utf-8")
    candidate_index = output / "candidate.git-index"
    git("read-tree", original_head, index=candidate_index)
    git("add", "--", *[c.path for c in changes], index=candidate_index)
    candidate_tree = git("write-tree", index=candidate_index)
    (output / "authored.json").write_bytes((proof / "authored.json").read_bytes())
    print("VOICE_CANDIDATE_IDENTITY " + json.dumps({
        "tree": candidate_tree, "source_head": original_head,
        "files": [{"path": c.path, "sha256": c.sha256} for c in changes],
    }, sort_keys=True), flush=True)
    for c in changes:
        print("VOICE_CANDIDATE_FILE " + json.dumps({"path": c.path, "content": c.content, "sha256": c.sha256}, sort_keys=True), flush=True)
    run("candidate-hook-on-modern-host", host310 + hook, required=("check-voice self-test:",))
    try:
        draw.write_bytes(original_draw.replace(future, b"", 1))
        run("candidate-hook-catches-dependency-fault", host310 + hook, expected=1,
            required=("draw.py", "TypeError", "unsupported operand type(s) for |"))
    finally:
        draw.write_bytes(original_draw)
    candidate_identity()
    run("candidate-hook-restored", host310 + hook, required=("check-voice self-test:",))
    run("required-just-check", host315 + ["just", "check"])
    candidate_identity()
    run("candidate-whitespace", ["git", "diff", "--check"])
    status = "Exact candidate passed original blind-spot control, real Python3.9 dependency mutation failure, restored hook and required just check."
finally:
    draw.write_bytes(original_draw)
    subprocess.run(["git", "restore", "--worktree", "--source=HEAD", "--", "."],
                   cwd=source, env=environment, check=True)
    original_identity()
    source_restored = True
    report: dict[str, object] = {
        "schema": 1, "source_head": original_head, "source_tree": original_tree,
        "candidate_tree": candidate_tree, "status": status, "results": results,
        "source_restored": source_restored, "manifest": manifest,
    }
    (output / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("VOICE_FINAL_REPORT " + json.dumps(report, sort_keys=True), flush=True)
