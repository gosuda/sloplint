from __future__ import annotations
import hashlib
import json
from pathlib import Path
import zipfile

for label, digest in [("x64", "1b76ed8a2e9f24e483400eebfa2f545e3821fff3987e477429e0090fa8fc5d14"), ("arm", "b3649089ed23611acea3bdd690ae9dc3463df991386fd5e44ac782f0d0225060")]:
    archive = Path(f"{label}.zip")
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == digest
    with zipfile.ZipFile(archive) as data:
        names = [name for name in data.namelist() if name.endswith("workspace.log")]
        assert len(names) == 1, names
        lines = data.read(names[0]).decode().splitlines()
    selected: set[int] = set()
    for index, line in enumerate(lines):
        if "FAILED" in line or "panicked at" in line or line.startswith("failures:") or "error:" in line or "test result: FAILED" in line:
            selected.update(range(max(0, index - 2), min(len(lines), index + 25)))
    print(f"ARTIFACT_FAILURES {label} {digest}")
    for index in sorted(selected):
        print(f"{index + 1}: {lines[index]}")
