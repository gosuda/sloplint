from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "probe" / "ci-tools" / "bamti-alias-views"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "bamti-class-scope-evidence"
EVIDENCE.mkdir(exist_ok=True)
RELATION = SOURCE / "crates/bamts-compiler/src/checker/relations.rs"
TEST = SOURCE / "crates/bamts-verification/tests/corpus_differential.rs"
HEAD = "11e99ee0788613ff6a9fd7d5b0afddf819953850"
EXITS: dict[str, int] = {}

def digest(source: bytes) -> str:
    return hashlib.sha256(source).hexdigest()

def run(name: str, args: list[str], expected: int = 0, required: str = "", limit: int = 600) -> str:
    started = time.monotonic()
    print(f"RUN {name}: {args!r}", flush=True)
    process = subprocess.Popen(args, cwd=SOURCE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
    timed_out = False
    try:
        output, _ = process.communicate(timeout=limit)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        output, _ = process.communicate()
    (EVIDENCE / f"{name}.log").write_text(output)
    EXITS[name] = process.returncode
    (EVIDENCE / "results.json").write_text(json.dumps({"head": HEAD, "exits": EXITS}, indent=2, sort_keys=True) + "\n")
    print(f"EXIT {name}: {process.returncode} timed_out={timed_out} elapsed={time.monotonic() - started:.3f}\n{output[-30000:]}", flush=True)
    assert not timed_out, f"{name}: exceeded {limit} seconds"
    assert process.returncode == expected, f"{name}: expected {expected}, got {process.returncode}"
    assert not required or required in output, f"{name}: missing {required!r}"
    return output

assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=SOURCE, text=True).strip() == HEAD
original = RELATION.read_bytes()
original_test = TEST.read_bytes()
assert digest(original) == "2065d40eabc613ffd30d094122dae6a84cf218edde33b559f3e8a2d7a02015a5"
rejected = (PROOF / "rejected-formatted-relations.rs").read_bytes()
scoped = (PROOF / "class-scoped-relations.rs").read_bytes()
assert digest(rejected) == "99de904efa506dd0f0991f314d3a25c301378b67c2f174e504ea3a8898fe4b55"
assert digest(scoped) == "ba9b76bd93450b8ae28045c57f92d8127843fe0d189f229322f1de5ce2f98c13"
assert json.loads((SOURCE / "node_modules/typescript/package.json").read_text())["version"] == "7.0.2"
test = (PROOF / "valita-test.rs").read_bytes()
assert digest(test) == "23ebdeb51ff67a5ae35e45ca4fa2183965dd17961b089dcd9566572900b93cbb"
args = ["cargo", "test", "--locked", "-p", "bamts-verification", "--test", "corpus_differential", "isolated_valita_alias_probe", "--", "--exact", "--nocapture"]
try:
    TEST.write_bytes(original_test + test)
    for label, source, expected in [("published", original, 0), ("rejected-all-views", rejected, 101), ("class-scoped", scoped, 0)]:
        RELATION.write_bytes(source)
        print("BAMTI_CLASS_SCOPE_IDENTITY " + json.dumps({"label": label, "head": HEAD, "relations_sha256": digest(source), "temporary_test_sha256": digest(TEST.read_bytes())}, sort_keys=True), flush=True)
        required = "stage=check: diagnostic=BAMTS-C053" if expected else "1 passed; 0 failed"
        run(label, args, expected, required, limit=600)
    run("class-scoped-relations", ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib", "checker::relations::tests::", "--", "--nocapture"], required="79 passed; 0 failed", limit=180)
finally:
    RELATION.write_bytes(original)
    TEST.write_bytes(original_test)
    run("restore-source", ["git", "diff", "--exit-code"])
print("BAMTI_CLASS_SCOPE_COMPLETE " + json.dumps(EXITS, sort_keys=True), flush=True)
