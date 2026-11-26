from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = ROOT / "candidate"
PROOF = ROOT / "probe"
EVIDENCE = ROOT / "evidence"
EVIDENCE.mkdir(exist_ok=True)
sys_environment = dict(os.environ)


def git(*arguments: str) -> str:
    return subprocess.check_output(["git", *arguments], cwd=SOURCE, text=True).strip()


def run(label: str, command: list[str], environment: dict[str, str] | None = None) -> tuple[int, str]:
    print(f"RUN {label}: {json.dumps(command)}", flush=True)
    output = EVIDENCE / f"{label}.log"
    with output.open("w", encoding="utf-8") as stream:
        try:
            status = subprocess.run(
                command, cwd=SOURCE, env=environment, stdout=stream,
                stderr=subprocess.STDOUT, timeout=1200, check=False,
            ).returncode
        except subprocess.TimeoutExpired:
            stream.write("\nTimed out after 1200 seconds.\n")
            status = 124
    text = output.read_text(encoding="utf-8", errors="replace")
    print(f"EXIT {label}: {status}", flush=True)
    print(text[-12000:] if status else text[-3000:], flush=True)
    return status, text


def launcher(text: str) -> list[str]:
    found = re.search(
        r"\(run\s+powershell\.exe(?P<args>.*?)\s+-File\s+%\{dep:windows_console\.ps1\}",
        text, re.DOTALL,
    )
    assert found, "committed Dune launcher missing"
    return ["powershell.exe", *found.group("args").split()]


def policies(environment: dict[str, str]) -> list[dict[str, str]]:
    command = (
        "Get-ExecutionPolicy -List | ForEach-Object { "
        "[PSCustomObject]@{Scope=[string]$_.Scope; Policy=[string]$_.ExecutionPolicy} "
        "} | ConvertTo-Json -Compress"
    )
    raw = subprocess.check_output(
        ["powershell.exe", "-NoProfile", "-Command", command],
        cwd=SOURCE, env=environment, text=True, encoding="utf-8", errors="replace",
    )
    result = json.loads(raw)
    assert isinstance(result, list) and len(result) == 5
    return result


def windows_policy() -> dict[str, object]:
    original_environment = dict(os.environ)
    restricted = {**original_environment, "PSExecutionPolicyPreference": "Restricted"}
    before = policies(original_environment)
    forced_before = policies(restricted)
    assert next(row["Policy"] for row in forced_before if row["Scope"] == "Process") == "Restricted"
    sentinel = EVIDENCE / "policy-sentinel.ps1"
    sentinel.write_text(
        "param([string]$Melt)\n"
        "$ErrorActionPreference = 'Stop'\n"
        "$policies = @(Get-ExecutionPolicy -List | ForEach-Object { "
        "[PSCustomObject]@{Scope=[string]$_.Scope; Policy=[string]$_.ExecutionPolicy} })\n"
        "Write-Output ('SENTINEL_POLICY ' + ($policies | ConvertTo-Json -Compress))\n"
        "Write-Output ('LAUNCH_SENTINEL:' + $Melt)\n",
        encoding="utf-8", newline="\n",
    )
    before_source = subprocess.check_output(
        ["git", "show", "HEAD:test/melt/dune"], cwd=SOURCE, text=True,
    )
    old_command = launcher(before_source)
    new_command = launcher((SOURCE / "test/melt/dune").read_text())
    assert old_command == ["powershell.exe", "-NoProfile"]
    assert new_command == ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass"]
    marker = "01963e9d35d5d0712d52fab49bc108660c1ee192623e37164df26da773f653e4"
    suffix = ["-File", str(sentinel), "-Melt", marker]
    red, red_output = run("restricted-original-launcher", old_command + suffix, restricted)
    green, green_output = run("restricted-candidate-launcher", new_command + suffix, restricted)
    reproduced = (
        red != 0 and "LAUNCH_SENTINEL:" not in red_output
        and ("PSSecurityException" in red_output or "scripts is disabled" in red_output)
    )
    executed = green == 0 and f"LAUNCH_SENTINEL:{marker}" in green_output
    emitted = next(
        line.removeprefix("SENTINEL_POLICY ")
        for line in green_output.splitlines() if line.startswith("SENTINEL_POLICY ")
    )
    child_policies = json.loads(emitted)
    assert next(row["Policy"] for row in child_policies if row["Scope"] == "Process") == "Bypass"
    assert [row for row in child_policies if row["Scope"] != "Process"] == [
        row for row in forced_before if row["Scope"] != "Process"
    ]
    after = policies(original_environment)
    forced_after = policies(restricted)
    assert before == after and forced_before == forced_after
    assert dict(os.environ) == original_environment
    result = {
        "red": red, "green": green, "reproduced": reproduced, "sentinel_executed": executed,
        "original_policy": before, "restricted_policy": forced_before,
        "child_policy": child_policies, "policy_unchanged": True,
        "scope": "PowerShell launcher only; unchanged real-console proof is run 45808277559",
    }
    assert reproduced and executed
    return result


def main() -> int:
    manifest = json.loads((PROOF / "ci-tools/charamel-windows-candidate.json").read_text())
    assert git("rev-parse", "HEAD") == manifest["head"]
    patch = PROOF / "ci-tools/charamel-windows-candidate.patch"
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == manifest["patch_sha256"]
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == manifest["tree"]
    assert git("diff", "--cached", "--name-only") == "test/melt/dune"
    assert hashlib.sha256((SOURCE / "test/melt/dune").read_bytes()).hexdigest() == manifest["file_sha256"]
    assert hashlib.sha256((SOURCE / "test/melt/windows_console.ps1").read_bytes()).hexdigest() == manifest["console_script_sha256"]
    identity = {**manifest, "platform": os.environ["RUNNER_OS"]}
    print("LAUNCH_IDENTITY " + json.dumps(identity, sort_keys=True), flush=True)
    (EVIDENCE / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    result: dict[str, object] = {"identity": identity}
    if os.name == "nt":
        result["policy"] = windows_policy()
    else:
        assert subprocess.check_output(["ocamlc", "-version"], text=True).strip() == "5.5.1"
        assert subprocess.check_output(["ocamlformat", "--version"], text=True).strip() == "0.29.0"
        gates = {}
        for label, command in [
            ("fmt", ["dune", "build", "@fmt"]),
            ("build", ["dune", "build", "--profile", "release"]),
            ("tests", ["dune", "runtest", "--profile", "release"]),
        ]:
            gates[label], _ = run(label, command)
        result["gates"] = gates
    git("diff", "--exit-code")
    git("diff", "--cached", "--check")
    assert git("write-tree") == manifest["tree"]
    (EVIDENCE / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    print("LAUNCH_RESULT " + json.dumps(result, sort_keys=True), flush=True)
    return int(any(code != 0 for code in result.get("gates", {}).values()))


if __name__ == "__main__":
    raise SystemExit(main())
