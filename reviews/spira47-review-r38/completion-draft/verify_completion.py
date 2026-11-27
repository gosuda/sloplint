"""Finish the unchanged candidate's full gates after authenticating the focused proof."""
from __future__ import annotations

import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import baseline_support as base
import candidate_support as tools
import completion_support as focused
import verify_baseline as observation
import verify_candidate as full


def main() -> None:
    completion = observation.read_object("completion-manifest.json")
    for name, wanted in base.object_value(completion["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != wanted:
            raise RuntimeError("completion proof digest differs: " + name)
    proof_root = base.PROOF.parents[1]
    workflow = proof_root / base.text(completion["workflow"])
    if base.digest(workflow.read_bytes()) != completion["workflow_sha256"]:
        raise RuntimeError("completion workflow digest differs")
    if base.run("proof-head", ["git", "-C", str(proof_root), "rev-parse", "HEAD"]).strip() != os.environ["GITHUB_SHA"]:
        raise RuntimeError("completion producer identity differs")
    original = observation.read_object("candidate-manifest.json")
    source = base.object_value(original["source"])
    if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
        raise RuntimeError("wrong maintained source")
    report: dict[str, object] = {
        "schema": 1, "completion_manifest": completion, "source_manifest": original,
        "source_gates_passed": False, "source_restored": False, "publication_eligible": False,
    }
    created: set[Path] = set()
    try:
        report["before"] = observation.source_snapshot(original, "source")
        token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
        try:
            report["reused_focused"] = focused.authenticate(completion, original, token)
            report["linter"] = tools.install_linter(original, token)
        finally:
            token = ""
        os.environ["GOWORK"] = "off"
        for name, command, pattern in (
            ("logos-version", ["logos", "version"], r"^logos version go1\.27\.4 linux/amd64$"),
            ("node-version", ["node", "--version"], r"^v26\.13\.1$"),
            ("pnpm-version", ["pnpm", "--version"], r"^12\.21\.0$"),
            ("just-version", ["just", "--version"], r"^just 1\.59\.0$"),
        ):
            if not re.fullmatch(pattern, base.run(name, command).strip()):
                raise RuntimeError("wrong " + name)
        if base.run("ripgrep-version", ["rg", "--version"]).splitlines()[0] != "ripgrep 15.1.0":
            raise RuntimeError("wrong pinned ripgrep")
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sloplint-version", ["sloplint", "--version"])
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run("sqlc-module", [
            "go", "-C", str(base.SOURCE / "tools"), "list", "-m", "-json", "github.com/sqlc-dev/sqlc",
        ])))
        if module["Version"] != original["sqlc_version"]:
            raise RuntimeError("source-pinned sqlc differs")
        rows = full.authored_files()
        if [row["path"] for row in rows] != base.object_value(original["candidate"])["changed_paths"]:
            raise RuntimeError("reviewed source path set differs")
        full.apply_files(rows, created)
        report["candidate_before"] = observation.source_snapshot(original, "candidate")
        if report["candidate_before"] != base.object_value(report["reused_focused"])["candidate"]:
            raise RuntimeError("fresh candidate differs from the focused proof")
        base.run("just-gate", ["just", "gate"], timeout=3600)
        base.run("just-slop", ["just", "slop"], timeout=600)
        base.run("just-eject", ["just", "eject"], timeout=1800)
        base.run("whitespace", ["git", "diff", "--cached", "--check", "HEAD"])
        base.run("tracked-source-unchanged", ["git", "diff", "--exit-code"])
        if base.run("untracked-source", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
            raise RuntimeError("unexpected untracked source")
        report["candidate_after"] = observation.source_snapshot(original, "candidate")
        if report["candidate_after"] != report["candidate_before"]:
            raise RuntimeError("candidate changed during source gates")
        report["source_gates_passed"] = True
    finally:
        try:
            base.run("restore-original-source", ["git", "restore", "--source", base.text(source["head"]),
                                                "--staged", "--worktree", "--", "."])
            for path in created:
                path.unlink(missing_ok=True)
            report["restored"] = observation.source_snapshot(original, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            if base.run("restored-untracked-source", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
                raise RuntimeError("temporary source remains after restoration")
            report["source_restored"] = True
        finally:
            report["publication_eligible"] = bool(report["source_gates_passed"] and report["source_restored"])
            report["commands"] = [asdict(row) for row in base.RECEIPTS]
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R38_COMPLETION_REPORT " + json.dumps(report), flush=True)


if __name__ == "__main__":
    main()
