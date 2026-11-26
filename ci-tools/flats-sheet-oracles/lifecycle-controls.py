from __future__ import annotations

import ast
from collections.abc import Callable
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import sys
from typing import cast

root = Path(sys.argv[1]).resolve()
binary = Path(sys.argv[2]).resolve()
evidence = Path(sys.argv[3]).resolve()
proof = Path(__file__).resolve().parent
sys.path.insert(0, str(root / "internal/lifecyclecheck"))
import gate
from gate import Host

class OracleHost(Host):
    def __init__(self, work: Path, mutation: str) -> None:
        super().__init__(binary, work)
        self.mutation = mutation
        self.mutated = False

    def mcp(self, name: str, **args: object) -> dict[str, object]:
        result = cast(dict[str, object], super().mcp(name, **args))
        if self.mutated or args.get("slug") != "boundary-sheet":
            return result
        before = name == "get_sheet" and self.mutation == "creation"
        after = name == "update_sheet" and self.mutation in ("version", "reservation", "approval")
        if not before and not after:
            return result
        if self.mutation == "approval":
            self.save("extra-request", gate.static("EXTRA-PENDING"))
            self.create_version("extra-request")
        else:
            with sqlite3.connect(self.data / "flats.db", timeout=5.0) as conn:
                conn.execute("PRAGMA foreign_keys=ON")
                if self.mutation == "version":
                    changed = conn.execute("UPDATE versions SET pruned=1 WHERE flat='boundary-sheet' AND number=1")
                    assert changed.rowcount == 1
                elif self.mutation == "reservation":
                    conn.execute("INSERT INTO version_numbers(flat,number) VALUES('boundary-sheet',2)")
                elif self.mutation == "creation":
                    columns = "flat,number,hash,size,files,kind,manifest,git_sha,git_dirty,message,created_at,screenshot,pruned,published"
                    values = "flat,2,hash,size,files,kind,manifest,git_sha,git_dirty,message,created_at,screenshot,pruned,published"
                    changed = conn.execute(f"INSERT INTO versions({columns}) SELECT {values} FROM versions WHERE flat='boundary-sheet' AND number=1")
                    assert changed.rowcount == 1
        self.mutated = True
        self.trace.append({"kind": "oracle-negative-control", "mutation": self.mutation,
                           "rows": gate.version_rows(self, "boundary-sheet"),
                           "approvals": gate.approval_states(self)})
        return result

old_source = (proof / "original-gate.py").read_text()
original = ast.parse(old_source, filename="original-gate.py")
old_function = next(node for node in original.body if isinstance(node, ast.FunctionDef) and node.name == "sheet_writes_need_no_approval")
old_namespace = gate.__dict__.copy()
exec(compile(ast.Module(body=[old_function], type_ignores=[]), "original-gate.py", "exec"), old_namespace)
old_check = cast(Callable[[Host, str], None], old_namespace["sheet_writes_need_no_approval"])
new_check = cast(Callable[[Host, str], None], gate.sheet_writes_need_no_approval)
evidence.mkdir(parents=True, exist_ok=True)
results: list[dict[str, object]] = []
expected = {
    "version": "sheet write changed the version rows",
    "reservation": "sheet write changed the version rows",
    "approval": "sheet write changed the approval list",
    "creation": "sheet snapshot must hold only its creation row and reservation",
}
for name, check in (("old", old_check), ("candidate", new_check)):
    for mutation in ("none", *expected):
        host = OracleHost(evidence / (name + "-" + mutation), mutation)
        error = ""
        try:
            host.start()
            host.save("pending-request", gate.static("PENDING"))
            pending = host.create_version("pending-request")
            host.initialize_mcp()
            try:
                check(host, pending)
            except AssertionError as failure:
                error = str(failure)
            assert mutation == "none" or host.mutated, f"{name}/{mutation}: control was not applied"
            if name == "old" or mutation == "none":
                assert not error, f"{name}/{mutation}: unexpected error: {error}"
            else:
                assert expected[mutation] in error, f"{name}/{mutation}: wrong failure: {error}"
            record = {"oracle": name, "mutation": mutation, "detected": bool(error), "message": error,
                      "rows": gate.version_rows(host, "boundary-sheet")}
            results.append(record)
            print("SHEET_LIFECYCLE_CONTROL " + json.dumps(record, sort_keys=True), flush=True)
        finally:
            host.stop()
            (host.work / "trace.json").write_text(json.dumps(host.trace, indent=2) + "\n")
(evidence / "results.json").write_text(json.dumps({
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source": gate.source_identity(),
    "original_oracle_sha256": hashlib.sha256(old_source.encode()).hexdigest(),
    "results": results,
    "scope": "actual disposable product host; focused Sheet oracle, no full lifecycle or human-approval acceptance claim"
}, indent=2) + "\n")
assert len(results) == 10
print("SHEET_LIFECYCLE_CONTROLS_PASS", flush=True)
