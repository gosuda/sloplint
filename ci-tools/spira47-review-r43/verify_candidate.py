"""Verify fixture metadata consumption without changing production."""
from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import asdict
from pathlib import Path

import archive_support
import baseline_support as base
import linter_support
import native_support as native
import observer
import proof_support as proof
import runtime_observer

ROLE_TEST = "cmd/sqlc-gen-spira/version_relation_test.lgo"
ROLE_DATA = "cmd/sqlc-gen-spira/testdata/version-relation-roles.json"
PIPE_TEST = "cmd/sqlc-gen-spira/sqlc_postgres_version_test.lgo"
PIPE_DATA = "cmd/sqlc-gen-spira/testdata/sqlc-version-pipeline.json"
DOC = "docs/specs/sqlc-gen-spira.md"


def rows(value: object, count: int) -> list[dict[str, object]]:
    if not isinstance(value, list) or len(value) != count:
        raise ValueError("authored path count differs")
    result = [base.object_value(row) for row in value]
    if len({base.text(row["path"]) for row in result}) != count:
        raise ValueError("duplicate authored path")
    for row in result:
        path = Path(base.text(row["path"]))
        data = base.text(row["content"]).encode()
        if path.is_absolute() or ".." in path.parts or row["mode"] != "100644":
            raise ValueError("invalid authored path")
        if (base.digest(data) != row["sha256"]
                or hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest() != row["blob"]):
            raise RuntimeError("authored path content differs")
    return result


def check_append(original: str, replacement: str, old_count: int, additions: list[dict[str, object]]) -> None:
    before: object = json.loads(original)
    after: object = json.loads(replacement)
    if (not isinstance(before, list) or len(before) != old_count
            or after != before + additions or not original.endswith("\n]\n")
            or not replacement.startswith(original[:-3] + ",\n")):
        raise RuntimeError("original fixture prefix changed")


def proposal(manifest: dict[str, object]) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    authored = base.object_value(json.loads((base.PROOF / "proposal.json").read_bytes()))
    candidate = rows(authored["files"], 5)
    counter = rows(base.object_value(authored["counterfactual"])["files"], 2)
    if ({base.text(row["path"]) for row in candidate} != {ROLE_TEST, ROLE_DATA, PIPE_TEST, PIPE_DATA, DOC}
            or {base.text(row["path"]) for row in counter} != {ROLE_TEST, ROLE_DATA}):
        raise RuntimeError("test/docs path scope differs")
    if (authored["candidate_tree"] != base.object_value(manifest["candidate"])["tree"]
            or authored["counterfactual_tree"] != base.object_value(manifest["counterfactual"])["tree"]
            or authored["production_changes"] is not False):
        raise RuntimeError("authored source scope differs")
    indexed = {base.text(row["path"]): row for row in candidate}
    old_role = (base.SOURCE / ROLE_TEST).read_text(encoding="utf-8")
    if old_role.count("len(cases) != 30") != 1 or old_role.count("want 30") != 1:
        raise RuntimeError("published reader count shape differs")
    count_only = old_role.replace("len(cases) != 30", "len(cases) != 32").replace("want 30", "want 32")
    expected_counter = {ROLE_TEST: count_only, ROLE_DATA: indexed[ROLE_DATA]["content"]}
    if any(row["content"] != expected_counter[base.text(row["path"])] for row in counter):
        raise RuntimeError("counterfactual changes more than the count and appended fixtures")
    additions = observer.rows_from("metadata-cases.json", 2)
    predictions = observer.rows_from("expected-new-pipeline.json", 2)
    check_append((base.SOURCE / ROLE_DATA).read_text(encoding="utf-8"),
                 base.text(indexed[ROLE_DATA]["content"]), 30, additions)
    check_append((base.SOURCE / PIPE_DATA).read_text(encoding="utf-8"),
                 base.text(indexed[PIPE_DATA]["content"]), 67, predictions)
    old_pipeline = (base.SOURCE / PIPE_TEST).read_text(encoding="utf-8")
    changed_count = old_pipeline.replace("len(cases) != 67", "len(cases) != 69").replace("want 67", "want 69")
    if indexed[PIPE_TEST]["content"] != changed_count:
        raise RuntimeError("permanent SQLc helper changes beyond its case count")
    return candidate, counter


def apply(authored: list[dict[str, object]], lane: str) -> None:
    for row in authored:
        path = base.SOURCE / base.text(row["path"])
        if not path.is_file():
            raise RuntimeError("test-only proposal unexpectedly adds a source path")
        path.write_bytes(base.text(row["content"]).encode())
        path.chmod(0o644)
    base.run("stage-" + lane, ["git", "add", "--"] + [base.text(row["path"]) for row in authored])


def outcome(output: str, test: str, names: set[str], failed: set[str]) -> dict[str, list[str]]:
    actual = proof.test_outcomes(output, test, names)
    if set(actual["failed"]) != failed or set(actual["passed"]) != names - failed:
        raise RuntimeError("frozen outcome partition differs: " + test)
    parent = "FAIL" if failed else "PASS"
    if re.search(r"^\s*--- " + parent + ": " + re.escape(test) + r" ", output, re.MULTILINE) is None:
        raise RuntimeError("parent outcome absent: " + test)
    return actual


def restore(source: dict[str, object], name: str) -> None:
    base.run(name, ["git", "restore", "--source", base.text(source["head"]), "--staged", "--worktree", "--", "."])


def metadata_observations(report: dict[str, object], manifest: dict[str, object]) -> None:
    cases = observer.rows_from("metadata-cases.json", 2)
    predictions = observer.rows_from("expected-new-pipeline.json", 2)
    plugin = base.BUILD / "unchanged-plugin"
    base.run("build-unchanged-plugin", ["logos", "build", "-o", str(plugin), "./cmd/sqlc-gen-spira"], timeout=900)
    report["unchanged_plugin_sha256"] = base.digest(plugin.read_bytes())
    originals: list[dict[str, object]] = []
    pipeline: list[dict[str, object]] = []
    report["native_original"] = originals
    report["metadata_pipeline"] = pipeline
    with native.postgres(manifest) as (container, version):
        report["postgres_version_num"] = version
        for case, expected in zip(cases, predictions, strict=True):
            if (case["name"] != expected["name"] or case["sql"] != expected["sql"]
                    or observer.columns(case) != expected["columns"]
                    or case["option"] != expected["option"] or expected["upstream"] is not False):
                raise RuntimeError("metadata oracle input differs")
            originals.append(observer.observe_native(container, case, base.text(case["sql"]), "original"))
            row = observer.observe_pipeline(container, case, plugin, "metadata", base.text(expected["error"]))
            if (row["request_present"] is not True or row["processed_sql"] != expected["text"]
                    or row["pipeline_contract"] != ("plugin-refusal" if expected["error"] else "generated")):
                raise RuntimeError("actual metadata handoff differs from its frozen prediction")
            pipeline.append(row)
        if [row["pipeline_contract"] for row in pipeline] != ["plugin-refusal", "generated"]:
            raise RuntimeError("metadata pipeline outcome order differs")


def runtime_pipeline(report: dict[str, object], manifest: dict[str, object]) -> None:
    real, launcher, old_path = runtime_observer.install_argv_observer()
    try:
        report["real_logos"] = {"path": str(real), "sha256": base.digest(real.read_bytes())}
        output = base.run("candidate-permanent-pipeline",
                          ["logos", "test", "-race", "-exec", str(launcher), "-count=1", "-v",
                           "-run", "^TestSQLcPostgresVersionPipeline$", "./cmd/sqlc-gen-spira"], timeout=1800)
        contracts = base.object_value(manifest["contracts"])
        report["candidate_permanent_pipeline"] = outcome(
            output, "TestSQLcPostgresVersionPipeline", observer.names(contracts["pipeline_names"]), set())
        observed = [base.object_value(json.loads(match)) for match in
                    re.findall(r"COMPILED_SQLC_QUERY_VALUES (\{[^\n]+\})", output)]
        if observed != [contracts["compiled_query_values"]]:
            raise RuntimeError("compiled query values differ")
        report["compiled_query_values"] = observed[0]
        report["nested_test_argv"] = runtime_observer.nested_race_records()
    finally:
        os.environ["PATH"] = old_path


def main() -> None:
    token = os.environ.pop("SPIRA_ARTIFACT_TOKEN")
    manifest = base.object_value(json.loads((base.PROOF / "candidate-manifest.json").read_bytes()))
    for name, expected in base.object_value(manifest["proof_sha256"]).items():
        if Path(name).name != name or base.digest((base.PROOF / name).read_bytes()) != expected:
            raise RuntimeError("proof content differs: " + name)
    workflow = base.PROOF.parent.parent / ".github/workflows/spira47-review-r43.yml"
    if base.digest(workflow.read_bytes()) != manifest["workflow_sha256"]:
        raise RuntimeError("active workflow differs")
    source = base.object_value(manifest["source"])
    contracts = base.object_value(manifest["contracts"])
    os.environ["GOWORK"] = "off"
    initial_path = os.environ["PATH"]
    report: dict[str, object] = {
        "schema": 1, "producer": os.environ["GITHUB_SHA"], "run": os.environ["GITHUB_RUN_ID"],
        "manifest": manifest, "manifest_sha256": base.digest((base.PROOF / "candidate-manifest.json").read_bytes()),
        "native_controls_verified": False, "counterfactual_verified": False,
        "focused_gates_passed": False, "source_gates_passed": False,
        "source_restored": False, "publication_eligible": False,
    }
    try:
        if base.run("proof-head", ["git", "-C", str(base.PROOF.parent.parent), "rev-parse", "HEAD"]).strip() != os.environ["GITHUB_SHA"]:
            raise RuntimeError("proof checkout differs")
        if base.run("source-head", ["git", "rev-parse", "HEAD"]).strip() != source["head"]:
            raise RuntimeError("source checkout differs")
        report["before"] = proof.source_snapshot(manifest, "source")
        report["authenticated_previous"] = archive_support.authenticate_previous(manifest, token)
        report["authenticated_linter"] = linter_support.install_linter(manifest, token)
        candidate, counter = proposal(manifest)
        base.run("logos-version", ["logos", "version"])
        if base.run("go-cache-disabled", ["go", "env", "GOCACHEPROG"]).strip():
            raise RuntimeError("external compiler cache active")
        base.run("sqlc-modules", ["go", "-C", str(base.SOURCE / "tools"), "mod", "download"], timeout=900)
        module = base.object_value(json.loads(base.run(
            "sqlc-module", ["go", "-C", str(base.SOURCE / "tools"), "list", "-m", "-json", "github.com/sqlc-dev/sqlc"])))
        if module["Version"] != manifest["sqlc_version"]:
            raise RuntimeError("SQLc module differs")
        metadata_observations(report, manifest)
        report["after_native"] = proof.source_snapshot(manifest, "source")
        if report["after_native"] != report["before"]:
            raise RuntimeError("native observer changed source")
        report["native_controls_verified"] = True
        apply(counter, "counterfactual")
        report["counterfactual_before"] = proof.source_snapshot(manifest, "counterfactual")
        base.run("counterfactual-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                               "./cmd/sqlc-gen-spira"], timeout=1200)
        output = base.run("counterfactual-relation-roles",
                          ["logos", "test", "-race", "-count=1", "-v", "-run",
                           "^TestPostgresRelationReferenceRoles$", "./cmd/sqlc-gen-spira"],
                          expected=1, timeout=1200)
        report["counterfactual_roles"] = outcome(
            output, "TestPostgresRelationReferenceRoles", observer.names(contracts["role_names"]),
            observer.names(contracts["counterfactual_failed"]))
        report["counterfactual_after"] = proof.source_snapshot(manifest, "counterfactual")
        if report["counterfactual_before"] != report["counterfactual_after"]:
            raise RuntimeError("counterfactual bytes changed")
        restore(source, "restore-counterfactual-source")
        report["counterfactual_restored"] = proof.source_snapshot(manifest, "source")
        if report["counterfactual_restored"] != report["before"]:
            raise RuntimeError("counterfactual restoration differs")
        base.run("counterfactual-clean", ["git", "diff", "--exit-code", "HEAD"])
        if base.run("counterfactual-untracked", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
            raise RuntimeError("counterfactual left untracked source")
        report["counterfactual_verified"] = True
        apply(candidate, "candidate")
        report["candidate_before"] = proof.source_snapshot(manifest, "candidate")
        base.run("candidate-test-compile", ["logos", "test", "-race", "-count=1", "-run", "^$",
                                           "./cmd/sqlc-gen-spira"], timeout=1200)
        output = base.run("candidate-relation-roles",
                          ["logos", "test", "-race", "-count=1", "-v", "-run",
                           "^TestPostgresRelationReferenceRoles$", "./cmd/sqlc-gen-spira"], timeout=1200)
        report["candidate_roles"] = outcome(
            output, "TestPostgresRelationReferenceRoles", observer.names(contracts["role_names"]), set())
        runtime_pipeline(report, manifest)
        report["focused_gates_passed"] = True
        base.run("just-gate", ["just", "gate"], timeout=3600)
        base.run("just-slop", ["just", "slop"], timeout=1200)
        base.run("just-eject", ["just", "eject"], timeout=1800)
        base.run("candidate-whitespace", ["git", "diff", "--cached", "--check", "HEAD"])
        base.run("candidate-unstaged", ["git", "diff", "--exit-code"])
        if base.run("candidate-untracked", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
            raise RuntimeError("unexpected untracked candidate source")
        report["candidate_after"] = proof.source_snapshot(manifest, "candidate")
        if report["candidate_after"] != report["candidate_before"]:
            raise RuntimeError("candidate bytes changed during gates")
        report["source_gates_passed"] = True
    finally:
        os.environ["PATH"] = initial_path
        try:
            restore(source, "restore-original-source")
            report["restored"] = proof.source_snapshot(manifest, "source")
            base.run("source-restoration", ["git", "diff", "--exit-code", "HEAD"])
            if base.run("untracked-restoration", ["git", "ls-files", "--others", "--exclude-standard"]).strip():
                raise RuntimeError("unexpected untracked source after restoration")
            report["source_restored"] = True
        finally:
            commands = [asdict(row) for row in base.RECEIPTS]
            unique = len({row["name"] for row in commands}) == len(commands)
            report["commands"] = commands
            report["unique_command_names"] = unique
            report["publication_eligible"] = bool(
                unique and report["native_controls_verified"] and report["counterfactual_verified"]
                and report["focused_gates_passed"] and report["source_gates_passed"] and report["source_restored"])
            (base.OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("SPIRA_R43_CANDIDATE_REPORT " + json.dumps(report), flush=True)
            if not unique:
                raise RuntimeError("command names must remain unique")


if __name__ == "__main__":
    main()
