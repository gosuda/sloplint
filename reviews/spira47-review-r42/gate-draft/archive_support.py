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


def check_partition(report: dict[str, object], key: str, names: set[str],
                    failed: set[str]) -> None:
    actual = base.object_value(report[key])
    if set(actual["failed"]) != failed or set(actual["passed"]) != names - failed:
        raise RuntimeError("frozen direct outcome partition differs: " + key)


def authenticate_baseline(manifest: dict[str, object], token: str) -> tuple[dict[str, object], dict[str, object]]:
    artifact = download(base.object_value(manifest["baseline_artifact"]), token)
    cases = observer.rows_from("version-relation-roles.json", 30)
    contracts = base.object_value(manifest["contracts"])
    upstream = base.object_value(contracts["role_upstream"])
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        check_members(archive)
        report = frozen_report(archive, manifest, "baseline")
        if (report["baseline_verified"] is not True or report["observations_complete"] is not True
                or report["source_restored"] is not True or report["source_gates_passed"] is not False
                or report["publication_eligible"] is not False):
            raise RuntimeError("complete baseline status differs")
        logs = check_logs(archive, report)
        original, pipeline = report["native_original"], report["pipeline"]
        if not isinstance(original, list) or not isinstance(pipeline, list):
            raise ValueError("baseline observations are not arrays")
        wanted = [base.text(case["name"]) for case in cases]
        if ([base.object_value(row)["name"] for row in original] != wanted
                or [base.object_value(row)["name"] for row in pipeline] != wanted):
            raise RuntimeError("complete baseline coverage or order differs")
        for case, native_row, pipeline_row in zip(cases, original, pipeline, strict=True):
            check_native(case, base.object_value(native_row))
            check_pipeline(archive, base.object_value(pipeline_row), "unchanged", upstream)
    names = {base.text(case["name"]) for case in cases}
    check_partition(report, "current_roles", names, observer.names(contracts["current_failed"]))
    check_partition(report, "counterfactual_roles", names, observer.names(contracts["counterfactual_failed"]))
    source = base.object_value(manifest["source"])
    tests = base.object_value(manifest["baseline_tests"])
    counterfactual = base.object_value(manifest["baseline_counterfactual"])
    before = base.object_value(report["before"])
    observed_tests = base.object_value(report["tests_before"])
    observed_counterfactual = base.object_value(report["counterfactual_before"])
    if (before != report["restored"] or before["tree"] != source["tree"]
            or before["blobs"] != source["blobs"]
            or observed_tests != report["tests_after"]
            or observed_tests["tree"] != tests["tree"] or observed_tests["blobs"] != tests["blobs"]
            or observed_counterfactual != report["counterfactual_after"]
            or observed_counterfactual["tree"] != counterfactual["tree"]
            or observed_counterfactual["blobs"] != counterfactual["blobs"]
            or report["pipeline_partition"] != {"generated": 25, "plugin-refusal": 3, "upstream": 2}):
        raise RuntimeError("baseline source, restoration or pipeline partition differs")
    receipt = {**artifact.receipt, "report_sha256": base.object_value(manifest["baseline_verification"])["report_sha256"],
               "command_logs_verified": logs, "native_cases": 30, "pipeline_cases": 30,
               "source_restoration_verified": True, "full_source_gates_reused": False}
    print("R42_BASELINE_AUTHENTICATED " + json.dumps(receipt), flush=True)
    return receipt, report


def authenticate_prior(manifest: dict[str, object], token: str) -> tuple[dict[str, object], dict[str, object]]:
    artifact = download(base.object_value(manifest["prior_artifact"]), token)
    cases = observer.rows_from("version-catalog.json", 11) + observer.rows_from("version-ctes.json", 26)
    wanted = [base.text(case["name"]) for case in cases]
    wanted_set = set(wanted)
    upstream = {"nested-materialized-cte-reference": 'queries.sql:1:1: relation "b" does not exist'}
    selected_logs = {"original-" + name for name in wanted}
    selected_logs.update({"retained-direct-runtime", "retained-pipeline-runtime", "retained-generator-controls"})
    selected_logs.update("candidate-sqlc-" + name for name in wanted)
    selected_logs.update("candidate-processed-" + name for name in wanted if name not in upstream)
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        check_members(archive)
        report = frozen_report(archive, manifest, "prior")
        if (report["source_gates_passed"] is not True or report["focused_gates_passed"] is not True
                or report["source_restored"] is not True or report["publication_eligible"] is not True):
            raise RuntimeError("prior published-source result differs")
        logs = check_logs(archive, report, selected_logs)
        original, pipeline = report["native_original"], report["candidate_pipeline"]
        if not isinstance(original, list) or not isinstance(pipeline, list):
            raise ValueError("prior observations are not arrays")
        native_rows = [base.object_value(row) for row in original if base.object_value(row)["name"] in wanted_set]
        pipeline_rows = [base.object_value(row) for row in pipeline if base.object_value(row)["name"] in wanted_set]
        if ([row["name"] for row in native_rows] != wanted or [row["name"] for row in pipeline_rows] != wanted):
            raise RuntimeError("selected prior coverage or order differs")
        for case, native_row, pipeline_row in zip(cases, native_rows, pipeline_rows, strict=True):
            check_native(case, native_row)
            check_pipeline(archive, pipeline_row, "candidate", upstream)
    candidate = base.object_value(report["candidate_before"])
    source = base.object_value(manifest["source"])
    if (candidate != report["candidate_after"] or candidate["tree"] != source["tree"]
            or candidate["blobs"] != source["blobs"] or report["before"] != report["restored"]):
        raise RuntimeError("prior published source or restoration differs")
    selected = {"native_original": native_rows, "pipeline": pipeline_rows}
    receipt = {**artifact.receipt, "report_sha256": base.object_value(manifest["prior_verification"])["report_sha256"],
               "selected_unique_command_logs_verified": logs, "native_cases": 37, "pipeline_cases": 37,
               "selection": wanted, "full_source_gates_reused": False,
               "log_scope": "Only the 113 unique selected-case, runtime and retained-control logs are read. The prior 204 receipts have 198 per-name files; six repeated setup names are not included."}
    print("R42_PRIOR_CASES_AUTHENTICATED " + json.dumps(receipt), flush=True)
    return receipt, selected
