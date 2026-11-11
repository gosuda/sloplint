//! Configuration loading: explicit, implicit, and option merges.

use sloplint::{RunError, RunOptions, run};
use sloplint_core::finding::Severity;

use super::{SAMPLE, fired, run_ok, standard, tempdir, write};

#[test]
fn rule_option_override_merges_with_preset() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/cognitive-depth\"]\nlevel = \"major\"\nminor_at = 2\nmajor_at = 3\ncritical_at = 19\nblocker_at = 26\n",
    );
    for (count, expected) in [(2, Severity::Minor), (3, Severity::Major)] {
        let mut src = String::from("fn deep(v: u8) {\n");
        for n in 0..count {
            src.push_str(&format!("if v > {n} {{\n"));
        }
        src.push_str(&"}\n".repeat(count));
        src.push_str("}\n");
        write(&root, "src/deep.rs", &src);
        let report = run_ok(&standard(&root));
        let cognitive: Vec<_> = report
            .findings
            .iter()
            .filter(|f| &*f.rule == "complexity/cognitive-depth")
            .collect();
        assert_eq!(cognitive.len(), 1, "{count} nested ifs must fire once");
        assert_eq!(
            cognitive[0].severity, expected,
            "{count} nested ifs with major_at 3 must be {expected:?}"
        );
    }
}

#[test]
fn audit_config_missing_explicit_fails() {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", SAMPLE);
    let missing = root.join("missing.toml");
    let error = run(&RunOptions {
        config: Some(missing.clone()),
        ..standard(&root)
    })
    .expect_err("explicit missing config must fail");
    match &error {
        RunError::PathNotFound { path } => assert_eq!(
            path, &missing,
            "missing explicit config must carry its path"
        ),
        other => panic!("expected PathNotFound, got: {other}"),
    }
    assert_eq!(error.exit_code(), 2, "missing explicit config exits 2");
}

#[test]
fn audit_config_missing_implicit_uses_defaults() {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", SAMPLE);
    let report = run_ok(&standard(&root));
    assert!(
        fired(&report, "grep/generic-name"),
        "default profile must flag the generic name without a config file: {:?}",
        report.findings.iter().map(|f| &f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn audit_config_present_explicit_changes_level() {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", SAMPLE);
    write(
        &root,
        "custom.toml",
        "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n",
    );
    let plain = run_ok(&standard(&root));
    let custom = run_ok(&RunOptions {
        config: Some(root.join("custom.toml")),
        ..standard(&root)
    });
    let plain_sev = plain
        .findings
        .iter()
        .find(|f| &*f.rule == "grep/generic-name")
        .map(|f| f.severity);
    assert_eq!(plain_sev, Some(Severity::Minor), "default level is minor");
    assert!(
        !fired(&custom, "grep/generic-name"),
        "explicit config must disable the selected rule"
    );
}
