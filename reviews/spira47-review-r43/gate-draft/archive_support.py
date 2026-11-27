"""Authenticate the prior source result and complete unchanged R42 baseline."""
from __future__ import annotations

import io
import json
import re
import zipfile
from pathlib import PurePosixPath

from artifact_support import download
import baseline_support as base
import observer


def read_member(archive: zipfile.ZipFile, name: str, limit: int) -> bytes:
    entry = archive.getinfo(name)
    if entry.file_size > limit:
        raise ValueError("selected evidence member exceeds its bound: " + name)
    with archive.open(entry) as stream:
        data = stream.read(limit + 1)
    if len(data) > limit or len(data) != entry.file_size:
        raise ValueError("selected evidence member size differs: " + name)
    return data


def check_members(archive: zipfile.ZipFile) -> None:
    entries = archive.infolist()
    if len(entries) > 512:
        raise ValueError("evidence archive exceeds its member-count bound")
    names: set[str] = set()
    for entry in entries:
        path = PurePosixPath(entry.filename)
        if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                or entry.filename in names):
            raise ValueError("invalid evidence archive member")
        names.add(entry.filename)


def frozen_report(archive: zipfile.ZipFile, manifest: dict[str, object],
                  prefix: str) -> dict[str, object]:
    spec = base.object_value(manifest[prefix + "_verification"])
    data = read_member(archive, "report.json", 16 * 1024 * 1024)
    report = base.object_value(json.loads(data))
    frozen = base.object_value(json.loads((base.PROOF / (prefix + "-report.json")).read_bytes()))
    if (report != frozen or base.digest(data) != spec["report_sha256"]
            or report["manifest_sha256"] != spec["manifest_sha256"]):
        raise RuntimeError("frozen complete report differs: " + prefix)
    artifact = base.object_value(manifest[prefix + "_artifact"])
    if (report["producer"] != artifact["producer"]
            or int(base.text(report["run"])) != artifact["run"]):
        raise RuntimeError("frozen report producer differs: " + prefix)
    commands = report["commands"]
    if not isinstance(commands, list) or len(commands) != spec["command_count"]:
        raise ValueError("frozen command count differs: " + prefix)
    for raw in commands:
        row = base.object_value(raw)
        if row["raw_exit"] != row["expected"] or row["timed_out"] is not False:
            raise RuntimeError("frozen command outcome differs: " + prefix)
    return report


def check_logs(archive: zipfile.ZipFile, report: dict[str, object],
               selected: set[str] | None = None) -> int:
    commands = report["commands"]
    if not isinstance(commands, list):
        raise ValueError("frozen commands are not an array")
    seen: set[str] = set()
    size = 0
    for raw in commands:
        row = base.object_value(raw)
        name = base.text(row["name"])
        if selected is not None and name not in selected:
            continue
        if re.fullmatch("[a-z0-9-]+", name) is None or name in seen:
            raise RuntimeError("selected command identity is not unique")
        seen.add(name)
        data = read_member(archive, name + ".log", 16 * 1024 * 1024)
        size += len(data)
        if size > 128 * 1024 * 1024 or base.digest(data) != row["log_sha256"]:
            raise RuntimeError("selected command log bound or hash differs: " + name)
    if selected is not None and seen != selected:
        raise RuntimeError("selected command-log set is incomplete")
    return len(seen)


def check_native(case: dict[str, object], row: dict[str, object]) -> None:
    expected = {"name": case["native_name"],
                "versions": case.get("native_versions", [case.get("native_version")])}
    if (row["name"] != case["name"] or row["sql"] != case["sql"]
            or row["sql_sha256"] != base.digest(base.text(case["sql"]).encode())
            or row["expected"] != expected or row["observed"] != expected
            or row["matches"] is not True):
        raise RuntimeError("frozen native contract differs: " + base.text(case["name"]))


def check_pipeline(archive: zipfile.ZipFile, row: dict[str, object], prefix: str,
                   upstream: dict[str, object]) -> None:
    name = base.text(row["name"])
    if row["pipeline_contract"] == "upstream":
        expected_output = "# package spira\n" + base.text(upstream[name]) + "\n"
        if (row["raw_exit"] != 1 or row["request_present"] is not False
                or row["files"] != [] or row["processed_native"] is not None
                or row["raw_output"] != expected_output):
            raise RuntimeError("frozen upstream boundary differs: " + name)
        if prefix + "-" + name + "-request.json" in archive.namelist():
            raise RuntimeError("frozen upstream input has a captured request")
        return
    if name in upstream or row["request_present"] is not True:
        raise RuntimeError("frozen plugin request contract differs: " + name)
    data = read_member(archive, prefix + "-" + name + "-request.json", 4 * 1024 * 1024)
    request = base.object_value(json.loads(data))
    if base.digest(data) != row["request_sha256"] or request["queries"] != [row["query_metadata"]]:
        raise RuntimeError("complete captured request differs: " + name)
    catalog = json.dumps(request["catalog"], sort_keys=True, separators=(",", ":")).encode()
    if base.digest(catalog) != row["catalog_sha256"] or len(catalog) != row["catalog_canonical_bytes"]:
        raise RuntimeError("captured catalog differs: " + name)
    processed = base.object_value(row["processed_native"])
    if (processed["matches"] is not True or processed["sql"] != row["processed_sql"]
            or processed["sql_sha256"] != row["processed_sql_sha256"]):
        raise RuntimeError("processed native contract differs: " + name)
    files = row["files"]
    if not isinstance(files, list):
        raise ValueError("generated file set is not an array")
    expected = ["models.sql.lgo", "queries.sql.lgo"] if row["pipeline_contract"] == "generated" else []
    if ([base.object_value(item)["path"] for item in files] != expected
            or row["raw_exit"] != (0 if expected else 1)):
        raise RuntimeError("generated file contract differs: " + name)
    for raw in files:
        item = base.object_value(raw)
        if base.digest(base.text(item["content"]).encode()) != item["sha256"]:
            raise RuntimeError("generated content hash differs: " + name)


def authenticate_previous(manifest: dict[str, object], token: str) -> dict[str, object]:
    artifact = download(base.object_value(manifest["previous_artifact"]), token)
    cases = observer.rows_from("previous-native-cases.json", 67)
    expectations = observer.rows_from("previous-pipeline-cases.json", 67)
    upstream = base.object_value(base.object_value(manifest["contracts"])["upstream"])
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        check_members(archive)
        report = frozen_report(archive, manifest, "previous")
        if (report["regressions_verified"] is not True or report["focused_gates_passed"] is not True
                or report["source_gates_passed"] is not True or report["source_restored"] is not True
                or report["publication_eligible"] is not True or report["unique_command_names"] is not True):
            raise RuntimeError("previous published-source status differs")
        logs = check_logs(archive, report)
        original, pipeline = report["native_original"], report["candidate_pipeline"]
        if not isinstance(original, list) or not isinstance(pipeline, list):
            raise ValueError("previous observations are not arrays")
        wanted = [base.text(case["name"]) for case in cases]
        if ([base.object_value(row)["name"] for row in original] != wanted
                or [base.object_value(row)["name"] for row in pipeline] != wanted):
            raise RuntimeError("previous native and pipeline case order differs")
        for case, expected, native_row, pipeline_row in zip(
                cases, expectations, original, pipeline, strict=True):
            check_native(case, base.object_value(native_row))
            row = base.object_value(pipeline_row)
            check_pipeline(archive, row, "candidate", upstream)
            if (row["sql"] != expected["sql"] or row["columns"] != expected["columns"]
                    or row["option"] != expected["option"]):
                raise RuntimeError("previous fixture input metadata differs")
            if row["request_present"] is True and row["processed_sql"] != expected["text"]:
                raise RuntimeError("previous captured query text differs")
    candidate = base.object_value(report["candidate_before"])
    source = base.object_value(manifest["source"])
    if (candidate != report["candidate_after"] or candidate["tree"] != source["tree"]
            or candidate["blobs"] != source["blobs"]
            or candidate["index_sha256"] != source["native_index_sha256"]
            or report["before"] != report["restored"]
            or report["pipeline_partition"] != {"generated": 30, "plugin-refusal": 34, "upstream": 3}):
        raise RuntimeError("previous published tree or restoration differs")
    receipt = {**artifact.receipt,
               "report_sha256": base.object_value(manifest["previous_verification"])["report_sha256"],
               "unique_command_logs_verified": logs, "native_cases": 67, "pipeline_cases": 67,
               "scope": "Prior production and original fixtures only. New metadata controls and all required source gates execute in this run.",
               "full_source_gates_reused": False}
    print("R43_PREVIOUS_AUTHENTICATED " + json.dumps(receipt), flush=True)
    return receipt
