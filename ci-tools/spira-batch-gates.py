from __future__ import annotations

import hashlib
import json
import os
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
    if label == "30":
        return (
            Regression("serve-read-timeout", "http3/server.lgo", "inner := &web.Server{Router: s.Router, ReadHeaderTimeout: s.ReadHeaderTimeout, ReadTimeout: s.ReadTimeout,", "inner := &web.Server{Router: s.Router, ReadHeaderTimeout: s.ReadHeaderTimeout,", "./http3", "^TestH3ServeClosesSlowBodyWithinReadTimeout$", ("--- FAIL: TestH3ServeClosesSlowBodyWithinReadTimeout",)),
            Regression("missing-settings-code", "@logos/src/logos/net/http/h3_server.go", "h3ServerMissingSettings      = 0x10a", "h3ServerMissingSettings      = 0x100", "./http3", "^TestH3QUICMissingSettingsReportsProtocolCode$", ("want remote H3_MISSING_SETTINGS",)),
        )
    if label == "38":
        return (
            Regression("tls-before-udp", "http3/server.lgo", "func (s *Server) startH3(ctx context.Context) error {\n", "func (s *Server) startH3(ctx context.Context) error {\n\tprobe, err := net.ListenPacket(\"udp\", s.Addr)\n\tif err != nil { return err }\n\tif err := probe.Close(); err != nil { return err }\n", "./http3", "^TestH3NeedsTLSFailsStart$", ("want ErrH3NeedsTLS with held TCP and UDP ports",)),
            Regression("trailer-field-limit", "http3/request_stream.lgo", "maxFieldSectionSize: cfg.maxFieldSectionSize,", "maxFieldSectionSize: 0,", "./http3", "^TestH3RequestStreamOmittedLimitInheritsDefaultForTrailers$", ("read trailers =",)),
        )
    if label == "39":
        return (Regression("client-deadline", "gateway/gateway_test.lgo", "Timeout: 10 * time.Second, Transport:", "Transport:", "./gateway", "^TestGatewayClientDeadlineFailsFast$", ("stalled server answered without an error",)),)
    if label == "41":
        return (Regression("second-idle-timeout", "db/db.lgo", "merged.ConnMaxIdleTime = o.ConnMaxIdleTime", "_ = o.ConnMaxIdleTime", "./db", "^TestOpenHonorsSecondIdleTimeout$", ("second idle timeout was ignored",)),)
    if label == "42":
        return (Regression("live-unidirectional-cap", "@logos/src/logos/net/http/h3_server.go", "h3ServerUniStreams       = 100", "h3ServerUniStreams       = 101", "./http3", "^TestH3PublicCapsIncomingUniStreamsAt100$", ("101st active stream opened past the 100-stream cap",)),)
    return ()


def prove(case: Regression, folder: Path) -> bool:
    path = LOGOS / case.path.removeprefix("@logos/") if case.path.startswith("@logos/") else SOURCE / case.path
    original = path.read_text()
    assert original.count(case.original) == 1, case.label
    command = ["logos", "test", "-race", "-count=1", "-v", "-run", case.test, case.package]
    try:
        path.write_text(original.replace(case.original, case.mutant))
        red = run(command, folder / f"{case.label}-red.log", 120)
        output = (folder / f"{case.label}-red.log").read_text(errors="replace")
        reproduced = red != 0 and all(message in output for message in case.failures)
    finally:
        path.write_text(original)
    green = run(command, folder / f"{case.label}-green.log", 120)
    return reproduced and green == 0


def memory_classification(candidate: Candidate, folder: Path) -> bool:
    path = SOURCE / "db/db.lgo"
    repaired = path.read_bytes()
    command = ["logos", "test", "-race", "-count=1", "-v", "-run", "^(TestPlainFilenameMemoryQueryKeepsExpiry|TestMemoryURIFragmentKeepsRows)$", "./db"]
    try:
        path.write_bytes(subprocess.check_output(["git", "show", f"{candidate.head}:db/db.lgo"], cwd=SOURCE))
        red = run(command, folder / "memory-classification-red.log", 120)
        output = (folder / "memory-classification-red.log").read_text(errors="replace")
        reproduced = red != 0 and all(f"--- FAIL: {name}" in output for name in ("TestPlainFilenameMemoryQueryKeepsExpiry", "TestMemoryURIFragmentKeepsRows"))
    finally:
        path.write_bytes(repaired)
    green = run(command, folder / "memory-classification-green.log", 120)
    return reproduced and green == 0


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
    (folder / "identity.json").write_text(json.dumps({"candidate": asdict(candidate), "files": files}, indent=2) + "\n")
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
        if candidate.label == "41":
            proofs["memory-classification"] = memory_classification(candidate, folder)
        if candidate.label == "40":
            source = (SOURCE / "cmd/sqlc-gen-spira/export_test.lgo").read_text()
            proofs["immutable-forwarder"] = "func MethodOK(arg string) bool" in source and "var MethodOK" not in source
        result["proofs"] = proofs
    git("diff", "--exit-code")
    assert git("write-tree") == candidate.tree
    subprocess.run(["git", "diff", "--exit-code"], cwd=LOGOS, check=True)
    (folder / "results.json").write_text(json.dumps(result, indent=2) + "\n")
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
