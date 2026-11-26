from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class Candidate:
    label: str
    checkout: str
    head: str
    patch_sha256: str
    tree: str


@dataclass(frozen=True)
class Regression:
    label: str
    path: str
    original: str
    mutant: str
    package: str
    test: str
    failures: tuple[str, ...]
    timeout: int = 120


WORKSPACE = Path(os.environ["GITHUB_WORKSPACE"])
SOURCE = WORKSPACE / "spira"
PROBE = WORKSPACE / "probe"
LOGOS = Path(os.environ["LOGOS_ROOT"])
EVIDENCE = WORKSPACE / "evidence"
BASE = "192c1b0b2f8ab195638bcebe0779c6b4f9a39b70"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True).strip()


def run(command: list[str], log: Path, timeout: int = 1800) -> int:
    print(f"Running {command!r}; log={log.relative_to(EVIDENCE)}", flush=True)
    with log.open("w") as output:
        try:
            code = subprocess.run(command, cwd=SOURCE, stdout=output, stderr=subprocess.STDOUT, check=False, timeout=timeout).returncode
        except subprocess.TimeoutExpired:
            output.write(f"\nExceeded {timeout} seconds.\n")
            code = 124
    print(f"Exit {code}: {log.relative_to(EVIDENCE)}", flush=True)
    if code:
        print(log.read_text(errors="replace")[-12000:], flush=True)
    return code


def regressions(label: str) -> tuple[Regression, ...]:
    return (
        Regression("caller-return-with-held-cleanup", "http3/server.lgo", "\tselect {\n\tcase <-shutDone:\n\tcase <-ctx.Done():\n\t\tcerr := r.closeConn()\n\t\treturn errors.Join(ctx.Err(), cerr)\n\t}\n", "\t<-shutDone\n", "./http3", "^TestH3StopReturnsWhileTransportCleanupBlocked$", ("Stop blocked", "past the 300ms caller deadline")),
    )


def prove(case: Regression, folder: Path) -> bool:
    path = LOGOS / case.path.removeprefix("@logos/") if case.path.startswith("@logos/") else SOURCE / case.path
    original = path.read_text()
    assert original.count(case.original) == 1, case.label
    command = ["logos", "test", "-race", "-count=1", "-v", "-run", case.test, case.package]
    try:
        path.write_text(original.replace(case.original, case.mutant))
        red = run(command, folder / f"{case.label}-red.log", case.timeout)
        output = (folder / f"{case.label}-red.log").read_text(errors="replace")
        reproduced = red != 0 and all(message in output for message in case.failures)
    finally:
        path.write_text(original)
    green = run(command, folder / f"{case.label}-green.log", case.timeout)
    green_output = (folder / f"{case.label}-green.log").read_text(errors="replace")
    expected = re.compile(case.test)
    passed = [name for name in re.findall(r"^--- PASS: (\S+)", green_output, re.MULTILINE) if expected.fullmatch(name)]
    skipped = [name for name in re.findall(r"^--- SKIP: (\S+)", green_output, re.MULTILINE) if expected.fullmatch(name)]
    executed = bool(passed) and not skipped
    result = {"command": command, "red": red, "reproduced": reproduced, "green": green, "passed": passed, "skipped": skipped}
    (folder / f"{case.label}-results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"PROOF {case.label}: {json.dumps(result, sort_keys=True)}", flush=True)
    for line in green_output.splitlines():
        if line.startswith("--- PASS:") or line.startswith("--- SKIP:"):
            print(line, flush=True)
    return reproduced and green == 0 and executed


def dashboard_proofs(folder: Path) -> dict[str, bool]:
    command = ["pnpm", "--dir", "frontend", "exec", "vitest", "run", "test/live.test.ts", "-t", "dashboard row"]
    tests = SOURCE / "frontend/test/live.test.ts"
    parser = SOURCE / "frontend/test/dom-stubs.ts"
    saved_tests = tests.read_text()
    saved_parser = parser.read_text()
    spy = 'const swapSpy = vi.spyOn(htmx, "swap");'
    bypass = 'const swapSpy = vi.spyOn(htmx, "swap").mockImplementation(() => {});'
    assert saved_tests.count(spy) == 2
    try:
        tests.write_text(saved_tests.replace(spy, bypass))
        red = run(command, folder / "dashboard-no-swap-red.log", 120)
        reproduced = red != 0 and "AssertionError" in (folder / "dashboard-no-swap-red.log").read_text()
        old_tests = subprocess.check_output(["git", "show", "HEAD:frontend/test/live.test.ts"], cwd=SOURCE, text=True)
        assert old_tests.count(spy) == 2
        tests.write_text(old_tests.replace(spy, bypass))
        old_oracle = run(command, folder / "dashboard-old-oracle.log", 120)
    finally:
        tests.write_text(saved_tests)
    green = run(command, folder / "dashboard-no-swap-green.log", 120)
    binding = '\tvi.stubGlobal("DOMParser", TemplateDOMParser);\n'
    assert saved_parser.count(binding) == 1
    try:
        parser.write_text(saved_parser.replace(binding, ""))
        parser_red = run(command, folder / "dashboard-parser-red.log", 120)
        parser_reproduced = parser_red != 0 and "AssertionError" in (folder / "dashboard-parser-red.log").read_text()
    finally:
        parser.write_text(saved_parser)
    parser_green = run(command, folder / "dashboard-parser-green.log", 120)
    result = {"command": command, "red": red, "old_oracle": old_oracle, "green": green, "parser_red": parser_red, "parser_green": parser_green}
    (folder / "dashboard-proof-results.json").write_text(json.dumps(result, indent=2) + "\n")
    return {"actual-row-oracle": reproduced and old_oracle == 0 and green == 0, "fragment-parser": parser_reproduced and parser_green == 0}


def shutdown_error_composition_proof(folder: Path) -> bool:
    server = SOURCE / "http3/server.lgo"
    tests = SOURCE / "http3/http3_shutdown_cleanup_test.lgo"
    saved_server = server.read_text()
    saved_tests = tests.read_text()
    old_tests = subprocess.check_output(["git", "show", "HEAD:http3/http3_shutdown_cleanup_test.lgo"], cwd=SOURCE, text=True)
    selection = "\t\treturn errors.Join(ctx.Err(), cerr)\n"
    replacement = "\t\treturn errors.Join(ctx.Err(), cerr, ErrH3Shutdown.Wrap(ctx.Err()))\n"
    assert saved_server.count(selection) == 1
    command = ["logos", "test", "-race", "-count=1", "-v", "-run", "^TestH3StopReturnsWhileTransportCleanupBlocked$", "./http3"]
    try:
        server.write_text(saved_server.replace(selection, replacement))
        tests.write_text(old_tests)
        old_green = run(command, folder / "shutdown-error-old-oracle.log", 120)
        old_output = (folder / "shutdown-error-old-oracle.log").read_text(errors="replace")
        tests.write_text(saved_tests)
        red = run(command, folder / "shutdown-error-composition-red.log", 120)
        red_output = (folder / "shutdown-error-composition-red.log").read_text(errors="replace")
        reproduced = red != 0 and "caller cancellation included unfinished shutdown fault" in red_output
    finally:
        server.write_text(saved_server)
        tests.write_text(saved_tests)
    green = run(command, folder / "shutdown-error-composition-green.log", 120)
    green_output = (folder / "shutdown-error-composition-green.log").read_text(errors="replace")
    marker = "--- PASS: TestH3StopReturnsWhileTransportCleanupBlocked "
    skipped = "--- SKIP: TestH3StopReturnsWhileTransportCleanupBlocked "
    executed = marker in old_output and marker in green_output and skipped not in old_output and skipped not in green_output
    result = {"old_oracle": old_green, "red": red, "reproduced": reproduced, "green": green, "explicit_passes": executed}
    (folder / "shutdown-error-composition-results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"SHUTDOWN_ERROR_PROOF {json.dumps(result, sort_keys=True)}", flush=True)
    for line in green_output.splitlines():
        if line.startswith("--- PASS:") or line.startswith("--- SKIP:"):
            print(line, flush=True)
    return old_green == 0 and reproduced and green == 0 and executed


def check(candidate: Candidate) -> dict[str, object]:
    folder = EVIDENCE / candidate.label
    folder.mkdir(parents=True)
    git("fetch", "--no-tags", "--depth=1", "origin", candidate.checkout)
    git("checkout", "--detach", candidate.checkout)
    patch = PROBE / "ci-tools" / f"spira-{candidate.label}-r8.patch"
    assert hashlib.sha256(patch.read_bytes()).hexdigest() == candidate.patch_sha256
    git("apply", "--index", "--whitespace=error", str(patch))
    assert git("write-tree") == candidate.tree
    git("diff", "--cached", "--check")
    files: dict[str, str] = {}
    for relative in git("diff", "--cached", "--name-only").splitlines():
        source = SOURCE / relative
        target = folder / "source" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        files[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
    identity = {"candidate": asdict(candidate), "files": files}
    (folder / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print(f"IDENTITY {json.dumps(identity, sort_keys=True)}", flush=True)
    result: dict[str, object] = {"candidate": asdict(candidate)}
    result["gate"] = run(["just", "gate"], folder / "gate.log")
    result["eject"] = run(["just", "eject"], folder / "eject.log")
    if result["eject"]:
        exported = Path("/tmp/spira-eject/ai")
        if exported.is_dir():
            shutil.copytree(exported, folder / "export-ai")
    if candidate.label == "combined":
        result["regressions"] = run(["uv", "run", "--no-project", "--python", "3.13", str(PROBE / "ci-tools/spira-regressions.py")], folder / "regressions.log")
    else:
        proofs: dict[str, bool] = {}
        for case in regressions(candidate.label):
            proofs[case.label] = prove(case, folder)
        if candidate.label == "40":
            source = (SOURCE / "cmd/sqlc-gen-spira/export_test.lgo").read_text()
            proofs["immutable-forwarder"] = "func MethodOK(arg string) bool" in source and "var MethodOK" not in source
        if candidate.label == "dashboard":
            proofs.update(dashboard_proofs(folder))
        proofs["shutdown-error-composition"] = shutdown_error_composition_proof(folder)
        result["proofs"] = proofs
    git("diff", "--exit-code")
    assert git("write-tree") == candidate.tree
    subprocess.run(["git", "diff", "--exit-code"], cwd=LOGOS, check=True)
    (folder / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"CANDIDATE_RESULT {json.dumps(result, sort_keys=True)}", flush=True)
    git("restore", "--source=HEAD", "--staged", "--worktree", "--", ".")
    return result


def main() -> int:
    manifest = json.loads((PROBE / "ci-tools/spira-candidates.json").read_text())
    candidates = tuple(Candidate(**record) for record in manifest)
    results: dict[str, dict[str, object]] = {}
    for candidate in candidates:
        results[candidate.label] = check(candidate)
        (EVIDENCE / "batch-results.json").write_text(json.dumps(results, indent=2) + "\n")
    passed = all(result["gate"] == 0 and result["eject"] == 0 for result in results.values())
    passed = passed and all(label != "combined" or result.get("regressions") == 0 for label, result in results.items())
    passed = passed and all(all(result.get("proofs", {}).values()) for result in results.values())
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
