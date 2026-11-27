"""Authenticate frozen native evidence and install its verified CLI."""
from __future__ import annotations

import io
import json
import os
import zipfile
from pathlib import Path

import artifact_support as artifacts
import baseline_support as baseline

PROOF = baseline.PROOF
OUTPUT = baseline.OUTPUT
BUILD = baseline.BUILD


def authenticate_baseline(manifest: dict[str, object], token: str) -> dict[str, object]:
    specification = baseline.object_value(manifest["baseline_artifact"])
    archive = artifacts.download(specification, token)
    frozen = baseline.object_value(json.loads((PROOF / "baseline-report.json").read_bytes()))
    with zipfile.ZipFile(io.BytesIO(archive.data)) as bundle:
        report = baseline.object_value(json.loads(bundle.read("report.json")))
        if report != frozen:
            raise RuntimeError("The frozen complete baseline report differs")
        if (report["baseline_verified"] is not True or report["source_restored"] is not True
                or report["publication_eligible"] is not False):
            raise RuntimeError("Baseline gate state differs")
        commands = report["commands"]
        if not isinstance(commands, list) or len(commands) != 100:
            raise RuntimeError("Baseline receipt count differs")
        names: set[str] = set()
        for value in commands:
            command = baseline.object_value(value)
            name = baseline.text(command["name"])
            if name in names or command["raw_exit"] != command["expected"] or command["timed_out"]:
                raise RuntimeError("Baseline command identity or outcome differs")
            names.add(name)
            if baseline.digest(bundle.read(name + ".log")) != command["log_sha256"]:
                raise RuntimeError("Baseline command bytes differ: " + name)
        original = baseline.object_value(report["before"])
        restored = baseline.object_value(report["restored"])
        if original != restored or original["tree"] != baseline.object_value(manifest["source"])["tree"]:
            raise RuntimeError("Baseline source or restoration differs")
    receipt = {**archive.receipt, "command_logs_verified": len(names),
               "baseline_verified": True, "source_restored": True,
               "scope": "Native values, compiled-query mismatches and the exact unchanged-source failure partition."}
    (OUTPUT / "authenticated-baseline.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                       encoding="utf-8")
    print("AUTHENTICATED_SPIRA_R38_BASELINE " + json.dumps(receipt), flush=True)
    return receipt


def install_linter(manifest: dict[str, object], token: str) -> dict[str, object]:
    specification = baseline.object_value(manifest["linter_artifact"])
    archive = artifacts.download(specification, token)
    with zipfile.ZipFile(io.BytesIO(archive.data)) as bundle:
        info = bundle.getinfo("sloplint")
        if info.file_size <= 0 or info.file_size > 256 * 1024 * 1024:
            raise RuntimeError("native linter exceeds the fixed extraction bound")
        binary = bundle.read(info)
        expected = baseline.text(specification["binary_sha256"])
        if baseline.digest(binary) != expected:
            raise RuntimeError("native linter binary digest differs")
        recorded_hash = bundle.read("logs/CLI_SHA256").decode().split()[0]
        recorded_source = bundle.read("logs/SOURCE_COMMIT").decode().strip()
        version = bundle.read("logs/CLI_VERSION").decode().strip()
        if recorded_hash != expected or recorded_source != specification["producer"]:
            raise RuntimeError("native linter producer records differ")
    tool_dir = BUILD / "verified-tools"
    tool_dir.mkdir(parents=True, exist_ok=True)
    tool = tool_dir / "sloplint"
    tool.write_bytes(binary)
    tool.chmod(0o755)
    os.environ["PATH"] = str(tool_dir) + os.pathsep + os.environ["PATH"]
    receipt = {**archive.receipt, "binary_sha256": expected,
               "binary_bytes": len(binary), "version": version, "source": recorded_source}
    (OUTPUT / "authenticated-linter.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                     encoding="utf-8")
    print("AUTHENTICATED_SLOPLINT " + json.dumps(receipt), flush=True)
    return receipt


