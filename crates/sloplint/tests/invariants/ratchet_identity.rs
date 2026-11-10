//! Ratchet identity: baselines hide only what they stored.

use sloplint::RunOptions;
use sloplint_core::config::{Mode, Strictness};

use super::{audit_filter_opts, baseline_write, ratchet_gate, run_ok, standard, tempdir, write};

#[test]
fn audit_unused_ratchet_reports_only_new() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(grep/generic-name): keep\npub fn foo() {}\n",
    );
    run_ok(&baseline_write(&root));
    assert_eq!(
        run_ok(&ratchet_gate(&root)).findings.len(),
        0,
        "baseline must hide its unused"
    );
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(grep/generic-name): keep\npub fn foo() {}\n// sloplint-allow(grep/generic-name): keep\npub fn bar() {}\n",
    );
    let gated = run_ok(&ratchet_gate(&root));
    assert_eq!(
        gated.findings.len(),
        1,
        "second identical suppression must surface"
    );
    assert_eq!(gated.summary.baselined, 1, "old unused must stay baselined");
    assert_eq!(
        &*gated.findings[0].rule, "meta/unused-suppression",
        "new finding must be unused"
    );
    assert_eq!(
        gated.findings[0].line, 3,
        "new unused must be the appended one"
    );
    assert!(
        !gated.findings[0].fingerprint.is_empty(),
        "unused must carry a fingerprint"
    );
    let text =
        std::fs::read_to_string(root.join("src/lib.rs")).unwrap_or_else(|e| panic!("read: {e}"));
    write(&root, "src/lib.rs", &format!("\n{text}"));
    let shifted = run_ok(&ratchet_gate(&root));
    assert_eq!(
        shifted.findings.len(),
        1,
        "blank line must not duplicate the unused"
    );
    assert_eq!(
        shifted.findings[0].line, 4,
        "shift must move the new unused"
    );
    assert_eq!(
        shifted.findings[0].fingerprint, gated.findings[0].fingerprint,
        "shift must preserve identity"
    );
}

#[test]
fn audit_suppressed_duplicate_survivor_takes_first_ordinal() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "pub fn handle() {}\npub fn handle() {}\n",
    );
    let before: Vec<_> = run_ok(&standard(&root))
        .findings
        .into_iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .collect();
    assert_eq!(before.len(), 2, "need duplicate ordinals");
    assert_ne!(
        before[0].fingerprint, before[1].fingerprint,
        "duplicates must differ by ordinal"
    );
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(grep/generic-name): dup\npub fn handle() {}\npub fn handle() {}\n",
    );
    let after = run_ok(&standard(&root));
    let kept: Vec<_> = after
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .collect();
    assert_eq!(kept.len(), 1, "suppression must drop only the first");
    assert_eq!(
        kept[0].fingerprint, before[0].fingerprint,
        "first ordinal survives suppression so deleting the suppressed duplicate does not renumber"
    );
}

#[test]
fn audit_file_budget_whole_file_ratchet_identity() {
    let (_tmp, root) = tempdir();
    let src = "fn anchor() {}\n".to_owned() + &"fn f() {}\n".repeat(809);
    write(&root, "src/big.rs", &src);
    let threshold = || {
        run_ok(&RunOptions {
            strictness: Some(Strictness::Strict),
            ..audit_filter_opts(&root, &["agent/file-token-budget"])
        })
    };
    let before = threshold();
    assert_eq!(before.findings.len(), 1, "need one file-budget finding");
    run_ok(&RunOptions {
        write_baseline: true,
        strictness: Some(Strictness::Strict),
        ..audit_filter_opts(&root, &["agent/file-token-budget"])
    });
    let text =
        std::fs::read_to_string(root.join("src/big.rs")).unwrap_or_else(|e| panic!("read: {e}"));
    write(&root, "src/big.rs", &format!("\n{text}"));
    let gated = run_ok(&RunOptions {
        mode: Some(Mode::Ratchet),
        strictness: Some(Strictness::Strict),
        ..audit_filter_opts(&root, &["agent/file-token-budget"])
    });
    assert!(
        gated.findings.is_empty(),
        "leading blank line must not re-report"
    );
    assert_eq!(gated.summary.baselined, 1, "baselined count must hold");
    let after = threshold();
    assert_eq!(after.findings[0].line, 1, "shifted finding stays on line 1");
    assert_eq!(
        after.findings[0].message,
        "file has 811 lines (minor at 600, major at 1000, critical at 1500, blocker at 2000)"
    );
}

#[test]
fn audit_shadowed_nesting_deletion_stays_baselined() {
    let (_tmp, root) = tempdir();
    let head = "pub fn nested(a: bool, b: bool, c: bool, d: bool, e: bool) -> i32 {\n";
    let first = format!(
        "{head}    if a {{\n        if b {{\n            if c {{\n                if d {{\n                    if e {{\n                        return 1;\n                    }}\n                }}\n            }}\n        }}\n    }}\n    0\n}}\n"
    );
    let second = format!(
        "{head}    match a {{\n        true => match b {{\n            true => match c {{\n                true => match d {{\n                    true => match e {{\n                        true => match a {{\n                            true => 1,\n                            false => 0,\n                        }},\n                        false => 0,\n                    }},\n                    false => 0,\n                }},\n                false => 0,\n            }},\n            false => 0,\n        }},\n        false => 0,\n    }}\n}}\n"
    );
    write(&root, "src/lib.rs", &format!("{first}{second}"));
    let depth_only = || {
        audit_filter_opts(
            &root,
            &[
                "complexity/cognitive-depth",
                "complexity/nesting-depth",
                "meta/*",
            ],
        )
    };
    let plain = run_ok(&RunOptions {
        strictness: Some(Strictness::Strict),
        ..depth_only()
    });
    assert!(
        plain
            .findings
            .iter()
            .any(|f| &*f.rule == "complexity/cognitive-depth"),
        "the if-nested twin must fire cognitive depth"
    );
    assert_eq!(
        plain
            .findings
            .iter()
            .filter(|f| &*f.rule == "complexity/nesting-depth")
            .count(),
        1,
        "the cognitive-shadowed twin stays silent"
    );
    run_ok(&RunOptions {
        write_baseline: true,
        strictness: Some(Strictness::Strict),
        ..depth_only()
    });
    write(&root, "src/lib.rs", &second);
    let gated = run_ok(&RunOptions {
        mode: Some(Mode::Ratchet),
        strictness: Some(Strictness::Strict),
        ..depth_only()
    });
    assert!(
        gated.findings.is_empty(),
        "deleting the shadowed twin must not re-report the survivor"
    );
}
