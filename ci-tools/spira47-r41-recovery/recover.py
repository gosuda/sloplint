"""Recover bounded evidence from a frozen failed job without rerunning source."""
from __future__ import annotations

import base64
import gzip
import hashlib
import io
import json
import os
import re
import sys
import zipfile
from pathlib import Path, PurePosixPath

from artifact_support import download, object_value, text


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_member(archive: zipfile.ZipFile, name: str, limit: int) -> bytes:
    info = archive.getinfo(name)
    if info.file_size > limit:
        raise ValueError("selected archive member exceeds its bound: " + name)
    with archive.open(info) as stream:
        data = stream.read(limit + 1)
    if len(data) != info.file_size or len(data) > limit:
        raise RuntimeError("selected archive member size differs: " + name)
    return data


def main() -> None:
    token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
    root = Path(__file__).resolve().parent
    manifest = object_value(json.loads((root / "manifest.json").read_bytes()))
    for name, wanted in object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or digest((root / name).read_bytes()) != wanted:
            raise RuntimeError("recovery proof hash differs: " + name)
    workflow = root.parent.parent / ".github/workflows/spira47-review-r41.yml"
    if digest(workflow.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active recovery workflow differs")
    specification = object_value(manifest["artifact"])
    artifact = download(specification, token)
    del token
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        entries = archive.infolist()
        if len(entries) > 512:
            raise ValueError("recovery archive has too many members")
        names: set[str] = set()
        for entry in entries:
            path = PurePosixPath(entry.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or entry.filename in names):
                raise ValueError("invalid recovery archive member")
            names.add(entry.filename)
        member_rows = [{"name": entry.filename, "bytes": entry.file_size} for entry in entries]
        (output / "archive-members.json").write_text(json.dumps(member_rows, indent=2) + "\n",
                                                     encoding="utf-8")
        print("SPIRA_R41_RECOVERY_MEMBERS " + json.dumps(member_rows), flush=True)
        if archive.getinfo("report.json").file_size != 70_404_886:
            raise RuntimeError("frozen report size differs")
        report_bytes = read_member(archive, "report.json", 70_404_886)
        report = object_value(json.loads(report_bytes))
        if (report["producer"] != specification["producer"]
                or str(report["run"]) != str(specification["run"])
                or report["manifest_sha256"] != specification["manifest_sha256"]
                or report["baseline_verified"] is not False
                or report["source_gates_passed"] is not False
                or report["publication_eligible"] is not False):
            raise RuntimeError("frozen failed report identity/status differs")
        commands = report["commands"]
        if not isinstance(commands, list) or not commands or len(commands) > 256:
            raise ValueError("unexpected recovery command count")
        logs: list[dict[str, object]] = []
        seen: set[str] = set()
        total = 0
        last = {text(object_value(row)["name"]) for row in commands[-8:]}
        for raw in commands:
            row = object_value(raw)
            name = text(row["name"])
            if re.fullmatch("[a-z0-9-]+", name) is None or name in seen:
                raise ValueError("invalid or duplicate command identity")
            seen.add(name)
            info = archive.getinfo(name + ".log")
            total += info.file_size
            if total > 256 * 1024 * 1024:
                raise ValueError("command logs exceed aggregate read bound")
            data = read_member(archive, name + ".log", 128 * 1024 * 1024)
            if digest(data) != row["log_sha256"]:
                raise RuntimeError("command log hash differs: " + name)
            record: dict[str, object] = {
                "name": name, "bytes": len(data), "sha256": digest(data),
                "raw_exit": row["raw_exit"], "expected": row["expected"],
                "timed_out": row["timed_out"],
            }
            if name in last or row["raw_exit"] != row["expected"] or row["timed_out"]:
                decoded = data.decode("utf-8", errors="strict")
                record["head"] = decoded[:24576]
                record["tail"] = decoded[-24576:]
                record["diagnostic_lines"] = [
                    line[:2048] for line in decoded.splitlines()
                    if re.search(r"--- FAIL:|FAIL\s|error:|panic:|Fatal|fatal|want |mismatch|differs", line)
                ][:160]
            logs.append(record)
        projection = dict(report)
        pipeline = report.get("pipeline", [])
        if not isinstance(pipeline, list):
            raise ValueError("frozen pipeline is not an array")
        projected_pipeline: list[dict[str, object]] = []
        for raw in pipeline:
            row = dict(object_value(raw))
            catalog = row.pop("catalog")
            catalog_bytes = json.dumps(catalog, sort_keys=True, separators=(",", ":")).encode()
            row["catalog_projection"] = {
                "sha256": digest(catalog_bytes), "canonical_bytes": len(catalog_bytes),
                "encoding": "UTF-8 JSON, sorted keys, compact separators",
            }
            generated = row["files"]
            if not isinstance(generated, list):
                raise ValueError("generated file records are not an array")
            projected_files: list[dict[str, object]] = []
            for item in generated:
                file = dict(object_value(item))
                data = text(file.pop("content")).encode()
                if digest(data) != file["sha256"]:
                    raise RuntimeError("generated content hash differs")
                file["content_bytes"] = len(data)
                projected_files.append(file)
            row["files"] = projected_files
            projected_pipeline.append(row)
        projection["pipeline"] = projected_pipeline
        receipt = {
            "schema": 1, "producer": os.environ["GITHUB_SHA"],
            "recovered_artifact": artifact.receipt,
            "report_sha256": digest(report_bytes), "report_bytes": len(report_bytes),
            "members": [{"name": entry.filename, "bytes": entry.file_size} for entry in entries],
            "verified_command_logs": logs, "report_projection": projection,
            "projection_scope": "Full report except pipeline catalog objects and generated content; these are bound by exact hashes/lengths. The complete report is preserved separately.",
            "source_executed": False, "publication_eligible": False,
        }
    payload = (json.dumps(receipt, separators=(",", ":")) + "\n").encode()
    packed = gzip.compress(payload, mtime=0)
    encoded = base64.b64encode(packed).decode("ascii")
    chunk_size = 12288
    chunks = [encoded[index:index + chunk_size] for index in range(0, len(encoded), chunk_size)]
    header = {
        "payload_sha256": digest(payload), "payload_bytes": len(payload),
        "gzip_sha256": digest(packed), "gzip_bytes": len(packed), "chunks": len(chunks),
        "archive": artifact.receipt, "commands": len(logs),
    }
    (output / "recovery.json").write_bytes(payload)
    (output / "recovery.json.gz").write_bytes(packed)
    (output / "report.json").write_bytes(report_bytes)
    print("SPIRA_R41_RECOVERY_HEADER " + json.dumps(header), flush=True)
    for index, chunk in enumerate(chunks):
        print("SPIRA_R41_RECOVERY_PART " + json.dumps({"index": index, "data": chunk}), flush=True)
    print("SPIRA_R41_RECOVERY_SUMMARY " + json.dumps({
        "report_keys": list(report), "source_restored": report["source_restored"],
        "unexpected": [row for row in commands
                       if object_value(row)["raw_exit"] != object_value(row)["expected"]
                       or object_value(row)["timed_out"]],
        "last_commands": commands[-8:],
        "native_original_count": len(report.get("native_original", [])),
        "pipeline_count": len(report.get("pipeline", [])),
    }), flush=True)


if __name__ == "__main__":
    main()
