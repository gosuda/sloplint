//! `project.max_file_bytes` is capped at 16 MiB; larger values fail the run with exit 2.

use sloplint::{RunError, RunOptions, run};
use sloplint_core::ConfigError;
use sloplint_core::config::{MAX_SOURCE_FILE_BYTES, parse_config};

fn limit_config(limit: &str) -> String {
    format!("[project]\nmax_file_bytes = {limit}\n")
}

fn refused(limit: &str) -> ConfigError {
    let Err(error) = parse_config(&limit_config(limit), "sloplint.toml") else {
        panic!("max_file_bytes = {limit} must be rejected");
    };
    error
}

fn refused_message(limit: &str) -> String {
    let error = refused(limit);
    let ConfigError::Toml { origin, message } = &error else {
        panic!("max_file_bytes = {limit}: expected a TOML error, got: {error}");
    };
    assert_eq!(
        origin, "sloplint.toml",
        "max_file_bytes = {limit}: the error must name the file"
    );
    message.clone()
}

#[test]
fn limits_above_the_ceiling_are_rejected_naming_the_key_and_ceiling() {
    let ceiling = MAX_SOURCE_FILE_BYTES.to_string();
    for limit in [
        "16777217",
        "4294967294",
        "4294967295",
        "4294967296",
        "5000000000",
        "9223372036854775807",
        "-1",
    ] {
        let message = refused_message(limit);
        assert!(
            message.contains("max_file_bytes"),
            "max_file_bytes = {limit}: `{message}` lacks the key"
        );
        assert!(
            message.contains(ceiling.as_str()),
            "max_file_bytes = {limit}: `{message}` lacks the ceiling {ceiling}"
        );
    }
}

#[test]
fn limits_beyond_i64_are_rejected() {
    for limit in ["9223372036854775808", "18446744073709551615"] {
        refused(limit);
    }
}

#[test]
fn fractional_and_string_limits_are_rejected() {
    for (limit, token) in [("1.5", "1.5"), ("\"big\"", "big")] {
        let message = refused_message(limit);
        assert!(
            message.contains(token),
            "max_file_bytes = {limit}: `{message}` lacks `{token}`"
        );
    }
}

#[test]
fn limits_up_to_the_ceiling_are_accepted() {
    let ceiling = MAX_SOURCE_FILE_BYTES.to_string();
    for limit in ["0", "1", "2097152", ceiling.as_str()] {
        let config = parse_config(&limit_config(limit), "sloplint.toml")
            .unwrap_or_else(|e| panic!("max_file_bytes = {limit} rejected: {e}"));
        assert_eq!(
            config.project.max_file_bytes.to_string(),
            limit,
            "max_file_bytes = {limit} must load unchanged"
        );
    }
}

#[test]
fn an_oversize_limit_fails_the_run_with_exit_two() {
    let over = (MAX_SOURCE_FILE_BYTES + 1).to_string();
    let ceiling = MAX_SOURCE_FILE_BYTES.to_string();
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    std::fs::write(root.join("sloplint.toml"), limit_config(&over))
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
    assert_eq!(error.exit_code(), 2, "a bad limit must fail with exit 2");
    let text = format!("{error}");
    assert!(
        text.contains("max_file_bytes"),
        "the failure must name the key, got: {text}"
    );
    assert!(
        text.contains(ceiling.as_str()),
        "the failure must name the ceiling {ceiling}, got: {text}"
    );
}
