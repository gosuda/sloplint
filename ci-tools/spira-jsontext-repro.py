from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path


ROOT = Path(os.environ["RUNNER_TEMP"]) / "spira-jsontext-repro"
EVIDENCE = Path(os.environ["GITHUB_WORKSPACE"]) / "evidence" / "jsontext-repro"


def run(command: list[str], cwd: Path, log: Path) -> int:
    with log.open("w") as stream:
        result = subprocess.run(command, cwd=cwd, stdout=stream, stderr=subprocess.STDOUT, check=False, timeout=300)
    return result.returncode


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    results: dict[str, dict[str, int]] = {}
    for label, alias in (("implicit", ""), ("explicit", "jsontext ")):
        source = ROOT / label
        source.mkdir(parents=True, exist_ok=True)
        evidence = EVIDENCE / label
        evidence.mkdir()
        (source / "go.mod").write_text(f"module example.com/{label}\n\ngo 1.27.0\n")
        code = f'''package sample

import (
    {alias}"encoding/json/jsontext"
    "encoding/json/v2"
)

func Encode(value jsontext.Value) ([]byte, error) {{ return json.Marshal(value) }}
'''
        (source / "sample.lgo").write_text(code)
        (evidence / "original.lgo").write_text(code)
        result = {"logos_build": run(["logos", "build", "./..."], source, evidence / "logos-build.log")}
        exported = ROOT / f"{label}-export"
        result["eject"] = run(["logos", "eject", "-o", str(exported)], source, evidence / "eject.log")
        if exported.is_dir():
            for path in exported.iterdir():
                if path.is_file() and path.suffix in {".go", ".lgo", ".mod"}:
                    shutil.copyfile(path, evidence / path.name)
            (evidence / "export-files.txt").write_text("\n".join(str(p.relative_to(exported)) for p in exported.rglob("*") if p.is_file()) + "\n")
            result["upstream_build"] = run(["env", "-u", "GOROOT", "-u", "GOTOOLCHAIN", os.environ["UPSTREAM_GO"], "build", "./..."], exported, evidence / "upstream-build.log")
        results[label] = result
        print(json.dumps({label: result}), flush=True)
    (EVIDENCE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
