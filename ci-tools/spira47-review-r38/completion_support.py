"""Authenticate the completed focused evidence while preserving its failed full gate."""
from __future__ import annotations

import io
import json
import zipfile
from pathlib import PurePosixPath

import artifact_support as artifacts
import baseline_support as base
import verify_candidate as full
import verify_baseline as observation


def rows(value: object) -> list[dict[str, object]]:
    if not isinstance(value, list):
        raise ValueError("expected a list of records")
    return [base.object_value(row) for row in value]


def names(value: object) -> list[str]:
    if not isinstance(value, list):
        raise ValueError("expected a list of names")
    return [base.text(name) for name in value]


def require_native(value: object, count: int) -> None:
    records = rows(value)
    if len(records) != count or any(row["matches"] is not True for row in records):
        raise RuntimeError("frozen native values differ")


def require_compiled(value: object, count: int) -> None:
    record = base.object_value(value)
    observed = base.object_value(record["observed"])
    if (len(observed) != count or observed != record["expected"]
            or record["mismatches"] != []):
        raise RuntimeError("compiled byte contract differs")


def authenticate(manifest: dict[str, object], original: dict[str, object],
                 token: str) -> dict[str, object]:
    specification = base.object_value(manifest["focused_artifact"])
    archive = artifacts.download(specification, token)
    frozen = observation.read_object("failed-candidate-report.json")
    logs: dict[str, str] = {}
    with zipfile.ZipFile(io.BytesIO(archive.data)) as bundle:
        members = bundle.infolist()
        if len(members) > 512 or sum(row.file_size for row in members) > 64 * 1024 * 1024:
            raise RuntimeError("focused archive exceeds its fixed bounds")
        for member in members:
            path = PurePosixPath(member.filename)
            if path.is_absolute() or ".." in path.parts or member.file_size > 16 * 1024 * 1024:
                raise RuntimeError("invalid focused archive member")
        report = base.object_value(json.loads(bundle.read("report.json")))
        if report != frozen or report["manifest"] != original:
            raise RuntimeError("frozen focused report or source manifest differs")
        if (report["source_gates_passed"] is not False or report["source_restored"] is not True
                or report["publication_eligible"] is not False):
            raise RuntimeError("the failed producer was relabeled")
        commands = rows(report["commands"])
        if len(commands) != 195:
            raise RuntimeError("focused command count differs")
        for command in commands:
            name = base.text(command["name"])
            if name in logs or PurePosixPath(name).name != name:
                raise RuntimeError("duplicate or invalid command name")
            data = bundle.read(name + ".log")
            if base.digest(data) != command["log_sha256"] or command["timed_out"] is not False:
                raise RuntimeError("focused command log or timeout differs: " + name)
            logs[name] = data.decode("utf-8", errors="strict")
            if name == "just-gate":
                if (command["command"] != ["just", "gate"] or command["raw_exit"] != 127
                        or command["expected"] != 0
                        or command["log_sha256"] != manifest["failed_gate_log_sha256"]
                        or "rg: command not found" not in logs[name]):
                    raise RuntimeError("the original gate failure differs")
            elif command["raw_exit"] != command["expected"]:
                raise RuntimeError("an unaccepted focused command failed: " + name)
        if "just-gate" not in logs or any(name in logs for name in ("just-slop", "just-eject")):
            raise RuntimeError("the prior full gate boundary differs")
    before = base.object_value(report["before"])
    restored = base.object_value(report["restored"])
    candidate = base.object_value(report["candidate_before"])
    if (before != restored or before["tree"] != base.object_value(original["source"])["tree"]
            or candidate != report["after_focused"]
            or candidate["tree"] != base.object_value(original["candidate"])["tree"]
            or candidate["index_sha256"] != manifest["candidate_index_sha256"]):
        raise RuntimeError("focused source or restoration identity differs")
    unchanged = base.object_value(report["unchanged_tests"])
    if (unchanged["before"] != unchanged["after"]
            or base.object_value(unchanged["before"])["tree"]
            != base.object_value(original["current_tests"])["tree"]):
        raise RuntimeError("unchanged-production source identity differs")
    for field, passing, failing in (("dollar", 3, 11), ("pipeline", 29, 11)):
        result = base.object_value(unchanged[field])
        if len(names(result["passed"])) != passing or len(names(result["failed"])) != failing:
            raise RuntimeError("unchanged-production failure partition differs")
    if set(names(base.object_value(unchanged["compiled_direct"])["mismatches"])) != {
        "runtime-cr", "runtime-crlf", "runtime-cr-value", "runtime-crlf-value",
    }:
        raise RuntimeError("unchanged compiled-value witness differs")
    dollars = full.read_rows("dollar-strings-final.json", 14)
    old_cases = full.read_rows("retained-native-cases.json", 26)
    cases = old_cases + dollars
    for field, test, values, log_name in (
        ("dollar_green", "TestPostgresDollarQuotedBoundaries", dollars, "dollar-generator-green"),
        ("pipeline_green", "TestSQLcPostgresLiteralPipeline", cases, "permanent-pipeline-green"),
    ):
        actual = full.require_cases(logs[log_name], test, values, set())
        if actual != report[field]:
            raise RuntimeError("focused literal PASS observations differ")
    full.require_passes(logs["direct-compiled-green"], {"TestGeneratedQueryConstants"})
    retained = {"TestPostgresContinuedEscapeStrings/" + base.text(row["name"]) for row in old_cases[:14]}
    retained |= {"TestPostgresTypedLiteralBoundaries/" + base.text(row["name"]) for row in old_cases[14:]}
    full.require_passes(logs["retained-generator-controls"], retained | {
        "TestPostgresContinuedEscapeStrings", "TestPostgresTypedLiteralBoundaries",
    })
    require_compiled(report["compiled_direct"], 8)
    require_compiled(report["compiled_pipeline"], 32)
    require_native(report["original_native"], 48)
    require_native(report["direct_compiled_native"], 8)
    require_native(report["pipeline_compiled_native"], 32)
    pipeline = rows(report["native_pipeline"])
    counts = {name: sum(row["outcome"] == name for row in pipeline)
              for name in ("generated", "version-refusal", "upstream-rejection")}
    captured = [row for row in pipeline if row["captured_request"] is True]
    if (len(pipeline) != 40 or len(captured) != 36
            or counts != {"generated": 32, "version-refusal": 4, "upstream-rejection": 4}
            or any(base.object_value(row["processed_native"])["matches"] is not True for row in captured)):
        raise RuntimeError("native sqlc/plugin observations differ")
    if report["native_version"] != original["postgres_version_num"]:
        raise RuntimeError("native PostgreSQL version differs")
    (base.OUTPUT / "prior-failed-source-proof.zip").write_bytes(archive.data)
    (base.OUTPUT / "prior-failed-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    (base.OUTPUT / "prior-failed-just-gate.log").write_text(logs["just-gate"], encoding="utf-8")
    receipt = {
        **archive.receipt, "command_logs_verified": len(logs), "producer_remains_failed": True,
        "failed_gate": "just gate: raw 127, rg missing; slop/eject not reached",
        "reused_scope": "Frozen unchanged-production REDs, focused candidate tests and native PostgreSQL observations only.",
        "full_source_gate_reuse": False, "candidate": candidate, "original": before,
        "native_original": 48, "native_captured": 36, "native_compiled": 40,
        "pipeline_partition": counts,
    }
    print("AUTHENTICATED_SPIRA_R38_FOCUSED " + json.dumps(receipt), flush=True)
    return receipt
