from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import zipfile
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
EVIDENCE = WORKSPACE / "evidence"


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    windows = os.name == "nt"
    platform = "win64" if windows else "linux-x86_64"
    url = f"https://github.com/protocolbuffers/protobuf/releases/download/v30.2/protoc-30.2-{platform}.zip"
    archive = EVIDENCE / "protoc.zip"
    subprocess.run(["curl", "--fail", "--location", "--retry", "3", "--max-time", "120", "--output", str(archive), url], check=True, timeout=180)
    directory = EVIDENCE / "protoc-bin"
    directory.mkdir()
    executable = directory / ("protoc.exe" if windows else "protoc")
    with zipfile.ZipFile(archive) as zipped:
        executable.write_bytes(zipped.read("bin/" + executable.name))
    if not windows:
        executable.chmod(0o755)
    os.environ["PATH"] = str(directory) + os.pathsep + os.environ["PATH"]
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    plugins = EVIDENCE / "plugins outside PATH"
    plugins.mkdir()
    os.environ["GOBIN"] = str(plugins)
    assert str(plugins) not in os.environ["PATH"].split(os.pathsep)
    for plugin in ("protoc-gen-go", "protoc-gen-go-grpc"):
        assert shutil.which(plugin) is None, f"fixture already has {plugin} on PATH"
    version = subprocess.check_output([str(executable), "--version"], text=True).strip()
    assert version == "libprotoc 30.2", version
    command = ["cmd", "/d", "/c", str(SOURCE / "scripts/update_proto.bat")] if windows else ["bash", "scripts/update_proto.sh"]
    result = subprocess.run(command, cwd=SOURCE, check=False, timeout=600)
    installed: dict[str, str] = {}
    for plugin in ("protoc-gen-go", "protoc-gen-go-grpc"):
        path = plugins / (plugin + ".exe" if windows else plugin)
        assert path.is_file(), f"generator did not install {plugin} in the isolated GOBIN"
        installed[plugin] = subprocess.check_output([str(path), "--version"], text=True).strip()
    identity = {"url": url, "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(), "version": version, "plugins": installed, "gobin_was_outside_path": True, "exit": result.returncode}
    (EVIDENCE / "generator-identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print("GENERATOR_IDENTITY " + json.dumps(identity, sort_keys=True), flush=True)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
