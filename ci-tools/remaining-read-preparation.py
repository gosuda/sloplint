from __future__ import annotations

import hashlib
import io
import json
import os
import subprocess
import zipfile
from pathlib import Path

EVIDENCE = Path(os.environ["GITHUB_WORKSPACE"]) / "evidence"


def emit(label: str, value: str) -> None:
    chunks = [value[index:index + 1000] for index in range(0, len(value), 1000)]
    for index, chunk in enumerate(chunks):
        print("PAYLOAD_CHUNK " + json.dumps({"label": label, "index": index, "total": len(chunks), "data": chunk}), flush=True)


def main() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    records = (
        ("oracle", 15109137822, "1c2f29d9626c31e8016503186ea397dbe1944dc8269d6b8bfd9e37384da2cd87"),
        ("ssh", 15109491806, "0b0bc86a5ab65550251e4487d8705b435df2e24dec07f2bf86e32555389ac3ac"),
    )
    for label, artifact_id, expected_digest in records:
        archive = subprocess.check_output(["gh", "api", f"/repos/gosuda/sloplint/actions/artifacts/{artifact_id}/zip"], timeout=120)
        actual_digest = hashlib.sha256(archive).hexdigest()
        assert actual_digest == expected_digest, (label, actual_digest)
        with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
            payload = zipped.read(f"{label}.json").decode("utf-8")
        (EVIDENCE / f"{label}.json").write_text(payload, encoding="utf-8")
        print("ARTIFACT_IDENTITY " + json.dumps({"label": label, "id": artifact_id, "sha256": actual_digest}), flush=True)
        emit(label, payload)


if __name__ == "__main__":
    main()
