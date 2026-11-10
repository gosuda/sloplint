//! Baseline name, missing-file, and corruption failures through the public API and the CLI.

use super::{
    BASELINE, RATCHET, assert_cli_failure, assert_io, assert_usage, baseline_doc, cli, corrupt,
    entries, failure, io_failure, mkdir, opts, project, put, ratchet, run_ok, set_baseline, str_of,
    tally, text, writing,
};
use sloplint::{execute, run};
use std::io::ErrorKind::NotFound;

// AA-I10 gaps: empty and dot baseline names, and a directory as the destination.
#[test]
fn baseline_empty_dot_and_directory_names_are_refused_or_io() {
    let (_tmp, root) = project();
    set_baseline(&root, "");
    for options in [writing(&root), ratchet(&root)] {
        let error = failure(run(&options), "empty baseline name");
        assert_usage(&error, "BaselineOutsideRoot");
        let message = error.to_string();
        assert!(message.contains("outside the project root"), "{message}");
    }
    mkdir(&root.join("sub"));
    for name in [".", "sub/.."] {
        set_baseline(&root, name);
        for options in [writing(&root), ratchet(&root)] {
            let (path, kind) = io_failure(run(&options), name);
            assert_eq!(path, root, "{name} resolves to the root itself");
            assert_ne!(kind, NotFound, "{name}");
        }
        assert!(root.join("sample.rs").is_file(), "{name}: root damaged");
    }
    let dir = root.join("bdir");
    mkdir(&dir);
    set_baseline(&root, "bdir");
    for (options, what) in [(writing(&root), "write"), (ratchet(&root), "ratchet")] {
        let (path, kind) = io_failure(run(&options), what);
        assert_eq!(path, dir, "{what}");
        assert_ne!(kind, NotFound, "{what}: not a missing baseline");
    }
    let left = std::fs::read_dir(&dir).map(|d| d.count());
    assert_eq!(left.ok(), Some(0), "directory was touched");
    let args = ["baseline", "--no-cache"];
    assert_cli_failure(&root, &args, 3, &[str_of(&dir), "not a regular file"]);
}

// AA-S12: a missing leaf is BaselineMissing (exit 2, hint); a missing parent is Io (exit 3).
#[test]
fn baseline_missing_leaf_and_parent_are_distinct_failures() {
    let (_tmp, root) = project();
    let error = failure(run(&ratchet(&root)), "missing baseline");
    assert_usage(&error, "BaselineMissing");
    let (hint, leaf) = ("run 'sloplint baseline'", root.join(BASELINE));
    assert!(error.to_string().contains(hint), "{error}");
    assert!(error.to_string().contains(str_of(&leaf)), "{error}");
    set_baseline(&root, "missing-dir/b.json");
    let parent = root.join("missing-dir");
    for (options, what) in [(ratchet(&root), "ratchet"), (writing(&root), "write")] {
        assert_io(run(&options), what, &parent, NotFound);
    }
    assert!(!parent.exists(), "a failed write created the parent");
    assert_cli_failure(&root, &RATCHET, 3, &[str_of(&parent)]);
}

// AA-S11: corrupt, truncated, and unsupported-version baselines fail closed with exit 2.
#[test]
fn baseline_corruption_fails_closed_with_exit_2() {
    let (_tmp, root) = project();
    let live = run_ok(&opts(&root));
    let n = tally(&live).0;
    assert!(n > 0, "the sample must produce findings");
    let (real, bad) = (entries(&live), "invalid baseline JSON");
    let good = baseline_doc("1", &real, "");
    let partial = r#"{"version":1,"findings":[{"path":"a.rs","rule":"x/y"}]}"#;
    corrupt(&root, "not json", "{not json", bad);
    corrupt(&root, "empty file", "", bad);
    corrupt(&root, "truncated", &good[..good.len() / 2], bad);
    corrupt(&root, "no findings key", r#"{"version":1}"#, bad);
    corrupt(&root, "no fingerprint", partial, bad);
    corrupt(&root, "v0", &baseline_doc("0", &real, ""), "version 0");
    corrupt(&root, "v2", &baseline_doc("2", &real, ""), "version 2");
    assert_cli_failure(&root, &RATCHET, 2, &["version 2"]);
    put(&root, BASELINE, &baseline_doc("1", "", ""));
    let open = run_ok(&ratchet(&root));
    assert_eq!(tally(&open), (n, 0, 0), "version 1 with no entries");
}

// AA-S9, AA-S10, AA-X10: duplicates collapse, unknown rules go stale, extra fields are ignored.
#[test]
fn baseline_duplicates_unknown_rules_and_extra_fields() {
    let (_tmp, root) = project();
    let live = run_ok(&opts(&root));
    let n = tally(&live).0;
    assert!(n > 0, "the sample must produce findings");
    let real = entries(&live);
    let unknown = r#"{"path":"sample.rs","rule":"no/such-rule","fingerprint":"0000"}"#;
    let gate = |doc: String| {
        put(&root, BASELINE, &doc);
        run_ok(&ratchet(&root))
    };
    let tripled = gate(baseline_doc("1", &format!("{real},{real},{real}"), ""));
    assert_eq!(tally(&tripled), (0, n, 0), "duplicates inflated counts");
    let stale = gate(baseline_doc("1", &format!("{real},{unknown}"), ""));
    assert_eq!(tally(&stale), (0, n, 1), "unknown rule is stale");
    let execution = execute(&ratchet(&root)).unwrap_or_else(|e| panic!("execute: {e}"));
    assert!(!execution.failed, "stale entries must not fail the gate");
    let noted = real.replace('}', r#","note":"hand edit"}"#);
    let extra = gate(baseline_doc("1", &noted, r#","generator":"hand","x":[1]"#));
    assert_eq!(tally(&extra), (0, n, 0), "extra fields are ignored");
}

#[test]
fn baseline_without_a_skipped_set_reads_as_empty() {
    let parsed = sloplint_core::baseline::Baseline::from_json(&baseline_doc("1", "", ""))
        .unwrap_or_else(|e| panic!("old baseline must parse: {e}"));
    assert!(
        parsed.skipped.is_empty(),
        "a missing skipped set means no recorded skips"
    );
    assert!(
        !parsed.to_json().contains("skipped"),
        "an empty skipped set stays out of the document: {}",
        parsed.to_json()
    );
}

#[test]
fn skipped_gate_names_a_newly_skipped_file_with_exit_1() {
    let (_tmp, root) = project();
    let out = cli(&root, &["baseline", "--no-cache"]);
    assert!(
        out.status.success(),
        "baseline must record: {}",
        text(&out.stderr)
    );
    put(
        &root,
        "hidden.rs",
        "// do not edit\npub fn handle(flag: bool) {}\n",
    );
    let gated = [
        "check",
        "--no-cache",
        "--mode",
        "ratchet",
        "--fail-on",
        "never",
        "--fail-on-skipped",
    ];
    let out = cli(&root, &gated);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a newly skipped file fails the gate: {}",
        text(&out.stderr)
    );
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("hidden.rs"),
        "the gate names the skipped file: {stdout}"
    );
    assert!(
        stdout.contains("skipped=1"),
        "the summary still counts the skip: {stdout}"
    );
    let open = [
        "check",
        "--no-cache",
        "--mode",
        "ratchet",
        "--fail-on",
        "never",
    ];
    let out = cli(&root, &open);
    assert!(
        out.status.success(),
        "the same run passes with the gate off: {}",
        text(&out.stderr)
    );
}
