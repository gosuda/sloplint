use super::*;

fn callees(language: &str) -> Vec<String> {
    Config::default()
        .languages
        .get(language)
        .unwrap_or_else(|| panic!("missing log-callee defaults for {language}"))
        .log_callees
        .clone()
}

fn assert_callees(language: &str, keep: &[&str], drop: &[&str]) {
    let current = callees(language);
    for name in keep {
        assert!(
            current.iter().any(|candidate| candidate.as_str() == *name),
            "{language} defaults must still match {name}: {}",
            current.join(",")
        );
    }
    for name in drop {
        assert!(
            current.iter().all(|candidate| candidate.as_str() != *name),
            "{language} defaults must not match {name}"
        );
    }
}

#[test]
fn default_log_callees_keep_message_bearers_only() {
    assert_callees(
        "rust",
        &["trace!", "debug!", "info!", "warn!", "error!"],
        &["anyhow!", "bail!", "eyre!", "panic!"],
    );
    assert_callees(
        "typescript",
        &["console.error", "logger.error"],
        &["new *Error"],
    );
    assert_callees(
        "javascript",
        &["console.error", "logger.error"],
        &["new *Error"],
    );
    assert_callees(
        "python",
        &["logging.error", "logger.exception"],
        &["*Error", "*Exception"],
    );
    assert_callees(
        "go",
        &["log.Fatal", "slog.Error"],
        &["fmt.Errorf", "errors.New"],
    );
    assert_callees(
        "java",
        &["logger.error", "LOG.fatal"],
        &[
            "printStackTrace",
            "System.err.println",
            "System.out.println",
        ],
    );
    assert_callees(
        "kotlin",
        &["logger.error", "LOG.error"],
        &["printStackTrace", "System.err.println", "println"],
    );
    assert_callees(
        "csharp",
        &["Console.WriteLine", "logger.LogError"],
        &["new *Exception"],
    );
    assert_callees(
        "cpp",
        &["spdlog::error", "printf"],
        &["*Error", "*Exception", "*_error"],
    );
    assert_callees("php", &["error_log", "Log::error"], &["new *Exception"]);
    assert_callees(
        "scala",
        &["logger.error", "println"],
        &["new *Exception", "new *Error"],
    );
    assert_callees(
        "dart",
        &["print", "logger.severe"],
        &["*Exception", "*Error"],
    );
    assert_callees(
        "zig",
        &["std.log.err", "log.debug"],
        &["panic!", "compileError!"],
    );
    assert_callees(
        "ocaml",
        &["Logs.err", "prerr_endline"],
        &["failwith", "invalid_arg"],
    );
    assert_callees("odin", &["log.error", "fmt.eprintf"], &["panic"]);
    assert_callees("starlark", &["print"], &["fail"]);
}

#[test]
fn max_file_bytes_ceiling_accepts_the_ceiling_and_rejects_above() {
    let ceiling = MAX_SOURCE_FILE_BYTES.to_string();
    let config = parse_config(
        &format!("[project]\nmax_file_bytes = {ceiling}\n"),
        "sloplint.toml",
    )
    .unwrap_or_else(|error| panic!("the ceiling must load: {error}"));
    assert_eq!(
        config.project.max_file_bytes, MAX_SOURCE_FILE_BYTES,
        "the ceiling itself must be accepted"
    );
    let over = MAX_SOURCE_FILE_BYTES + 1;
    let Err(error) = parse_config(
        &format!("[project]\nmax_file_bytes = {over}\n"),
        "sloplint.toml",
    ) else {
        panic!("one byte past the ceiling must be rejected");
    };
    let ConfigError::Toml { message, .. } = &error else {
        panic!("past the ceiling must be a TOML error, got: {error}");
    };
    assert!(
        message.contains("max_file_bytes") && message.contains(ceiling.as_str()),
        "the refusal must name the key and the ceiling, got: {message}"
    );
}
