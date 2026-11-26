from __future__ import annotations

import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Regression:
    name: str
    path: str
    original: str
    mutant: str
    command: tuple[str, ...]
    failure: str


ROOT = Path(os.environ["GITHUB_WORKSPACE"]) / "spira"
EVIDENCE = Path(os.environ["GITHUB_WORKSPACE"]) / "evidence"
CASES = (
    Regression(
        "unicode-package",
        "cmd/sqlc-gen-spira/names.lgo",
        'func isGoIdent(s string) bool {\n',
        'func isGoIdent(s string) bool {\n\tfor _, r := range s {\n\t\tif r > unicode.MaxASCII {\n\t\t\treturn false\n\t\t}\n\t}\n',
        ("logos", "test", "-race", "-count=1", "-v", "-run", "^TestPackageNameAcceptsUnicode$", "./cmd/sqlc-gen-spira"),
        "--- FAIL: TestPackageNameAcceptsUnicode",
    ),
    Regression(
        "request-stream-default",
        "http3/export_test.lgo",
        '\tif cfg.MaxFieldSectionSize != 0 {\n\t\tconfig.maxFieldSectionSize = cfg.MaxFieldSectionSize\n\t}\n',
        '\tconfig.maxFieldSectionSize = cfg.MaxFieldSectionSize\n',
        ("logos", "test", "-race", "-count=1", "-v", "-run", "^TestH3RequestStreamDispatchesFullDuplex$", "./http3"),
        "--- FAIL: TestH3RequestStreamDispatchesFullDuplex",
    ),
    Regression(
        "dashboard-row-swap",
        "frontend/test/dom-stubs.ts",
        '\tvi.stubGlobal("DOMParser", TemplateDOMParser);\n',
        '',
        ("pnpm", "--dir", "frontend", "exec", "vitest", "run", "test/live.test.ts", "-t", "dashboard row"),
        "AssertionError",
    ),
    Regression(
        "generated-session-sweep-log",
        "cmd/spira/templates/account-store.lgo.tmpl",
        '\t\tslog.ErrorContext(ctx, "account: sweep expired sessions", "err", err)\n',
        '\t\t_ = slog.Default()\n',
        ("logos", "test", "-race", "-count=1", "-v", "-run", "^TestNewGeneratedSQLiteAppRuns$", "./cmd/spira"),
        "omits the failed session sweep",
    ),
)


def run(case: Regression, label: str) -> int:
    log = EVIDENCE / f"{case.name}-{label}.log"
    with log.open("w") as output:
        result = subprocess.run(case.command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, check=False, timeout=1200)
    print(f"{case.name} {label}: exit {result.returncode}", flush=True)
    if result.returncode:
        print(log.read_text(errors="replace")[-7000:], flush=True)
    return result.returncode


def main() -> int:
    results: dict[str, dict[str, int]] = {}
    for case in CASES:
        path = ROOT / case.path
        saved = path.read_text()
        assert saved.count(case.original) == 1, case.name
        try:
            path.write_text(saved.replace(case.original, case.mutant))
            red = run(case, "red")
            assert red != 0, f"{case.name}: mutant unexpectedly passes"
            assert case.failure in (EVIDENCE / f"{case.name}-red.log").read_text(errors="replace"), f"{case.name}: failed outside the intended assertion"
        finally:
            path.write_text(saved)
        green = run(case, "green")
        assert green == 0, f"{case.name}: repaired source failed"
        results[case.name] = {"red": red, "green": green}
        (EVIDENCE / "regressions.json").write_text(json.dumps(results, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
