"""Observe the actual nested generated-package compiler invocation."""
from __future__ import annotations

import json
import os
import shutil
import sys
from pathlib import Path

import baseline_support as base


def install_argv_observer() -> tuple[Path, Path, str]:
    found = shutil.which("logos")
    if found is None:
        raise RuntimeError("real logos compiler is missing")
    real = Path(found).resolve(strict=True)
    observer_dir = base.BUILD / "argv-observer"
    observer_dir.mkdir(exist_ok=False)
    wrapper = observer_dir / "logos"
    wrapper.write_text("#!" + sys.executable + "\n"
                       + (base.PROOF / "capture_logos.py").read_text(encoding="utf-8"),
                       encoding="utf-8")
    wrapper.chmod(0o755)
    launcher = observer_dir / "launch-test"
    launcher.write_text("#!" + sys.executable + "\n"
                        + (base.PROOF / "launch_test.py").read_text(encoding="utf-8"),
                        encoding="utf-8")
    launcher.chmod(0o755)
    old_path = os.environ["PATH"]
    os.environ["SPIRA_ARGV_OBSERVER_DIR"] = str(observer_dir)
    os.environ["SPIRA_TEST_PROCESS_ARGV"] = str(base.OUTPUT / "test-process-argv.jsonl")
    os.environ["SPIRA_REAL_LOGOS"] = str(real)
    os.environ["SPIRA_LOGOS_ARGV"] = str(base.OUTPUT / "logos-argv.jsonl")
    os.environ["PATH"] = str(observer_dir) + os.pathsep + old_path
    return real, launcher, old_path


def nested_race_records() -> list[dict[str, object]]:
    rows = [base.object_value(json.loads(line))
            for line in (base.OUTPUT / "logos-argv.jsonl").read_bytes().splitlines()]
    for row in rows:
        print("CURRENT_OBSERVER_RAW_ARGV " + json.dumps(row), flush=True)
    selected = [row for row in rows if row["module"] == "module example.com/spira-sqlc-runtime"
                and isinstance(row["argv"], list) and row["argv"][:1] == ["test"]]
    if len(selected) != 3 or len({base.text(row["cwd"]) for row in selected}) != 3:
        raise RuntimeError("did not observe the three distinct nested generated-package test processes")
    if any(row["argv"] != ["test", "-race", "-count=1", "-v", "."] for row in selected):
        raise RuntimeError("candidate nested test arguments differ")
    launchers = [base.object_value(json.loads(line))
                 for line in (base.OUTPUT / "test-process-argv.jsonl").read_bytes().splitlines()]
    if len(launchers) != 1:
        raise RuntimeError("did not observe the actual combined runtime test binary")
    for row in launchers:
        print("OUTER_TEST_PROCESS_ARGV " + json.dumps(row), flush=True)
        if row["incoming_first_path"] != str(Path(os.environ["GOROOT"]) / "bin"):
            raise RuntimeError("native test PATH did not match the pinned runner prepend")
    for row in selected:
        print("NESTED_QUERY_TEST_ARGV " + json.dumps(row), flush=True)
    return selected


