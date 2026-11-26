from __future__ import annotations
import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

ARTIFACTS = (
    (15102168536, "c932f3d1dc7f0af56a183c3caacededfe5a7a89cbda751c8d896449c42dbc90f"),
    (15103895195, "f23f98cbea99c5135378c9a1da1f199fcc69d7be83685aadb37b3d997fe8b304"),
    (15101880147, "256718ca3059b080a19dcdee22912b3c58bf90db827db7ed2e1dfba3de41f444"),
    (15105951749, "f2872671248c3719c9680248fc92c2e46bad38326a4b970861c50420b258d7d3"),
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
