from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess

SOURCE = Path.cwd()
PROOF = SOURCE.parent / "probe" / "ci-tools" / "bamti-alias-order"
EVIDENCE = Path(os.environ["RUNNER_TEMP"]) / "bamti-alias-order-evidence"
EVIDENCE.mkdir(exist_ok=True)
RELATION = SOURCE / "crates/bamts-compiler/src/checker/relations.rs"
EXITS: dict[str, int] = {}

def run(name: str, args: list[str], expected: int | None = 0, required: str = "", limit: int = 600) -> str:
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
    (EVIDENCE / "results.json").write_text(json.dumps(EXITS, indent=2, sort_keys=True) + "\n")
    print(f"EXIT {name}: {process.returncode} timed_out={timed_out}\n{output[-20000:]}", flush=True)
    assert not timed_out, f"{name}: timed out after {limit}s, actual exit {process.returncode}"
    if expected is None:
        assert process.returncode in (0, 1), f"{name}: unexpected oracle failure"
    else:
        assert process.returncode == expected, f"{name}: expected {expected}, got {process.returncode}"
    assert not required or required in output, f"{name}: missing {required!r}"
    return output

head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=SOURCE, text=True).strip()
assert head == "87be57b3bdee04bdc9606d0fadde7912b20a52a6"
original = RELATION.read_text()
assert hashlib.sha256(original.encode()).hexdigest() == "3479aaa39ad13441b543153d340ad3a12b12e5b6c0303f11fe62911d194834d9"
assert json.loads((SOURCE / "node_modules/typescript/package.json").read_text())["version"] == "7.0.2"
oracle = EVIDENCE / "oracle"
oracle.mkdir(exist_ok=True)
preamble = "type A<T> = { next: A<T[]>; value: T };\ntype B<T> = { next: B<T[]>; value: string };\n"
targets = {
    "forward": "{ root: { next: B<string[]> } } | { root: B<string> }",
    "reverse": "{ root: B<string> } | { root: { next: B<string[]> } }",
    "direct": "B<string>",
    "nested": "B<string[]>",
}
for name, target in targets.items():
    source = "{ root: A<string> }" if name in ("forward", "reverse") else ("A<string>" if name == "direct" else "A<string[]>")
    witness = oracle / f"{name}.ts"
    witness.write_text(preamble + f"declare const value: {source};\nconst result: {target} = value;\n")
    run(f"typescript-{name}", ["node", "node_modules/typescript/bin/tsc", "--ignoreConfig", "--noEmit", "--strict", "--skipLibCheck", "--pretty", "false", str(witness)], 1, "error TS2322:", limit=30)
test = (PROOF / "test.rs").read_text()
assert hashlib.sha256(test.encode()).hexdigest() == "27bf6b9d2459d2aff6cd9938bf48341dc2a1d70d572f177155a9c29c737ab91a"
assert original.endswith("}\n")
RELATION.write_text(original[:-2] + test + "}\n")
args = ["cargo", "test", "--locked", "-p", "bamts-compiler", "--lib"]
try:
    run("compile", [*args, "--no-run"])
    run("published-union-order", [*args, "checker::relations::tests::transformed_alias_union_order_is_stable", "--", "--exact", "--nocapture"], 101, "transformed alias union order changed compatibility")
finally:
    RELATION.write_text(original)
assert hashlib.sha256(RELATION.read_bytes()).hexdigest() == "3479aaa39ad13441b543153d340ad3a12b12e5b6c0303f11fe62911d194834d9"
run("restored-source", ["git", "diff", "--exit-code"])
print("BAMTI_ALIAS_ORDER_BASELINE_COMPLETE " + json.dumps({"head": head, "exits": EXITS}, sort_keys=True), flush=True)
