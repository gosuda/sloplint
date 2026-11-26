from __future__ import annotations

import hashlib
import json
import os
import subprocess
import zipfile
from pathlib import Path

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "source"
PROBE = WORKSPACE / "probe"
EVIDENCE = WORKSPACE / "evidence"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def command(args: list[str], name: str, timeout: int = 600) -> None:
    with (EVIDENCE / f"{name}.log").open("w", encoding="utf-8") as output:
        result = subprocess.run(args, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, check=False, timeout=timeout)
    print(f"PREP_RESULT {name}: {result.returncode}", flush=True)
    if result.returncode:
        print((EVIDENCE / f"{name}.log").read_text(encoding="utf-8", errors="replace")[-10000:], flush=True)
        raise RuntimeError(f"{name} failed")


def main() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((PROBE / "ci-tools/remaining-ssh-review-draft.json").read_text())
    assert git("rev-parse", "HEAD") == manifest["head"]
    patch = PROBE / "ci-tools" / manifest["patch"]
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    git("apply", "--index", str(patch))
    assert git("write-tree") == manifest["tree"]
    (EVIDENCE / "draft-identity.json").write_text(json.dumps(manifest, indent=2) + "\n")
    os.environ["GOENV"] = "off"
    os.environ["GOCACHEPROG"] = ""
    assert subprocess.check_output(["go", "env", "GOCACHEPROG"], cwd=SOURCE, text=True).strip() == ""
    archive = EVIDENCE / "protoc.zip"
    command(["curl", "--fail", "--location", "--retry", "3", "--max-time", "120", "--output", str(archive), "https://github.com/protocolbuffers/protobuf/releases/download/v30.2/protoc-30.2-linux-x86_64.zip"], "protoc-download")
    protoc_dir = EVIDENCE / "protoc-bin"
    protoc_dir.mkdir()
    protoc = protoc_dir / "protoc"
    with zipfile.ZipFile(archive) as zipped:
        protoc.write_bytes(zipped.read("bin/protoc"))
    protoc.chmod(0o755)
    os.environ["PATH"] = str(protoc_dir) + os.pathsep + os.environ["PATH"]
    plugins = EVIDENCE / "plugins outside PATH"
    plugins.mkdir()
    os.environ["GOBIN"] = str(plugins)
    assert str(plugins) not in os.environ["PATH"].split(os.pathsep)
    version = subprocess.check_output([str(protoc), "--version"], text=True).strip()
    assert version == "libprotoc 30.2", version
    (EVIDENCE / "protoc-identity.json").write_text(json.dumps({"url": "https://github.com/protocolbuffers/protobuf/releases/download/v30.2/protoc-30.2-linux-x86_64.zip", "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(), "version": version}, indent=2) + "\n")
    command(["bash", "scripts/update_proto.sh"], "generator")
    assert (plugins / "protoc-gen-go").is_file()
    assert (plugins / "protoc-gen-go-grpc").is_file()
    command(["gofmt", "-w", "client.go", "middleware.go", "middleware_test.go"], "gofmt", 60)
    git("add", "--update")
    assert git("ls-files", "--others", "--exclude-standard") == ""
    git("diff", "--cached", "--check")
    changed = git("diff", "--cached", "--name-only").splitlines()
    allowed = {"client.go", "middleware.go", "middleware_test.go", "proto/middleware.proto", "proto/middleware.pb.go", "proto/middleware_grpc.pb.go", "scripts/update_proto.sh", "scripts/update_proto.bat"}
    assert set(changed) <= allowed, changed
    files = {path: (SOURCE / path).read_text() for path in changed}
    value = {"head": manifest["head"], "base": manifest["base_sha"], "draft_tree": manifest["tree"], "tree": git("write-tree"), "files": files, "sha256": {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in changed}}
    payload = json.dumps(value, ensure_ascii=True)
    (EVIDENCE / "ssh-review-prepared.json").write_text(payload + "\n")
    (EVIDENCE / "ssh-review-prepared.patch").write_text(subprocess.check_output(["git", "diff", "--cached", "--binary"], cwd=SOURCE, text=True))
    chunks = [payload[index:index + 1000] for index in range(0, len(payload), 1000)]
    for index, chunk in enumerate(chunks):
        print("PAYLOAD_CHUNK " + json.dumps({"label": "ssh-review-prepared", "index": index, "total": len(chunks), "data": chunk}), flush=True)


if __name__ == "__main__":
    main()
