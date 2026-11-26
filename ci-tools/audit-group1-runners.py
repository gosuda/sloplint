from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

import yaml


ROOT = Path(__file__).parent
MATRIX_RUNNER = re.compile(r"\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}")
CONDITIONAL_JSON = re.compile(
    r"\$\{\{\s*fromJSON\(\(github\.event_name\s*==\s*'schedule'\s*\|\|\s*"
    r"github\.event_name\s*==\s*'workflow_dispatch'\)\s*&&\s*'([^']*)'\s*"
    r"\|\|\s*'([^']*)'\)\s*\}\}",
    re.IGNORECASE,
)


def alternatives(value: object) -> list[object]:
    if not isinstance(value, str):
        return [value]
    match = CONDITIONAL_JSON.fullmatch(value)
    if match is None:
        raise ValueError(f"unresolved matrix expression: {value}")
    return [json.loads(branch) for branch in match.groups()]


def runner_values(runner: object, matrix: object) -> list[str]:
    if not isinstance(runner, str):
        raise ValueError(f"runner is not a label: {runner}")
    if "${{" not in runner:
        return [runner]
    match = MATRIX_RUNNER.fullmatch(runner)
    if match is None:
        raise ValueError(f"unresolved runner expression: {runner}")
    key = match.group(1)
    labels: set[str] = set()
    for branch in alternatives(matrix):
        if not isinstance(branch, dict):
            raise ValueError(f"matrix is not a mapping: {branch}")
        if key in branch:
            for axis in alternatives(branch[key]):
                if not isinstance(axis, list) or not all(isinstance(label, str) for label in axis):
                    raise ValueError(f"invalid runner axis: {axis}")
                labels.update(axis)
        includes = branch.get("include", [])
        if not isinstance(includes, list):
            raise ValueError(f"unresolved includes: {includes}")
        for row in includes:
            if not isinstance(row, dict):
                raise ValueError(f"invalid matrix row: {row}")
            if key in row:
                label = row[key]
                if not isinstance(label, str) or "${{" in label:
                    raise ValueError(f"unresolved included runner: {label}")
                labels.add(label)
            elif key not in branch:
                raise ValueError(f"matrix row has no runner: {row}")
    if not labels:
        raise ValueError("runner axis has no possible values")
    return sorted(labels)


def allowed(label: str) -> bool:
    if label.startswith("depot-macos-"):
        return label == "depot-macos-latest"
    return label.startswith(("depot-ubuntu-", "depot-windows-")) and "26.04" not in label


def main() -> int:
    source = json.loads((ROOT / "group1_round7_runner_sources.json").read_text())
    blobs = {(item["headRepo"], item["blob"]): item for item in source["sources"]}
    paths = {(item["headRepo"], item["head"], item["path"]) for item in source["files"]}
    jobs: list[dict[str, object]] = []
    failures: list[dict[str, object]] = []
    for file in source["files"]:
        content = blobs[(file["headRepo"], file["blob"])]["content"]
        raw = content.encode()
        digest = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
        if digest != file["blob"]:
            failures.append({**file, "error": f"blob identity mismatch: {digest}"})
            continue
        document = yaml.load(content, Loader=yaml.BaseLoader)
        for name, job in document.get("jobs", {}).items():
            item = {"repo": file["headRepo"], "pr": file["n"], "head": file["head"], "path": file["path"], "job": name}
            try:
                if "runs-on" in job:
                    values = runner_values(job["runs-on"], job.get("strategy", {}).get("matrix"))
                    item["labels"] = values
                    item["dynamic"] = "${{" in job["runs-on"]
                    if not all(allowed(label) for label in values):
                        raise ValueError(f"noncompliant runner: {values}")
                elif "uses" in job:
                    target = job["uses"]
                    if not isinstance(target, str) or not target.startswith("./"):
                        raise ValueError(f"external or unresolved reusable workflow: {target}")
                    if (file["headRepo"], file["head"], target[2:]) not in paths:
                        raise ValueError(f"missing reusable workflow: {target}")
                    item["reusable"] = target
                else:
                    raise ValueError("job has neither runs-on nor reusable workflow")
                jobs.append(item)
            except ValueError as error:
                failures.append({**item, "error": str(error)})
    report = {"head_count": len(source["heads"]), "default_heads": sum(head.get("scope") == "default" for head in source["heads"]), "pull_request_heads": sum(head.get("scope") == "pull_request" for head in source["heads"]), "empty_repositories": source.get("emptyRepositories", []), "workflow_instances": len(source["files"]), "unique_blobs": len(blobs), "jobs": jobs, "failures": failures}
    (ROOT / "group1_round7_runner_values.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"heads": report["head_count"], "default_heads": report["default_heads"], "pull_request_heads": report["pull_request_heads"], "empty_repositories": report["empty_repositories"], "workflow_instances": report["workflow_instances"], "unique_blobs": report["unique_blobs"], "job_instances": len(jobs), "dynamic_job_instances": sum(bool(job.get("dynamic")) for job in jobs), "reusable_job_instances": sum("reusable" in job for job in jobs), "labels": sorted({label for job in jobs for label in job.get("labels", [])}), "failures": failures}, indent=2))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
