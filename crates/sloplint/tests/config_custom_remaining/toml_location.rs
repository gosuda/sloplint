//! TOML syntax and shape errors in `sloplint.toml` name where the input is wrong.

use std::path::Path;

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

fn located(src: &str, location: &str) {
    let error = toml_error(src);
    let ConfigError::Toml { origin, message } = &error else {
        panic!("expected a TOML error, got: {error}");
    };
    assert_eq!(origin, "sloplint.toml");
    assert!(
        message.starts_with(location),
        "`{message}` does not start with `{location}`"
    );
    assert!(
        error
            .to_string()
            .starts_with(&format!("sloplint.toml: {location}")),
        "display must lead with the origin and location: {error}"
    );
}

#[test]
fn toml_syntax_error_names_the_line() {
    located(
        "[project]\nstrictness = \"strict\nmode = 1\n",
        "line 2, column ",
    );
}

#[test]
fn toml_type_error_points_at_the_offending_value() {
    let src = "[project]\nstrictness = \"strict\"\nmax_file_bytes = \"x\"\n";
    located(src, "line 3, column 18: ");
    let message = toml_error(src).to_string();
    assert!(
        message.contains("invalid type"),
        "lost the cause: {message}"
    );
}

#[test]
fn toml_unknown_field_points_at_the_offending_key() {
    let src = "[project]\nstrictness = \"strict\"\n  bogus = 1\n";
    located(src, "line 3, column 3: ");
    let message = toml_error(src).to_string();
    assert!(
        message.contains("unknown field `bogus`"),
        "lost the cause: {message}"
    );
}

#[test]
fn toml_column_counts_characters_not_bytes() {
    let src = "project = { baseline = \"é\", max_file_bytes = \"x\" }\n";
    located(src, "line 1, column 46: ");
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
    assert!(
        message.starts_with("line 2, column 1: "),
        "`{message}` does not locate the unknown key"
    );
}

#[test]
fn valid_toml_still_loads() {
    let config = parse_config("[project]\nstrictness = \"strict\"\n", "sloplint.toml")
        .unwrap_or_else(|e| panic!("valid config rejected: {e}"));
    assert_eq!(config.project.strictness.as_str(), "strict");
}
