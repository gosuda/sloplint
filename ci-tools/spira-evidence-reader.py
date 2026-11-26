from __future__ import annotations
import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

ARTIFACTS = (
    (15105527927, "3b5a3e8d4e2631cbabcdef863ebac38e11512af657daa6aa928c0e0d64510057"),
    (15106701561, "b760a39c23ac132bd6c8abfb873888e8f1de6afa8c4c7830d0f4a4a0142c9860"),
)

def main() -> None:
    with tempfile.TemporaryDirectory(prefix="spira-evidence-") as temporary:
        for artifact, digest in ARTIFACTS:
            target = Path(temporary) / f"{artifact}.zip"
            with target.open("wb") as output:
                subprocess.run(["gh", "api", f"/repos/gosuda/sloplint/actions/artifacts/{artifact}/zip"], stdout=output, check=True)
            assert hashlib.sha256(target.read_bytes()).hexdigest() == digest
            with zipfile.ZipFile(target) as archive:
                for name in archive.namelist():
                    assert not Path(name).is_absolute() and ".." not in Path(name).parts
                    if name.endswith("/identity.json"):
                        identity = json.loads(archive.read(name))
                        folder = str(Path(name).parent)
                        for relative, expected in identity["files"].items():
                            assert hashlib.sha256(archive.read(f"{folder}/source/{relative}")).hexdigest() == expected
                        print(json.dumps({"artifact": artifact, "identity": identity}), flush=True)
                    if name.endswith("results.json") or name.endswith("cache-environment.txt"):
                        print(f"ARTIFACT {artifact} FILE {name}", flush=True)
                        print(archive.read(name).decode(), flush=True)
                    if name == "43/stream-read-cancellation-red.log":
                        print("CANCELLATION CONTROL LOG", flush=True)
                        print(archive.read(name).decode(), flush=True)
            print(f"Verified artifact {artifact} SHA256 {digest}", flush=True)

if __name__ == "__main__":
    main()
