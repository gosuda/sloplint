from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from zipfile import ZipFile

WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "candidate"
PROBE = WORKSPACE / "probe"
EVIDENCE = WORKSPACE / "evidence"
EVIDENCE.mkdir(exist_ok=True)
sys.stdout.reconfigure(encoding="utf-8")
sys.stderr.reconfigure(encoding="utf-8")


def git(*arguments: str) -> str:
    return subprocess.check_output(["git", *arguments], cwd=SOURCE, text=True).strip()


def run(command: list[str], label: str, timeout: int = 1800) -> int:
    path = EVIDENCE / f"{label}.log"
    print(f"RUN {label}: {command!r}", flush=True)
    with path.open("w", encoding="utf-8") as output:
        try:
            code = subprocess.run(command, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, timeout=timeout, check=False).returncode
        except subprocess.TimeoutExpired:
            output.write(f"\nExceeded {timeout} seconds.\n")
            code = 124
    print(f"EXIT {label}: {code}", flush=True)
    content = path.read_text(encoding="utf-8", errors="replace")
    if code:
        print(content[-16000:], flush=True)
    else:
        for line in content.splitlines():
            if "PASS: native console input" in line:
                print(line, flush=True)
    return code


def mutation(label: str, original: str, replacement: str, diagnostic: str) -> dict[str, object]:
    path = SOURCE / "lib/os/os_platform.win32.ml"
    saved = path.read_text(encoding="utf-8")
    assert saved.count(original) == 1, label
    command = ["opam", "exec", "--switch=rd", "--", "dune", "build", "--profile", "release", "@test/melt/windows-console", "--force"]
    try:
        path.write_text(saved.replace(original, replacement), encoding="utf-8", newline="\n")
        red = run(command, f"{label}-red", 180)
        text = (EVIDENCE / f"{label}-red.log").read_text(encoding="utf-8", errors="replace")
        reproduced = red != 0 and diagnostic in " ".join(text.split())
    finally:
        path.write_text(saved, encoding="utf-8", newline="\n")
    green = run(command, f"{label}-green", 180)
    text = (EVIDENCE / f"{label}-green.log").read_text(encoding="utf-8", errors="replace")
    passed = green == 0 and "PASS: native console input" in text
    result: dict[str, object] = {"red": red, "reproduced": reproduced, "green": green, "native_pass": passed}
    print(f"PROOF {label}: {json.dumps(result, sort_keys=True)}", flush=True)
    return result


def main() -> int:
    manifest = json.loads((PROBE / "ci-tools/charamel-windows-candidate.json").read_text())
    assert git("rev-parse", "HEAD") == manifest["head"]
    baseline: dict[str, object] = {"head": manifest["head"], "tree": git("write-tree")}
    if os.name == "nt":
        archive_path = EVIDENCE / "previous-windows.zip"
        expected_digest = "cb1a860e06d1e736ed4e0ad62a6d2714dae9ed0a1579f3ec4cbdd901ef1983af"
        assert hashlib.sha256(archive_path.read_bytes()).hexdigest() == expected_digest
        with ZipFile(archive_path) as archive:
            previous = json.loads(archive.read("results.json"))
            baseline = json.loads(archive.read("baseline.json"))
            assert previous["candidate"]["head"] == manifest["head"]
            assert previous["candidate"]["tree"] == "2e9241f9cd1cbfb3e131adaa58d693dad16152ca"
            assert baseline["head"] == manifest["head"] and baseline["tree"] == git("write-tree")
            assert baseline["tests"] == 1
            (EVIDENCE / "baseline-tests.log").write_bytes(archive.read("baseline-tests.log"))
        baseline["artifact"] = 15110611238
        baseline["artifact_sha256"] = expected_digest
        baseline["run"] = 45800790388
        print(f"BASELINE_RESULT {json.dumps(baseline, sort_keys=True)}", flush=True)
        (EVIDENCE / "baseline.json").write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8", newline="\n")
    patch = PROBE / "ci-tools/charamel-windows-candidate.patch"
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == manifest["tree"]
    git("diff", "--cached", "--check")
    files: dict[str, str] = {}
    for relative in git("diff", "--cached", "--name-only").splitlines():
        path = SOURCE / relative
        target = EVIDENCE / "source" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
        files[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    identity = {"candidate": manifest, "platform": os.environ["RUNNER_OS"], "files": files}
    (EVIDENCE / "identity.json").write_text(json.dumps(identity, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"IDENTITY {json.dumps(identity, sort_keys=True)}", flush=True)
    result: dict[str, object] = {"candidate": manifest, "platform": os.environ["RUNNER_OS"], "baseline": baseline}
    for name, command in (
        ("fmt", ["dune", "build", "@fmt"]),
        ("build", ["dune", "build", "--profile", "release"]),
        ("tests", ["dune", "runtest", "--profile", "release", "--force"]),
    ):
        result[name] = run(["opam", "exec", "--switch=rd", "--", *command], name)
    proofs: dict[str, dict[str, object]] = {}
    if os.name == "nt":
        console = run(["opam", "exec", "--switch=rd", "--", "dune", "build", "--profile", "release", "@test/melt/windows-console", "--force"], "native-console", 180)
        console_log = (EVIDENCE / "native-console.log").read_text(encoding="utf-8", errors="replace")
        result["native_console"] = {"exit": console, "native_pass": console == 0 and "PASS: native console input" in console_log}
        proofs["stdout-independent"] = mutation(
            "stdout-independent",
            "  let echo_off () =\n    let input_mode = mode input in",
            "  let echo_off () =\n    ignore (saved_state ());\n    let input_mode = mode input in",
            "restore exited before hidden input prompt",
        )
        proofs["input-mode-restored"] = mutation(
            "input-mode-restored",
            "| Echo { input; input_mode } ->\n        ignore (Charamel_os_win32.set_console_mode input input_mode)",
            "| Echo { input; input_mode } ->\n        ignore (input, input_mode)",
            "input console mode was not restored",
        )
    if os.name == "nt":
        def failures(name: str) -> dict[str, list[str]]:
            content = (EVIDENCE / name).read_text(encoding="utf-8", errors="replace")
            clean = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", content)
            cases = sorted(set(" ".join(line.split("[FAIL]", 1)[1].strip().rstrip(chr(0x2502)).split()) for line in clean.splitlines() if "[FAIL]" in line))
            suites = sorted(set(re.findall(r'File "(test/[^"]+/dune)"', clean)))
            return {"cases": cases, "suites": suites}
        baseline_failures = failures("baseline-tests.log")
        candidate_failures = failures("tests.log")
        result["baseline_comparison"] = {
            "baseline": baseline_failures,
            "candidate": candidate_failures,
            "additional_cases": sorted(set(candidate_failures["cases"]) - set(baseline_failures["cases"])),
            "additional_suites": sorted(set(candidate_failures["suites"]) - set(baseline_failures["suites"])),
        }
    result["proofs"] = proofs
    git("diff", "--exit-code")
    assert git("write-tree") == manifest["tree"]
    (EVIDENCE / "results.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"CANDIDATE_RESULT {json.dumps(result, sort_keys=True)}", flush=True)
    passed = all(result[name] == 0 for name in ("fmt", "build", "tests"))
    passed = passed and all(proof["reproduced"] and proof["native_pass"] for proof in proofs.values())
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
