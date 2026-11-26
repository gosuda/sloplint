#!/usr/bin/env python3
"""Run only the witnessed Sheet failures on the immutable pre-change source."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace")
HERE = Path(__file__).resolve().parent
MANIFEST = json.loads((HERE / "manifest.json").read_text(encoding="utf-8"))
SOURCE = Path(sys.argv[1]).resolve()
LANE = sys.argv[2]
assert LANE in ("linux", "windows")
OUTPUT = Path(os.environ["RUNNER_TEMP"]) / ("flats-sheet-baseline-" + LANE)
OUTPUT.mkdir(parents=True, exist_ok=True)
ENV = os.environ.copy()
ENV.update(GOENV="off", GOCACHEPROG="", PYTHONDONTWRITEBYTECODE="1")
for key, name in (("GOCACHE", "go-build"), ("GOMODCACHE", "go-modules")):
    cache = Path(os.environ["RUNNER_TEMP"]) / ("flats-sheet-baseline-cache-" + LANE) / name
    cache.mkdir(parents=True, exist_ok=True)
    ENV[key] = str(cache)
if LANE == "windows":
    ENV["TMP"] = ENV["TEMP"] = os.environ["RUNNER_TEMP"]
    ENV.pop("FLATS_SHEET_FULL", None)
else:
    ENV["FLATS_SHEET_FULL"] = "1"

def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()

def save(name, value):
    (OUTPUT / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")

def run_output(args):
    return subprocess.check_output(args, cwd=SOURCE, env=ENV, text=True, encoding="utf-8").strip()

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def identity():
    assert run_output(["git", "rev-parse", "HEAD"]) == MANIFEST["source_head"]
    assert run_output(["git", "rev-parse", "HEAD^{tree}"]) == MANIFEST["source_tree"]
    assert run_output(["git", "write-tree"]) == MANIFEST["source_tree"]
    assert not run_output(["git", "status", "--porcelain", "--untracked-files=all"])
    files = {}
    for item in MANIFEST["files"]:
        assert run_output(["git", "hash-object", "--no-filters", item["path"]]) == item["blob"], item["path"]
        files[item["path"]] = {"blob": item["blob"], "sha256": digest(SOURCE / item["path"])}
    return {"head": MANIFEST["source_head"], "tree": MANIFEST["source_tree"], "files": files}

def resources():
    result = {"utc": now(), "platform": platform.platform(), "cpu_count": os.cpu_count()}
    if hasattr(os, "getloadavg"):
        result["load_average"] = os.getloadavg()
    proc = Path("/proc/meminfo")
    if proc.exists():
        result["memory"] = proc.read_text()
    elif sys.platform == "darwin":
        result["memory_bytes"] = subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True).strip()
    elif os.name == "nt":
        import ctypes
        class Memory(ctypes.Structure):
            _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong),
                        ("total_phys", ctypes.c_ulonglong), ("avail_phys", ctypes.c_ulonglong),
                        ("total_page", ctypes.c_ulonglong), ("avail_page", ctypes.c_ulonglong),
                        ("total_virtual", ctypes.c_ulonglong), ("avail_virtual", ctypes.c_ulonglong),
                        ("avail_extended", ctypes.c_ulonglong)]
        state = Memory()
        state.length = ctypes.sizeof(state)
        if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(state)):
            result["memory_bytes"] = state.total_phys
            result["memory_available_bytes"] = state.avail_phys
    return result


def environment_snapshot():
    commands = {
        "go-version": ["go", "version"],
        "go-environment": ["go", "env", "GOENV", "GOCACHEPROG", "GOCACHE", "GOMODCACHE", "GOMAXPROCS", "GOFLAGS"],
    }
    if os.name == "nt":
        commands["windows-clock"] = ["pwsh", "-NoLogo", "-NoProfile", "-Command",
            "Get-Date -Format o; (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o'); Get-Process | Sort-Object CPU -Descending | Select-Object -First 12 ProcessName,Id,CPU,WorkingSet64 | ConvertTo-Json"]
    else:
        commands["load"] = ["uptime"]
        commands["processes"] = ["ps", "-eo", "pid,comm,pcpu,pmem", "--sort=-pcpu"]
    result = {}
    for name, args in commands.items():
        p = subprocess.run(args, cwd=SOURCE, env=ENV, capture_output=True, text=True, encoding="utf-8", errors="replace")
        result[name] = {"command": args, "exit_code": p.returncode, "stdout": p.stdout, "stderr": p.stderr}
    flush_path = OUTPUT / "owned-flush-sample.tmp"
    started = time.monotonic()
    with flush_path.open("wb") as handle:
        handle.write(b"baseline flush sample\n")
        handle.flush()
        os.fsync(handle.fileno())
    result["owned_flushed_write_seconds"] = time.monotonic() - started
    flush_path.unlink()
    return result

before = identity()
machine = resources()
assert machine["cpu_count"] == MANIFEST["expected_cpu_count"], machine
assert (LANE == "windows") == (os.name == "nt")
assert run_output(["go", "env", "GOCACHEPROG"]) == ""
assert ENV["GOENV"] == "off"
tests = MANIFEST["tests"][LANE]
pattern = "^(" + "|".join(tests) + ")$"
if LANE == "linux":
    command = ["bash", "scripts/ci-test.sh", "sheet", "-json", "-v", "-count=1", "-run", pattern]
else:
    command = ["go", "test", "-json", "-v", "-count=1", "-timeout=30m", "-run", pattern, "./internal/sheet/..."]
state = {"started_at": now(), "lane": LANE, "source": before, "manifest": MANIFEST,
         "command": command, "environment": {k: ENV.get(k) for k in ("GOENV", "GOCACHEPROG", "GOCACHE", "GOMODCACHE", "GOFLAGS", "GOMAXPROCS", "TMP", "TEMP", "TMPDIR", "GOTMPDIR", "FLATS_SHEET_FULL")},
         "resources_start": machine, "diagnostics_start": environment_snapshot(),
         "runner": {k: os.environ.get(k) for k in ("RUNNER_NAME", "RUNNER_OS", "RUNNER_ARCH", "PROOF_RUNNER", "GITHUB_RUN_ID", "GITHUB_JOB", "GITHUB_SHA", "GITHUB_REF")},
         "proof_files": {p.name: digest(p) for p in HERE.iterdir() if p.is_file()}}
save("prepared.json", state)
print("BASELINE_START " + json.dumps({k: state[k] for k in ("started_at", "lane", "command", "resources_start", "runner")}), flush=True)
log = OUTPUT / "tests.jsonl"
started = time.monotonic()
with log.open("wb") as handle:
    completed = subprocess.run(command, cwd=SOURCE, env=ENV, stdout=handle, stderr=subprocess.STDOUT)
state.update(exit_code=completed.returncode, elapsed_seconds=time.monotonic() - started,
             finished_at=now(), resources_end=resources(), diagnostics_end=environment_snapshot(),
             log_sha256=digest(log))
events = []
outputs = {}
for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        continue
    if not isinstance(event, dict) or "Action" not in event:
        continue
    if event["Action"] == "output":
        outputs.setdefault(event.get("Test", ""), []).append(event.get("Output", ""))
    else:
        events.append(event)
ran = {e.get("Test") for e in events if e.get("Action") == "run"}
state["missing_requested_tests"] = sorted(set(tests) - ran)
state["events"] = events
state["source_after"] = identity()
save("results.json", state)
print("BASELINE_RESULT " + json.dumps({k: state[k] for k in ("lane", "exit_code", "elapsed_seconds", "missing_requested_tests", "resources_end", "source_after")}), flush=True)
for test, lines in outputs.items():
    if test:
        print("TEST_OUTPUT " + json.dumps({"test": test, "output": "".join(lines)}), flush=True)
for event in events:
    if event.get("Action") in ("fail", "pass", "skip"):
        print("TEST_RESULT " + json.dumps(event), flush=True)
if state["missing_requested_tests"]:
    raise SystemExit(2)
raise SystemExit(completed.returncode)
