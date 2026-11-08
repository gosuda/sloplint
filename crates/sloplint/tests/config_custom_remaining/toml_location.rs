//! TOML syntax and shape errors in `sloplint.toml` name where the input is wrong.

use std::path::Path;

use super::numbers;
use sloplint::{RunError, RunOptions, run};
use sloplint_core::ConfigError;
use sloplint_core::config::parse_config;

fn write(root: &Path, rel: &str, text: &str) {
    std::fs::write(root.join(rel), text).unwrap_or_else(|e| panic!("cannot write {rel}: {e}"));
}

fn toml_error(src: &str) -> ConfigError {
    match parse_config(src, "sloplint.toml") {
        Ok(_) => panic!("config was accepted:\n{src}"),
        Err(error) => error,
    }
}

fn located(src: &str, expected: &[u64]) -> String {
    let error = toml_error(src);
    let ConfigError::Toml { origin, message } = &error else {
        panic!("expected a TOML error, got: {error}");
    };
    assert_eq!(origin, "sloplint.toml", "wrong origin: {error}");
    holds_numbers_in_order(message, expected);
    message.clone()
}

fn holds_numbers_in_order(message: &str, expected: &[u64]) {
    let seen = numbers(message);
    let mut rest = seen.iter();
    for wanted in expected {
        assert!(
            rest.any(|&number| number == *wanted),
            "`{message}` lacks the location number {wanted} in order (saw {seen:?})"
        );
    }
}

#[test]
fn toml_syntax_error_names_the_line() {
    located("[project]\nstrictness = \"strict\nmode = 1\n", &[2]);
}

#[test]
fn toml_type_error_points_at_the_offending_value() {
    let src = "[project]\nstrictness = \"strict\"\nmax_file_bytes = \"x\"\n";
    let message = located(src, &[3, 18]);
    assert!(message.contains("\"x\""), "lost the value: {message}");
}

#[test]
fn toml_unknown_field_points_at_the_offending_key() {
    let src = "[project]\nstrictness = \"strict\"\n  bogus = 1\n";
    let message = located(src, &[3, 3]);
    assert!(message.contains("bogus"), "lost the key: {message}");
}

#[test]
fn toml_column_counts_characters_not_bytes() {
    located(
        "project = { baseline = \"é\", max_file_bytes = \"x\" }\n",
        &[1, 46],
    );
}

#[test]
fn custom_rule_file_errors_name_the_file_line_and_column() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(
        root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"rules.toml\"]\n",
    );
    write(root, "rules.toml", "[[custom_rules]]\nbogus = 1\n");
    let options = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    };
    let Err(RunError::Config(ConfigError::Toml { origin, message })) = run(&options) else {
        panic!("a malformed custom rule file must be a TOML configuration error");
    };
    assert_eq!(origin, "rules.toml");
    holds_numbers_in_order(&message, &[2, 1]);
}

#[test]
fn valid_toml_still_loads() {
    let config = parse_config("[project]\nstrictness = \"strict\"\n", "sloplint.toml")
        .unwrap_or_else(|e| panic!("valid config rejected: {e}"));
    assert_eq!(config.project.strictness.as_str(), "strict");
}
