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
    if label == "30":
        return (
            Regression("serve-read-timeout", "http3/server.lgo", "inner := &web.Server{Router: s.Router, ReadHeaderTimeout: s.ReadHeaderTimeout, ReadTimeout: s.ReadTimeout,", "inner := &web.Server{Router: s.Router, ReadHeaderTimeout: s.ReadHeaderTimeout,", "./http3", "^TestH3ServeTimesOutSlowBodyRead$", ("handler body read did not time out",)),
            Regression("missing-settings-deadline", "@logos/src/logos/net/http/h3_server.go", "h3ServerSettingsTimeout  = 5 * time.Second", "h3ServerSettingsTimeout  = 8 * time.Second", "./http3", "^TestH3QUICMissingSettingsReportsProtocolCode$", ("missing SETTINGS exceeded five seconds plus one second observation tolerance",)),
        )
    if label == "38":
        return (
            Regression("tls-before-udp", "http3/server.lgo", "func (s *Server) startH3(ctx context.Context) error {\n", "func (s *Server) startH3(ctx context.Context) error {\n\tprobe, err := net.ListenPacket(\"udp\", s.Addr)\n\tif err != nil { return err }\n\tif err := probe.Close(); err != nil { return err }\n", "./http3", "^TestH3NeedsTLSFailsStart$", ("want ErrH3NeedsTLS with held TCP and UDP ports",)),
            Regression("trailer-field-limit", "http3/request_stream.lgo", "maxFieldSectionSize: cfg.maxFieldSectionSize,", "maxFieldSectionSize: 0,", "./http3", "^TestH3RequestStreamOmittedLimitInheritsDefaultForTrailers$", ("read trailers =",)),
        )
    if label == "39":
        return (Regression("client-deadline", "gateway/gateway_test.lgo", "Timeout: 10 * time.Second, Transport:", "Transport:", "./gateway", "^TestGatewayClientDeadlineFailsFast$", ("stalled server answered without an error",)),)
    if label == "41":
        return (Regression("memory-open-cap", "db/db.lgo", "handle.SetMaxOpenConns(opts.MaxOpenConns)", "handle.SetMaxOpenConns(opts.MaxIdleConns)", "./db", "^TestMemoryPoolCapsIdleAtOpenLimit$", ("idle limit overrode the smaller open-connection cap",)),)
    if label == "42":
        return (Regression("live-unidirectional-cap", "@logos/src/logos/net/http/h3_server.go", "h3ServerUniStreams       = 100", "h3ServerUniStreams       = 101", "./http3", "^TestH3PublicCapsIncomingUniStreamsAt100$", ("101st active stream opened past the 100-stream cap",)),)
    if label == "43":
        return (
            Regression("stream-read-cancellation", "websocket/conn.lgo", "\t\tdefer c.joinStreamCancellation(ctx, &err)()\n\t}\n\tif err := c.readDeadline(ctx); err != nil {", "\t}\n\tif err := c.readDeadline(ctx); err != nil {", "./websocket", "^TestStreamCancelUnblocksPendingNext$", ("Next did not return after cancel",)),
            Regression("generated-sweep-cause", "cmd/spira/templates/account-store.lgo.tmpl", 'slog.ErrorContext(ctx, "account: sweep expired sessions", "err", err)', 'slog.ErrorContext(ctx, "account: sweep expired sessions")', "./cmd/spira", "^TestNewGeneratedSQLiteAppRuns$", ("omits the planted sweep cause",), 1200),
        )
    if label == "44":
        return (
            Regression("one-delayed-retry", "http3/server.lgo", "func bindExactRetry(ctx context.Context, addr string) (net.Listener, net.PacketConn, error) {\n\tl, conn, err := bindExact(addr)\n\tif err == nil || !errors.Is(err, ErrListenUDP) || !errors.Is(err, syscall.EADDRINUSE) {\n\t\treturn l, conn, err\n\t}\n\ttimer := time.NewTimer(10 * time.Millisecond)\n\tdefer timer.Stop()\n\tfor attempt := 1; attempt < 8; attempt++ {\n\t\tselect {\n\t\tcase <-ctx.Done():\n\t\t\treturn nil, nil, ctx.Err()\n\t\tcase <-timer.C:\n\t\t}\n\t\tl, conn, err = bindExact(addr)\n\t\tif err == nil || !errors.Is(err, ErrListenUDP) || !errors.Is(err, syscall.EADDRINUSE) {\n\t\t\treturn l, conn, err\n\t\t}\n\t\tif attempt < 7 {\n\t\t\ttimer.Reset(10 * time.Millisecond)\n\t\t}\n\t}\n\treturn nil, nil, err\n}\n", "func bindExactRetry(ctx context.Context, addr string) (net.Listener, net.PacketConn, error) {\n\tl, conn, err := bindExact(addr)\n\tif err == nil || !errors.Is(err, ErrListenUDP) || !errors.Is(err, syscall.EADDRINUSE) {\n\t\treturn l, conn, err\n\t}\n\ttimer := time.NewTimer(70 * time.Millisecond)\n\tdefer timer.Stop()\n\tfor attempt := 1; attempt < 2; attempt++ {\n\t\tselect {\n\t\tcase <-ctx.Done():\n\t\t\treturn nil, nil, ctx.Err()\n\t\tcase <-timer.C:\n\t\t}\n\t\tl, conn, err = bindExact(addr)\n\t\tif err == nil || !errors.Is(err, ErrListenUDP) || !errors.Is(err, syscall.EADDRINUSE) {\n\t\t\treturn l, conn, err\n\t\t}\n\t\tif attempt < 7 {\n\t\t\ttimer.Reset(10 * time.Millisecond)\n\t\t}\n\t}\n\treturn nil, nil, err\n}\n", "./http3", "^(TestH3StartRetriesAtEveryInterval|TestH3StartPersistentUDPConflictExhaustsRetries)$", ("retry 1 reached TCP conflict after 70ms, want 10ms", "--- PASS: TestH3StartPersistentUDPConflictExhaustsRetries")),
            Regression("missing-fourth-retry", "http3/server.lgo", "\t\tl, conn, err = bindExact(addr)\n", "\t\tif attempt != 4 {\n\t\t\tl, conn, err = bindExact(addr)\n\t\t}\n", "./http3", "^TestH3StartRetriesAtEveryInterval$", ("retry 4 reached TCP conflict after 50ms, want 40ms",)),
        )
    return ()


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
