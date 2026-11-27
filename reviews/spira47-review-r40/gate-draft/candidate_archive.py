"""Authenticate the complete fixed baseline archive before reusing its observations."""
from __future__ import annotations

import io
import json
import re
import zipfile
from pathlib import PurePosixPath

from artifact_support import download
import baseline_support as base


def read_member(archive: zipfile.ZipFile, name: str, limit: int) -> bytes:
    entry = archive.getinfo(name)
    if entry.file_size > limit:
        raise ValueError("selected baseline member exceeds its bound: " + name)
    with archive.open(entry) as stream:
        data = stream.read(limit + 1)
    if len(data) > limit or len(data) != entry.file_size:
        raise ValueError("selected baseline member size differs: " + name)
    return data


def authenticate(manifest: dict[str, object], token: str) -> tuple[dict[str, object], dict[str, object]]:
    artifact = download(base.object_value(manifest["baseline_artifact"]), token)
    frozen = base.object_value(json.loads((base.PROOF / "baseline-report.json").read_bytes()))
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        members = archive.infolist()
        if len(members) > 128:
            raise ValueError("baseline archive exceeds its member-count bound")
        names: set[str] = set()
        for entry in members:
            path = PurePosixPath(entry.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or entry.filename in names):
                raise ValueError("invalid baseline archive member")
            names.add(entry.filename)
        report_bytes = read_member(archive, "report.json", 4 * 1024 * 1024)
        report = base.object_value(json.loads(report_bytes))
        if report != frozen:
            raise RuntimeError("complete baseline report differs")
        if (report["baseline_verified"] is not True or report["observations_complete"] is not True
                or report["source_restored"] is not True or report["publication_eligible"] is not False):
            raise RuntimeError("baseline status differs")
        commands = report["commands"]
        if not isinstance(commands, list) or len(commands) != 82:
            raise RuntimeError("baseline command count differs")
        command_names: set[str] = set()
        for raw in commands:
            row = base.object_value(raw)
            name = base.text(row["name"])
            if (re.fullmatch(r"[a-z0-9-]+", name) is None or name in command_names
                    or row["raw_exit"] != row["expected"] or row["timed_out"] is not False):
                raise RuntimeError("baseline receipt identity or outcome differs")
            command_names.add(name)
            data = read_member(archive, name + ".log", 4 * 1024 * 1024)
            if base.digest(data) != row["log_sha256"]:
                raise RuntimeError("baseline command log digest differs: " + name)
        argv_bytes = read_member(archive, "logos-argv.jsonl", 128 * 1024)
        launcher_bytes = read_member(archive, "test-process-argv.jsonl", 128 * 1024)
    argv = [base.object_value(json.loads(line)) for line in argv_bytes.splitlines()]
    nested = [row for row in argv if row["module"] == "module example.com/spira-sqlc-runtime"]
    if (nested != report["nested_test_argv"] or len(nested) != 2
            or len({base.text(row["cwd"]) for row in nested}) != 2
            or any(row["argv"] != ["test", "-count=1", "-v", "."] for row in nested)):
        raise RuntimeError("baseline missing-race observation differs")
    launchers = [base.object_value(json.loads(line)) for line in launcher_bytes.splitlines()]
    if len(launchers) != 2:
        raise RuntimeError("baseline outer launcher count differs")
    before = base.object_value(report["before"])
    if (before != report["restored"] or before["tree"] != base.object_value(manifest["source"])["tree"]
            or before["blobs"] != 621 or report["tests_before"] != report["tests_after"]):
        raise RuntimeError("baseline source or restoration differs")
    tests = base.object_value(report["tests_before"])
    if tests["tree"] != "0d6a92efd9b8d24e58cdcadad0253c2565759994" or tests["blobs"] != 623:
        raise RuntimeError("baseline test snapshot differs")
    cases: list[dict[str, object]] = json.loads((base.PROOF / "version-references.json").read_bytes())
    wanted = {base.text(case["name"]) for case in cases}
    original = report["native_original"]
    pipeline = report["pipeline"]
    if not isinstance(original, list) or not isinstance(pipeline, list):
        raise ValueError("baseline observations are not arrays")
    if (len(original) != 20 or len(pipeline) != 20
            or {base.text(base.object_value(row)["name"]) for row in original} != wanted
            or {base.text(base.object_value(row)["name"]) for row in pipeline} != wanted):
        raise RuntimeError("baseline observation coverage differs")
    for raw in original:
        if base.object_value(raw)["matches"] is not True:
            raise RuntimeError("baseline native original did not match")
    for raw in pipeline:
        row = base.object_value(raw)
        if (row["raw_exit"] != 0 or row["observed_generator_accepts"] is not True
                or base.object_value(row["processed_native"])["matches"] is not True):
            raise RuntimeError("baseline actual generation or native outcome differs")
    outcomes = base.object_value(report["generator"])
    failed = {base.text(case["name"]) for case in cases if case["allow"] is False}
    passed = wanted - failed
    if set(outcomes["failed"]) != failed or set(outcomes["passed"]) != passed:
        raise RuntimeError("baseline 12-RED/eight-control partition differs")
    receipt: dict[str, object] = {
        **artifact.receipt, "report_sha256": base.digest(report_bytes),
        "command_logs_verified": len(command_names), "argv_sha256": base.digest(argv_bytes),
        "launcher_sha256": base.digest(launcher_bytes), "missing_race_argv": nested,
        "native_original": len(original), "native_captured": len(pipeline),
        "generator_red": len(failed), "generator_controls": len(passed),
        "source_restored": True, "baseline_verified": True,
        "reused_scope": "Frozen old20 real SQLc request metadata, native values and actual missing-race records.",
        "full_source_gates_reused": False,
    }
    (base.OUTPUT / "authenticated-baseline.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                           encoding="utf-8")
    print("AUTHENTICATED_SPIRA_R40_BASELINE " + json.dumps(receipt), flush=True)
    return receipt, report
