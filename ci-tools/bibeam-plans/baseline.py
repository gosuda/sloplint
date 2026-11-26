from __future__ import annotations

from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

from actions import EVIDENCE, PROOF, SOURCE, Record, check_lock, git, run

ARCHIVE_SHA256 = "73db86876d062b6f356031e6d472acc98422be1760b4d42ba9ae3751260b2aa3"


def diagnostics(label: str, text: str) -> list[str]:
    clean = re.sub(r"\x1b\[[0-9;]*m", "", text)
    if label == "deny":
        return sorted(set(
            re.findall(r"RUSTSEC-\d{4}-\d{4}", clean)
            + re.findall(r"error\[yanked\]: ([^\n]+)", clean)
        ))
    if label == "clippy":
        return sorted({
            line for line in clean.splitlines()
            if line.startswith("error:") and not line.startswith("error: could not compile")
        })
    return sorted({
        line for line in clean.splitlines()
        if re.match(r"\.github/workflows/[^:]+:\d+:\d+:", line)
    })


def verify(record: Record, archive: zipfile.ZipFile) -> dict[str, object]:
    destination = EVIDENCE / f"baseline-{record.number}"
    destination.mkdir()
    git("fetch", "--depth=1", "origin", record.head)
    git("checkout", "--detach", record.head)
    assert git("rev-parse", "HEAD") == record.head
    git("diff", "--exit-code")
    git("diff", "--cached", "--exit-code")
    source_tree = git("rev-parse", "HEAD^{tree}")
    before_lock = (SOURCE / "Cargo.lock").read_text()
    prior = json.loads(archive.read(f"pr-{record.number}/results.json"))
    assert prior["identity"]["head"] == record.head
    candidate_tree = prior["identity"]["final_tree"]
    outcomes: dict[str, object] = {}
    commands = [
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]),
        ("deny", ["cargo", "deny", "check"]),
        ("actionlint", ["actionlint"]),
    ]
    for label, command in commands:
        status = run(destination, f"baseline-{label}", command)
        candidate_status = prior["gates"][label]
        baseline_text = (destination / f"baseline-{label}.log").read_text(errors="replace")
        candidate_text = archive.read(f"pr-{record.number}/{label}.log").decode(errors="replace")
        before = diagnostics(label, baseline_text)
        after = diagnostics(label, candidate_text)
        assert status != 0 and candidate_status != 0, (label, status, candidate_status)
        assert before and after, (label, before, after)
        outcomes[label] = {
            "baseline_exit": status, "candidate_exit": candidate_status,
            "baseline_diagnostics": before, "candidate_diagnostics": after,
            "candidate_subset_of_baseline": set(after) <= set(before),
        }
    check_lock(before_lock, (SOURCE / "Cargo.lock").read_text())
    changed = git("diff", "HEAD", "--name-only").splitlines()
    assert set(changed) <= {"Cargo.lock"}, changed
    if changed:
        git("restore", "--source=HEAD", "--worktree", "--", *changed)
    files = prior["identity"]["files"]
    for path, digest in files.items():
        content = archive.read(f"pr-{record.number}/source/{path}")
        assert hashlib.sha256(content).hexdigest() == digest
        (SOURCE / path).write_bytes(content)
    git("add", "--", *files)
    assert git("write-tree") == candidate_tree
    author = git("show", "-s", "--format=%an")
    email = git("show", "-s", "--format=%ae")
    assert author and email
    git("config", "--global", "user.name", author)
    git("config", "--global", "user.email", email)
    message = destination / "commit-message.txt"
    message.write_text(record.commit_message)
    fixed_cog = run(destination, "candidate-cog-with-identity", ["cog", "verify", "--file", str(message)])
    syntax = run(destination, "candidate-actionlint-syntax", ["actionlint", "-shellcheck="])
    assert fixed_cog == 0 and syntax == 0
    git("diff", "--exit-code")
    assert git("write-tree") == candidate_tree
    result = {
        "record": asdict(record), "original_tree": source_tree, "candidate_tree": candidate_tree,
        "source_unchanged_during_checks": True, "comparison": outcomes,
        "candidate_cog_exit": fixed_cog, "candidate_actionlint_syntax_exit": syntax,
        "mandatory_application_gates_pass": False,
    }
    (destination / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    print("BIBEAM_BASELINE_RESULT " + json.dumps(result, sort_keys=True), flush=True)
    git("restore", "--source=HEAD", "--staged", "--worktree", "--", *files)
    git("diff", "--exit-code")
    git("diff", "--cached", "--exit-code")
    return result


def main() -> int:
    archive_path = EVIDENCE / "previous-gates.zip"
    assert hashlib.sha256(archive_path.read_bytes()).hexdigest() == ARCHIVE_SHA256
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    assert rustc == "rustc 1.100.0 (0ef1bc386 2026-11-09)", rustc
    records = [
        Record(**row)
        for row in json.loads((PROOF / "ci-tools/bibeam-plans/actions.json").read_text())
    ]
    results: list[dict[str, object]] = []
    with zipfile.ZipFile(archive_path) as archive:
        for record in records:
            results.append(verify(record, archive))
            (EVIDENCE / "baseline-results.json").write_text(json.dumps(results, indent=2) + "\n")
    print("BIBEAM_BASELINE_COMPLETE: comparison only; mandatory application gates remain failing", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
