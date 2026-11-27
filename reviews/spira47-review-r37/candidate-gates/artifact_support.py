"""Read immutable same-repository artifacts without forwarding credentials."""
from __future__ import annotations

import hashlib
import http.client
import json
import urllib.parse
import urllib.request
from dataclasses import dataclass


@dataclass(frozen=True)
class Artifact:
    data: bytes
    receipt: dict[str, object]


def object_value(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or any(not isinstance(key, str) for key in value):
        raise ValueError("expected JSON object")
    return value


def text(value: object) -> str:
    if not isinstance(value, str):
        raise ValueError("expected text")
    return value


def integer(value: object) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError("expected integer")
    return value


def request(path: str, token: str, limit: int = 1_048_576) -> tuple[int, dict[str, str], bytes]:
    connection = http.client.HTTPSConnection("api.github.com", timeout=60)
    try:
        connection.request("GET", path, headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "spira-frozen-source-verification",
        })
        response = connection.getresponse()
        body = response.read(limit + 1)
        if len(body) > limit:
            raise RuntimeError("GitHub metadata or archive exceeds its fixed bound")
        return response.status, dict(response.getheaders()), body
    finally:
        connection.close()


def metadata(path: str, token: str) -> dict[str, object]:
    status, _, body = request(path, token)
    if status != 200:
        raise RuntimeError(f"GitHub metadata returned status {status}")
    return object_value(json.loads(body))


def download(specification: dict[str, object], token: str) -> Artifact:
    base = "/repos/gosuda/sloplint"
    run_id = integer(specification["run"])
    job_id = integer(specification["job"])
    artifact_id = integer(specification["artifact"])
    size = integer(specification["zip_bytes"])
    if size <= 0 or size > 32 * 1024 * 1024:
        raise ValueError("artifact ZIP size is outside the proof bound")
    producer = text(specification["producer"])
    conclusion = text(specification["conclusion"])
    if conclusion not in ("success", "failure"):
        raise ValueError("unsupported frozen producer conclusion")
    expected_digest = text(specification["zip_sha256"])
    run = metadata(f"{base}/actions/runs/{run_id}", token)
    job = metadata(f"{base}/actions/jobs/{job_id}", token)
    artifact = metadata(f"{base}/actions/artifacts/{artifact_id}", token)
    if (run["id"] != run_id or run["head_sha"] != producer
            or run["status"] != "completed" or run["conclusion"] != conclusion):
        raise RuntimeError("artifact producer run differs from the frozen terminal run")
    if (job["id"] != job_id or job["run_id"] != run_id
            or job["status"] != "completed" or job["conclusion"] != conclusion):
        raise RuntimeError("artifact producer job differs")
    workflow = object_value(artifact["workflow_run"])
    if (artifact["id"] != artifact_id or artifact["expired"]
            or artifact["name"] != specification["name"]
            or artifact["size_in_bytes"] != size
            or artifact["digest"] != "sha256:" + expected_digest
            or workflow["id"] != run_id or workflow["head_sha"] != producer):
        raise RuntimeError("artifact metadata differs")
    status, headers, data = request(f"{base}/actions/artifacts/{artifact_id}/zip", token, size)
    if status == 302:
        location = next((v for k, v in headers.items() if k.lower() == "location"), "")
        parsed = urllib.parse.urlsplit(location)
        if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
            raise RuntimeError("artifact storage redirect is not HTTPS")
        with urllib.request.urlopen(location, timeout=120) as response:
            data = response.read(size + 1)
    elif status != 200:
        raise RuntimeError(f"artifact download returned status {status}")
    if len(data) != size or hashlib.sha256(data).hexdigest() != expected_digest:
        raise RuntimeError("artifact ZIP bytes differ")
    return Artifact(data, {
        "producer": producer, "run": run_id, "job": job_id, "artifact": artifact_id,
        "name": specification["name"], "zip_bytes": size, "zip_sha256": expected_digest,
        "producer_conclusion": conclusion, "download_digest_verified": True,
    })
