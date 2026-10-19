//! `project.max_file_bytes` is bounded by the 32-bit source positions the analyzer uses.

use sloplint::{RunError, RunOptions, run};
use sloplint_core::ConfigError;
use sloplint_core::config::parse_config;

fn limit_config(limit: &str) -> String {
    format!("[project]\nmax_file_bytes = {limit}\n")
}

#[test]
fn limits_above_u32_max_are_rejected_with_the_value_and_type_named() {
    for limit in ["4294967296", "5000000000", "9223372036854775807"] {
        let Err(error) = parse_config(&limit_config(limit), "sloplint.toml") else {
            panic!("max_file_bytes = {limit} must be rejected");
        };
        let ConfigError::Toml { origin, message } = &error else {
            panic!("max_file_bytes = {limit}: expected a TOML error, got: {error}");
        };
        assert_eq!(origin, "sloplint.toml");
        for fragment in [limit, "u32"] {
            assert!(
                message.contains(fragment),
                "max_file_bytes = {limit}: `{message}` lacks `{fragment}`"
            );
        }
    }
}

#[test]
fn negative_and_fractional_limits_are_rejected() {
    for limit in ["-1", "1.5", "\"big\""] {
        let Err(error) = parse_config(&limit_config(limit), "sloplint.toml") else {
            panic!("max_file_bytes = {limit} must be rejected");
        };
        assert!(
            matches!(error, ConfigError::Toml { .. }),
            "max_file_bytes = {limit}: expected a TOML error, got: {error}"
        );
    }
}

#[test]
fn limits_up_to_u32_max_are_accepted() {
    for limit in ["0", "1", "2097152", "4294967294", "4294967295"] {
        let config = parse_config(&limit_config(limit), "sloplint.toml")
            .unwrap_or_else(|e| panic!("max_file_bytes = {limit} rejected: {e}"));
        assert_eq!(config.project.max_file_bytes.to_string(), limit);
    }
}

#[test]
fn an_oversized_limit_fails_the_run_with_exit_two() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    std::fs::write(root.join("sloplint.toml"), limit_config("5000000000"))
        .unwrap_or_else(|e| panic!("cannot write the config: {e}"));
    std::fs::write(root.join("a.rs"), "pub fn handle() {}\n")
        .unwrap_or_else(|e| panic!("cannot write the source: {e}"));
    let options = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    };
    let Err(error) = run(&options) else {
        panic!("an unsupported max_file_bytes must stop the run");
    };
    assert!(
        matches!(error, RunError::Config(_)),
        "an unsupported max_file_bytes must be a configuration error, got: {error}"
    );
    assert_eq!(error.exit_code(), 2);
}
