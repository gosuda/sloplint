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


