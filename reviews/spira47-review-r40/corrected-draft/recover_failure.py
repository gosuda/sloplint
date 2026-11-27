"""Authenticate and preserve the failed observer's complete raw records."""
from __future__ import annotations

import hashlib
import io
import json
import os
import zipfile
from pathlib import PurePosixPath

from artifact_support import download
import baseline_support as base


def authenticate(specification: dict[str, object]) -> dict[str, object]:
    artifact = download(specification, os.environ["GH_TOKEN"])
    prior = base.OUTPUT / "prior-failed-observer"
    prior.mkdir(exist_ok=False)
    (prior / "artifact.zip").write_bytes(artifact.data)
    with zipfile.ZipFile(io.BytesIO(artifact.data)) as archive:
        members = archive.infolist()
        if len(members) > 128 or sum(item.file_size for item in members) > 16 * 1024 * 1024:
            raise ValueError("failed observer artifact exceeds the frozen bound")
        names: set[str] = set()
        for item in members:
            path = PurePosixPath(item.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in item.filename
                    or item.filename in names or item.file_size > 4 * 1024 * 1024):
                raise ValueError("invalid failed observer artifact member")
            names.add(item.filename)
        if not {"report.json", "logos-argv.jsonl"} <= names:
            raise ValueError("failed observer artifact is incomplete")
        report_bytes = archive.read("report.json")
        argv_bytes = archive.read("logos-argv.jsonl")
    report = base.object_value(json.loads(report_bytes))
    expected = base.object_value(json.loads((base.PROOF / "prior-report.json").read_bytes()))
    if report != expected:
        raise RuntimeError("failed observer report differs")
    if (report["baseline_verified"] is not False or report["observations_complete"] is not False
            or report["source_restored"] is not True or report["publication_eligible"] is not False):
        raise RuntimeError("failed observer outcome was relabeled")
    rows = [base.object_value(json.loads(line)) for line in argv_bytes.splitlines()]
    if len(rows) > 32:
        raise ValueError("failed observer argv record count exceeds its bound")
    for row in rows:
        print("PRIOR_OBSERVER_RAW_ARGV " + json.dumps(row), flush=True)
    nested = [row for row in rows if row["module"] == "module example.com/spira-sqlc-runtime"]
    outer = [row for row in rows if row["module"] == "module github.com/gosuda/spira"
             and row["argv"] in (
                 ["test", "-race", "-count=1", "-v", "-run", "^TestGeneratedQueryConstants$", "./cmd/sqlc-gen-spira"],
                 ["test", "-race", "-count=1", "-v", "-run", "^TestSQLcPostgresLiteralPipeline$", "./cmd/sqlc-gen-spira"])]
    if len(outer) != 2 or nested:
        raise RuntimeError("archived argv does not confirm the missing nested observation")
    (prior / "report.json").write_bytes(report_bytes)
    (prior / "logos-argv.jsonl").write_bytes(argv_bytes)
    result: dict[str, object] = {
        **artifact.receipt, "report_sha256": hashlib.sha256(report_bytes).hexdigest(),
        "argv_sha256": hashlib.sha256(argv_bytes).hexdigest(),
        "raw_argv": rows, "outer_calls": len(outer), "nested_calls": len(nested),
        "prior_outcome_preserved": "failure", "source_restored": True,
        "gate_results_reused_as_new_success": False,
    }
    (prior / "authentication.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print("SPIRA_R40_PRIOR_OBSERVER " + json.dumps(result), flush=True)
    return result
