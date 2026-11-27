"""Authenticate the frozen complete catalog/CTE baseline before source application."""
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
        raise ValueError("selected baseline member exceeds its bound: " + name)
    with archive.open(entry) as stream:
        data = stream.read(limit + 1)
    if len(data) > limit or len(data) != entry.file_size:
        raise ValueError("selected baseline member size differs: " + name)
    return data


def check_outcomes(report: dict[str, object], key: str, names: set[str], failed: set[str]) -> None:
    actual = base.object_value(report[key])
    if set(actual["failed"]) != failed or set(actual["passed"]) != names - failed:
        raise RuntimeError("authenticated baseline outcome partition differs: " + key)


def check_pipeline(archive: zipfile.ZipFile, row: dict[str, object], prefix: str = "unchanged") -> None:
    name = base.text(row["name"])
    if row["pipeline_contract"] == "upstream":
        if (name != "nested-materialized-cte-reference" or row["raw_exit"] != 1
                or row["request_present"] is not False or row["files"] != []
                or row["processed_native"] is not None
                or row["raw_output"] != '# package spira\nqueries.sql:1:1: relation "b" does not exist\n'):
            raise RuntimeError("authenticated upstream boundary differs")
        if prefix + "-" + name + "-request.json" in archive.namelist():
            raise RuntimeError("authenticated upstream case has a plugin request")
        return
    if row["request_present"] is not True:
        raise RuntimeError("authenticated plugin request is absent: " + name)
    request = read_member(archive, prefix + "-" + name + "-request.json", 4 * 1024 * 1024)
    captured = base.object_value(json.loads(request))
    if base.digest(request) != row["request_sha256"] or captured["queries"] != [row["query_metadata"]]:
        raise RuntimeError("authenticated complete request differs: " + name)
    catalog = json.dumps(captured["catalog"], sort_keys=True, separators=(",", ":")).encode()
    if base.digest(catalog) != row["catalog_sha256"] or len(catalog) != row["catalog_canonical_bytes"]:
        raise RuntimeError("authenticated catalog differs: " + name)
    processed = base.object_value(row["processed_native"])
    if (processed["matches"] is not True or processed["sql"] != row["processed_sql"]
            or processed["sql_sha256"] != row["processed_sql_sha256"]):
        raise RuntimeError("authenticated processed native observation differs: " + name)
    generated = row["files"]
    if not isinstance(generated, list):
        raise ValueError("authenticated generated set is not an array")
    paths = [base.text(base.object_value(item)["path"]) for item in generated]
    wanted = ["models.sql.lgo", "queries.sql.lgo"] if row["pipeline_contract"] == "generated" else []
    if paths != wanted or row["raw_exit"] != (0 if wanted else 1):
        raise RuntimeError("authenticated output contract differs: " + name)
    for raw in generated:
        item = base.object_value(raw)
        if base.digest(base.text(item["content"]).encode()) != item["sha256"]:
            raise RuntimeError("authenticated generated content differs: " + name)


def authenticate(manifest: dict[str, object], token: str) -> tuple[dict[str, object], dict[str, object]]:
    artifact = download(base.object_value(manifest["baseline_artifact"]), token)
    frozen_bytes = (base.PROOF / "baseline-report.json").read_bytes()
    frozen = base.object_value(json.loads(frozen_bytes))
    specification = base.object_value(manifest["baseline_verification"])
    catalog = observer.rows_from("version-catalog.json", 11)
    ctes = observer.rows_from("version-ctes.json", 26)
    cases = catalog + ctes
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        members = archive.infolist()
        if len(members) > 512:
            raise ValueError("baseline archive exceeds its member-count bound")
        names: set[str] = set()
        for entry in members:
            path = PurePosixPath(entry.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or entry.filename in names):
                raise ValueError("invalid baseline archive member")
            names.add(entry.filename)
        report_bytes = read_member(archive, "report.json", 16 * 1024 * 1024)
        report = base.object_value(json.loads(report_bytes))
        if report != frozen or base.digest(report_bytes) != specification["report_sha256"]:
            raise RuntimeError("complete baseline report differs")
        if (report["baseline_verified"] is not True or report["observations_complete"] is not True
                or report["source_restored"] is not True or report["source_gates_passed"] is not False
                or report["publication_eligible"] is not False):
            raise RuntimeError("baseline status differs")
        original_manifest = base.object_value(report["manifest"])
        if report["manifest_sha256"] != specification["manifest_sha256"]:
            raise RuntimeError("authenticated baseline manifest differs")
        if (report["producer"] != artifact.receipt["producer"]
                or int(base.text(report["run"])) != artifact.receipt["run"]):
            raise RuntimeError("authenticated baseline producer differs")
        commands = report["commands"]
        if not isinstance(commands, list) or len(commands) != specification["command_count"]:
            raise RuntimeError("baseline command count differs")
        command_names: set[str] = set()
        log_bytes = 0
        for raw in commands:
            row = base.object_value(raw)
            name = base.text(row["name"])
            if (re.fullmatch(r"[a-z0-9-]+", name) is None or name in command_names
                    or row["raw_exit"] != row["expected"] or row["timed_out"] is not False):
                raise RuntimeError("baseline receipt identity or outcome differs")
            command_names.add(name)
            data = read_member(archive, name + ".log", 16 * 1024 * 1024)
            log_bytes += len(data)
            if log_bytes > 128 * 1024 * 1024 or base.digest(data) != row["log_sha256"]:
                raise RuntimeError("baseline command log bound or digest differs: " + name)
        original = report["native_original"]
        pipeline = report["pipeline"]
        if not isinstance(original, list) or not isinstance(pipeline, list):
            raise ValueError("baseline observations are not arrays")
        wanted = [base.text(case["name"]) for case in cases]
        if ([base.object_value(row)["name"] for row in original] != wanted
                or [base.object_value(row)["name"] for row in pipeline] != wanted):
            raise RuntimeError("baseline observation coverage or order differs")
        for case, raw in zip(cases, original, strict=True):
            row = base.object_value(raw)
            expected = {"name": case["native_name"],
                        "versions": case.get("native_versions", [case.get("native_version")])}
            if (row["matches"] is not True or row["observed"] != expected or row["expected"] != expected
                    or row["sql"] != case["sql"]
                    or row["sql_sha256"] != base.digest(base.text(case["sql"]).encode())):
                raise RuntimeError("baseline original native contract differs")
        for raw in pipeline:
            check_pipeline(archive, base.object_value(raw))
    before = base.object_value(report["before"])
    source = base.object_value(manifest["source"])
    tests = base.object_value(manifest["original_tests"])
    if (before != report["restored"] or before["tree"] != source["tree"]
            or before["blobs"] != source["blobs"]
            or report["tests_before"] != report["tests_after"]
            or report["tests_before"] != report["current_model_restored"]):
        raise RuntimeError("baseline source or restoration differs")
    observed_tests = base.object_value(report["tests_before"])
    if observed_tests["tree"] != tests["tree"] or observed_tests["blobs"] != tests["blobs"]:
        raise RuntimeError("baseline test snapshot differs")
    counter = base.object_value(report["counterfactual_before"])
    if (counter != report["counterfactual_after"]
            or counter["tree"] != base.object_value(original_manifest["counterfactual"])["tree"]):
        raise RuntimeError("baseline one-model counterfactual differs")
    original_hashes = base.object_value(original_manifest["proof_sha256"])
    frozen_inputs = {"authored-tests.json": "original-authored-tests.json",
                     "source-index.json": "source-index.json", "test-index.json": "original-test-index.json",
                     "version-catalog.json": "version-catalog.json", "version-ctes.json": "version-ctes.json"}
    for name, current_name in frozen_inputs.items():
        if base.digest((base.PROOF / current_name).read_bytes()) != original_hashes[name]:
            raise RuntimeError("candidate proof changed a frozen baseline input: " + name)
    contracts = base.object_value(original_manifest["contracts"])
    check_outcomes(report, "catalog_red", {base.text(row["name"]) for row in catalog},
                   observer.names(contracts["catalog_failed"]))
    check_outcomes(report, "cte_red", {base.text(row["name"]) for row in ctes},
                   observer.names(contracts["cte_failed"]))
    permanent = observer.names(contracts["pipeline_names"])
    if len(permanent) != 60:
        raise RuntimeError("baseline permanent pipeline name set differs")
    check_outcomes(report, "current_pipeline", permanent, set())
    check_outcomes(report, "counterfactual_pipeline", permanent,
                   observer.names(contracts["counterfactual_failed"]))
    if report["pipeline_partition"] != {"generated": 7, "plugin-refusal": 29, "upstream": 1}:
        raise RuntimeError("baseline pipeline partition differs")
    receipt: dict[str, object] = {
        **artifact.receipt, "report_sha256": base.digest(report_bytes),
        "command_logs_verified": len(command_names), "command_log_bytes": log_bytes,
        "complete_requests_verified": 36, "native_original": 37, "native_processed": 36,
        "catalog_red": 5, "catalog_controls": 6, "cte_red": 11, "cte_controls": 15,
        "current_permanent_passes": 60, "counterfactual_red": 12, "counterfactual_controls": 48,
        "source_restored": True, "baseline_verified": True,
        "reused_scope": "Exact unchanged-production RED/control outcomes, actual SQLc request metadata and native values, and the one-model permanent-test counterfactual.",
        "full_source_gates_reused": False,
    }
    (base.OUTPUT / "authenticated-baseline.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                           encoding="utf-8")
    print("AUTHENTICATED_SPIRA_R41_BASELINE " + json.dumps(receipt), flush=True)
    return receipt, report
