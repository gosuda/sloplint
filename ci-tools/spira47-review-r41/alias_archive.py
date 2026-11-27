"""Authenticate the rejected materialized SELECT alias proposal and its frozen controls."""
from __future__ import annotations

import io
import json
import re
import zipfile
from pathlib import PurePosixPath

from artifact_support import download
import baseline_support as base
from candidate_archive import check_outcomes, check_pipeline, read_member
import observer


def authenticate(manifest: dict[str, object], token: str) -> tuple[dict[str, object], dict[str, object]]:
    artifact = download(base.object_value(manifest["alias_artifact"]), token)
    frozen = base.object_value(json.loads((base.PROOF / "alias-report.json").read_bytes()))
    specification = base.object_value(manifest["alias_verification"])
    cases = observer.rows_from("version-projections.json", 10)
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        members = archive.infolist()
        if len(members) > 256:
            raise ValueError("alias archive exceeds its member-count bound")
        names: set[str] = set()
        for entry in members:
            path = PurePosixPath(entry.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or entry.filename in names):
                raise ValueError("invalid alias archive member")
            names.add(entry.filename)
        report_bytes = read_member(archive, "report.json", 8 * 1024 * 1024)
        report = base.object_value(json.loads(report_bytes))
        if report != frozen or base.digest(report_bytes) != specification["report_sha256"]:
            raise RuntimeError("complete alias report differs")
        if (report["baseline_verified"] is not True or report["observations_complete"] is not True
                or report["source_restored"] is not True or report["source_gates_passed"] is not False
                or report["publication_eligible"] is not False):
            raise RuntimeError("alias diagnostic status differs")
        original_manifest = base.object_value(report["manifest"])
        if report["manifest_sha256"] != specification["manifest_sha256"]:
            raise RuntimeError("alias manifest differs")
        if (report["producer"] != artifact.receipt["producer"]
                or int(base.text(report["run"])) != artifact.receipt["run"]):
            raise RuntimeError("alias producer differs")
        commands = report["commands"]
        if not isinstance(commands, list) or len(commands) != specification["command_count"]:
            raise RuntimeError("alias command count differs")
        command_names: set[str] = set()
        log_bytes = 0
        for raw in commands:
            row = base.object_value(raw)
            name = base.text(row["name"])
            if (re.fullmatch(r"[a-z0-9-]+", name) is None or name in command_names
                    or row["raw_exit"] != row["expected"] or row["timed_out"] is not False):
                raise RuntimeError("alias receipt identity or outcome differs")
            command_names.add(name)
            data = read_member(archive, name + ".log", 8 * 1024 * 1024)
            log_bytes += len(data)
            if log_bytes > 64 * 1024 * 1024 or base.digest(data) != row["log_sha256"]:
                raise RuntimeError("alias command log bound or digest differs")
        original = report["native_original"]
        unchanged = report["unchanged_pipeline"]
        proposed = report["proposed_pipeline"]
        if not isinstance(original, list) or not isinstance(unchanged, list) or not isinstance(proposed, list):
            raise ValueError("alias observations are not arrays")
        wanted = [base.text(case["name"]) for case in cases]
        for values in (original, unchanged, proposed):
            if [base.object_value(row)["name"] for row in values] != wanted:
                raise RuntimeError("alias observation coverage or order differs")
        for case, raw in zip(cases, original, strict=True):
            row = base.object_value(raw)
            expected = {"name": case["native_name"], "versions": [case["native_version"]]}
            if (row["matches"] is not True or row["expected"] != expected or row["observed"] != expected
                    or row["sql"] != case["sql"]
                    or row["sql_sha256"] != base.digest(base.text(case["sql"]).encode())):
                raise RuntimeError("alias original native contract differs")
        for lane, rows in (("unchanged", unchanged), ("proposed", proposed)):
            for raw in rows:
                check_pipeline(archive, base.object_value(raw), lane)
    before = base.object_value(report["before"])
    source = base.object_value(manifest["source"])
    tests = base.object_value(manifest["tests"])
    if (before != report["restored"] or before["tree"] != source["tree"] or before["blobs"] != 625
            or report["tests_before"] != report["tests_after"]
            or report["proposed_before"] != report["proposed_after"]):
        raise RuntimeError("alias source or restoration differs")
    test_snapshot = base.object_value(report["tests_before"])
    proposed_snapshot = base.object_value(report["proposed_before"])
    if (test_snapshot["tree"] != tests["tree"] or test_snapshot["blobs"] != tests["blobs"]
            or proposed_snapshot["tree"] != specification["rejected_proposal_tree"]
            or proposed_snapshot["blobs"] != 631):
        raise RuntimeError("alias source snapshot differs")
    original_hashes = base.object_value(original_manifest["proof_sha256"])
    for name in ("authored-tests.json", "test-index.json", "source-index.json", "version-projections.json"):
        if base.digest((base.PROOF / name).read_bytes()) != original_hashes[name]:
            raise RuntimeError("final proof changed a frozen alias input: " + name)
    names = {base.text(case["name"]) for case in cases}
    check_outcomes(report, "unchanged_projection", names,
                   {base.text(case["name"]) for case in cases if case["allow"] is True})
    check_outcomes(report, "proposed_projection", names,
                   {"cte-select-quoted-alias", "cte-select-bare-alias"})
    if (report["unchanged_pipeline_partition"] != {"generated": 0, "plugin-refusal": 10}
            or report["proposed_pipeline_partition"] != {"generated": 9, "plugin-refusal": 1}):
        raise RuntimeError("alias pipeline partition differs")
    receipt: dict[str, object] = {
        **artifact.receipt, "report_sha256": base.digest(report_bytes),
        "command_logs_verified": len(command_names), "command_log_bytes": log_bytes,
        "complete_requests_verified": 20, "native_original": 10, "native_processed": 20,
        "maintained_genuine_reference_red": 7, "maintained_alias_controls": 3,
        "rejected_proposal_alias_red": 2, "rejected_proposal_controls": 8,
        "source_restored": True, "baseline_verified": True,
        "reused_scope": "Exact ten added source tests, native values and unchanged SQLc handoffs, plus authentic rejection of the unrepaired proposal.",
        "full_source_gates_reused": False,
    }
    (base.OUTPUT / "authenticated-alias-baseline.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                                                 encoding="utf-8")
    print("AUTHENTICATED_SPIRA_R41_ALIAS_BASELINE " + json.dumps(receipt), flush=True)
    return receipt, report
