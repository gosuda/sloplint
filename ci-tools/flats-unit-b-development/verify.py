#!/usr/bin/env python3
"""Execute immutable Flats Unit B development gates and retain exact outcomes."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace")
HERE = Path(__file__).resolve().parent
MANIFEST = json.loads((HERE / "manifest.json").read_text(encoding="utf-8"))
SOURCE = Path(sys.argv[2]).resolve()
EVIDENCE = Path(os.environ["PROOF_EVIDENCE"]).resolve()
EVIDENCE.mkdir(parents=True, exist_ok=True)

def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()

def write(name, value):
    path = EVIDENCE / name
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)

def git(*args):
    return subprocess.check_output(["git", *args], cwd=SOURCE, text=True, encoding="utf-8").strip()

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def identity():
    assert git("rev-parse", "HEAD") == MANIFEST["base"]
    assert git("write-tree") == MANIFEST["tree"]
    subprocess.run(["git", "diff", "--exit-code"], cwd=SOURCE, check=True)
    actual = git("diff", "--cached", "--name-only").splitlines()
    expected = sorted(x["path"] for x in MANIFEST["files"])
    assert sorted(actual) == expected, (actual, expected)
    files = {}
    for item in MANIFEST["files"]:
        observed = digest(SOURCE / item["path"])
        assert observed == item["sha256"], (item["path"], observed)
        files[item["path"]] = observed
    untracked = git("ls-files", "--others", "--exclude-standard")
    assert not untracked, untracked
    return {"head": git("rev-parse", "HEAD"), "index_tree": git("write-tree"), "files": files,
            "worktree_matches_candidate_index": True, "unexpected_untracked": untracked}

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

def prepare():
    assert digest(__file__) == MANIFEST["helper_sha256"]
    assert digest(HERE / "plans.json") == MANIFEST["plans_sha256"]
    assert digest(HERE.parent.parent / ".github/workflows/flats-unit-b-development.yml") == MANIFEST["probe_workflow_sha256"]
    assert git("rev-parse", "HEAD") == MANIFEST["base"]
    assert git("write-tree") == MANIFEST["base_tree"]
    assert not git("status", "--porcelain")
    patch = HERE / "candidate.patch"
    assert digest(patch) == MANIFEST["patch_sha256"]
    subprocess.run(["git", "fetch", "--no-tags", "--depth=2", "origin", MANIFEST["candidate"]],
                   cwd=SOURCE, check=True)
    commit = git("cat-file", "-p", MANIFEST["candidate"])
    parents = re.findall(r"^parent ([a-f0-9]{40})$", commit, re.M)
    assert parents == [MANIFEST["base"]], parents
    assert re.search(r"^tree " + MANIFEST["tree"] + r"$", commit, re.M)
    subprocess.run(["git", "apply", "--index", "--check", str(patch)], cwd=SOURCE, check=True)
    subprocess.run(["git", "apply", "--index", str(patch)], cwd=SOURCE, check=True)
    write("prepared.json", {"utc": now(), "manifest": MANIFEST, "identity": identity(),
                           "resources": resources(),
                           "runner": {key: os.environ.get(key) for key in
                                      ("RUNNER_NAME", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_ENVIRONMENT",
                                       "PROOF_RUNNER", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB",
                                       "GITHUB_SHA", "GITHUB_REF")}})
    print("Prepared exact candidate", MANIFEST["candidate"], MANIFEST["tree"], flush=True)

def environment():
    env = os.environ.copy()
    env.update(GOENV="off", GOCACHEPROG="", PYTHONDONTWRITEBYTECODE="1")
    for key, leaf in (("GOCACHE", "go-build"), ("GOMODCACHE", "go-modules")):
        path = EVIDENCE / "cold-cache" / leaf
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    env["PROOF_ROOT"] = str(HERE)
    return env

def test_events(path):
    events = []
    with path.open(encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if not line.startswith("{"):
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if event.get("Action") in ("run", "skip", "pass", "fail") and event.get("Package"):
                events.append({key: event[key] for key in ("Time", "Action", "Package", "Test", "Elapsed") if key in event})
    return events

def execute(item, env):
    name = item["name"]
    assert re.fullmatch(r"[a-zA-Z0-9_-]+", name), name
    started = now()
    before = time.monotonic()
    state = {"name": name, "status": "running", "started_at": started, "command": item["command"],
             "shell": item.get("shell", "bash"), "cwd": item.get("cwd", "."),
             "resources_start": resources(), "environment_overrides": item.get("env", {})}
    write(name + ".json", state)
    child_env = env.copy()
    for key, value in item.get("env", {}).items():
        child_env[key] = os.environ["RUNNER_TEMP"] if value == "@RUNNER_TEMP@" else value
    if item.get("shell", "bash") == "pwsh":
        prefix = "$env:GOENV='off'; $env:GOCACHEPROG=''; if ((go env GOCACHEPROG) -ne '') { throw 'GOCACHEPROG was injected' }; "
        args = ["pwsh", "-NoLogo", "-NoProfile", "-Command", prefix + item["command"]]
    else:
        prefix = "export GOENV=off GOCACHEPROG=''; cache_prog=$(go env GOCACHEPROG); test -z \"$cache_prog\"\n"
        args = ["bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", prefix + item["command"]]
    log = EVIDENCE / (name + ".log")
    print("START", name, started, flush=True)
    try:
        with log.open("wb") as handle:
            process = subprocess.run(args, cwd=SOURCE / item.get("cwd", "."), env=child_env,
                                     stdout=handle, stderr=subprocess.STDOUT)
        code = process.returncode
        state.update(status="completed", exit_code=code, finished_at=now(),
                     elapsed_seconds=time.monotonic() - before, resources_end=resources(),
                     log_sha256=digest(log))
        events = test_events(log)
        write(name + "-test-events.json", events)
        state["test_events"] = {"file": name + "-test-events.json", "total": len(events),
                                "run": sum(e["Action"] == "run" for e in events),
                                "skip": sum(e["Action"] == "skip" for e in events),
                                "fail": sum(e["Action"] == "fail" for e in events)}
    except Exception as exc:
        code = 125
        state.update(status="harness-error", exit_code=code, finished_at=now(),
                     elapsed_seconds=time.monotonic() - before, error=repr(exc))
    write(name + ".json", state)
    print("END", name, "exit", code, "seconds", round(state["elapsed_seconds"], 3), flush=True)
    if code and log.exists():
        print(log.read_text(encoding="utf-8", errors="replace").splitlines()[-120:], flush=True)
    return state

def plan(group):
    ci = (SOURCE / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    if group == "race":
        section = re.search(r"^  race:\n(.*?)(?=^  [a-z])", ci, re.M | re.S).group(1)
        lanes = re.search(r"for lane in (.*?); do", section).group(1).split()
        assert len(lanes) == 12 and len(set(lanes)) == 12, lanes
        return [{"name": "race-" + lane, "command": "scripts/ci-race.sh " + shlex.quote(lane) + " -json -count=1"} for lane in lanes]
    if group.startswith("test-"):
        lanes = dict(re.findall(r"          - lane: ([a-z0-9-]+)\n            shards: ([a-z0-9 -]+)", ci))
        shards = lanes[group[5:]].split()
        result = []
        if group == "test-other":
            result.extend(json.loads((HERE / "plans.json").read_text())["test-other-prefix"])
        result.extend({"name": "test-" + shard, "command": "scripts/ci-test.sh " + shlex.quote(shard) + " -json -count=1"} for shard in shards)
        return result
    if group == "memory":
        return [{"name": "memory-" + value, "command": "scripts/ci-memory.sh " + value + " -json -count=1"}
                for value in ("reader", "markdown", "source")]
    if group.startswith("large-"):
        lane = "notebook-large-" + group[6:]
        workflow = (SOURCE / ".github/workflows/notebook-memory-race.yml").read_text()
        assert lane in workflow
        return [{"name": lane, "command": "scripts/ci-race.sh " + lane + " -json -count=1"}]
    return json.loads((HERE / "plans.json").read_text(encoding="utf-8"))[group]

def run_group(group):
    identity()
    env = environment()
    cache = {}
    for key in ("GOCACHE", "GOMODCACHE"):
        path = Path(env[key])
        cache[key] = {"path": str(path), "empty_at_start": not any(path.iterdir())}
        assert cache[key]["empty_at_start"], cache
    write("cache-state.json", cache)
    probe = subprocess.run(["go", "env", "-json", "GOCACHEPROG", "GOENV", "GOCACHE", "GOMODCACHE", "GOVERSION", "GOOS", "GOARCH"],
                           cwd=SOURCE, env=env, capture_output=True, text=True)
    assert probe.returncode == 0, probe.stderr
    go_env = json.loads(probe.stdout)
    assert go_env["GOCACHEPROG"] == "" and go_env["GOENV"] == "off", go_env
    write("go-environment.json", go_env)
    items = plan(group)
    write("executed-plan.json", items)
    results = []
    failed = False
    start = time.monotonic()
    for item in items:
        dependencies = item.get("depends_on", [])
        if any(not any(r["name"] == dep and r.get("exit_code") == 0 for r in results) for dep in dependencies):
            result = {"name": item["name"], "status": "skipped-failed-prerequisite", "dependencies": dependencies}
            write(item["name"] + ".json", result)
            failed = True
        else:
            result = execute(item, env)
            failed |= result["exit_code"] != 0
        results.append(result)
        write("summary.json", {"group": group, "status": "running", "results": results,
                               "elapsed_seconds": time.monotonic() - start,
                               "sum_command_seconds": sum(r.get("elapsed_seconds", 0) for r in results)})
    source_error = None
    try:
        restored = identity()
    except Exception as exc:
        failed = True
        restored = None
        source_error = repr(exc)
    write("summary.json", {"group": group, "status": "completed", "exit_code": int(failed),
                           "results": results, "elapsed_seconds": time.monotonic() - start,
                           "sum_command_seconds": sum(r.get("elapsed_seconds", 0) for r in results),
                           "source_identity_after": restored, "source_error": source_error})
    raise SystemExit(int(failed))

if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "prepare":
        prepare()
    elif mode == "run":
        run_group(sys.argv[3])
    elif mode == "finalize":
        write("final-source-identity.json", {"utc": now(), "identity": identity()})
    else:
        raise SystemExit("unknown command")
