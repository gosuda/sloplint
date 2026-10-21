//! `parse_config` error variants and the documented key set that must load.

use sloplint_core::config::{Mode, Strictness, parse_config};
use sloplint_core::{ConfigError, FailOn, RuleFilter, RuleSet};

fn rejected(src: &str) -> ConfigError {
    match parse_config(src, "sloplint.toml") {
        Ok(_) => panic!("config was accepted:\n{src}"),
        Err(error) => error,
    }
}

fn toml_cause(src: &str, fragments: &[&str]) {
    let error = rejected(src);
    let ConfigError::Toml { origin, message } = &error else {
        panic!("expected a TOML error for:\n{src}\ngot: {error}");
    };
    assert_eq!(origin, "sloplint.toml");
    for fragment in fragments {
        assert!(
            message.contains(fragment),
            "`{message}` lacks `{fragment}` for:\n{src}"
        );
    }
    assert!(
        error.to_string().starts_with("sloplint.toml: "),
        "display must lead with the origin: {error}"
    );
}

#[test]
fn enum_settings_name_every_valid_value() {
    toml_cause(
        "[project]\nstrictness = \"lax\"\n",
        &["lax", "minimal", "basic", "standard", "strict"],
    );
    toml_cause(
        "[project]\nmode = \"fast\"\n",
        &["fast", "threshold", "ratchet"],
    );
    toml_cause(
        "[project]\nfail_on = \"fatal\"\n",
        &["fatal", "warn", "deny", "never"],
    );
    toml_cause(
        "[project]\nstrictness = \"Strict\"\n",
        &["Strict", "strict"],
    );
}

#[test]
fn unknown_fields_are_rejected_in_every_table() {
    for (src, key) in [
        ("bogus = 1\n", "bogus"),
        ("[unknown_table]\n", "unknown_table"),
        ("[project]\nbogus = 1\n", "bogus"),
        ("[languages.rust]\nbogus = 1\n", "bogus"),
        ("[[custom_rules]]\nbogus = 1\n", "bogus"),
    ] {
        toml_cause(src, &[format!("unknown field `{key}`").as_str()]);
    }
}

#[test]
fn wrong_value_types_are_rejected() {
    for (src, fragment) in [
        ("[project]\nlanguages = \"rust\"\n", "invalid type"),
        ("[project]\nexclude = [1]\n", "invalid type"),
        ("[project]\nmax_file_bytes = -1\n", "expected u"),
        ("[project]\nmax_file_bytes = 1.5\n", "expected u"),
        (
            "[languages.rust]\nlog_callees = \"trace!\"\n",
            "invalid type",
        ),
        ("rules = 3\n", "invalid type"),
    ] {
        toml_cause(src, &[fragment]);
    }
}

#[test]
fn unknown_languages_are_typed_errors() {
    for (src, value) in [
        ("[project]\nlanguages = [\"rust\", \"cobol\"]\n", "cobol"),
        ("[project]\nlanguages = [\"Rust\"]\n", "Rust"),
        ("[languages.cobol]\nlog_callees = []\n", "cobol"),
        ("[languages.TypeScript]\nlog_callees = []\n", "TypeScript"),
    ] {
        let error = rejected(src);
        let ConfigError::UnknownLanguage { value: named } = &error else {
            panic!("expected an unknown language error for:\n{src}\ngot: {error}");
        };
        assert_eq!(named, value);
        assert!(
            error.to_string().contains(value),
            "display omits it: {error}"
        );
    }
}

#[test]
fn invalid_globs_name_their_key_and_pattern() {
    for key in [
        "exclude",
        "extend_exclude",
        "test_globs",
        "custom_rule_files",
    ] {
        let src = format!("[project]\n{key} = [\"src/**\", \"[\"]\n");
        let error = rejected(&src);
        let ConfigError::Glob {
            key: named, glob, ..
        } = &error
        else {
            panic!("expected a glob error for {key}, got: {error}");
        };
        assert_eq!(named, &format!("project.{key}"));
        assert_eq!(glob, "[");
        assert!(
            error.to_string().contains(&format!("project.{key}")),
            "display omits the key: {error}"
        );
    }
}

#[test]
fn invalid_log_callee_patterns_name_their_language() {
    for (pattern, fragment) in [
        ("", "empty callee pattern"),
        ("*", "needs a suffix"),
        ("a b", "whitespace inside callee pattern"),
    ] {
        let src = format!("[languages.rust]\nlog_callees = [{pattern:?}]\n");
        let error = rejected(&src);
        let ConfigError::RuleOption { rule, message } = &error else {
            panic!("expected a rule option error for {pattern:?}, got: {error}");
        };
        assert_eq!(rule, "languages.rust");
        assert!(
            message.contains(fragment),
            "{pattern:?}: `{message}` lacks `{fragment}`"
        );
    }
}

const FULL_CONFIG: &str = r#"
[project]
strictness = "strict"
languages = ["rust", "typescript"]
mode = "ratchet"
baseline = "base.json"
fail_on = "warn"
exclude = ["**/gen/**"]
extend_exclude = ["**/vendored/**"]
test_globs = ["**/it/**"]
max_file_bytes = 1048576
custom_rule_files = ["rules/*.toml"]

[languages.rust]
log_callees = ["trace!", "new *Error"]

[rules."complexity/cognitive-depth"]
deny_at = 18

[rules."agent/colocated-tests"]
level = "warn"

[[custom_rules]]
id = "custom/no-unwrap"
language = "rust"
severity = "deny"
message = "`{method}()` in library code"
why = "unwrap panics on the error path"
fix = "propagate with ?"
query = '''
(call_expression
  function: (field_expression field: (field_identifier) @method)
  (#eq? @method "unwrap")) @match
'''
include = ["src/**"]
exclude = ["**/tests/**"]
"#;

#[test]
fn every_documented_key_loads() {
    let config = parse_config(FULL_CONFIG, "sloplint.toml")
        .unwrap_or_else(|e| panic!("the documented configuration was rejected: {e}"));
    let project = &config.project;
    assert_eq!(project.strictness, Strictness::Strict);
    assert_eq!(project.mode, Mode::Ratchet);
    assert_eq!(project.fail_on, FailOn::Warn);
    assert_eq!(project.languages, ["rust", "typescript"]);
    assert_eq!(project.baseline, "base.json");
    assert_eq!(project.exclude, ["**/gen/**"]);
    assert_eq!(project.extend_exclude, ["**/vendored/**"]);
    assert_eq!(project.test_globs, ["**/it/**"]);
    assert_eq!(project.max_file_bytes, 1_048_576);
    assert_eq!(project.custom_rule_files, ["rules/*.toml"]);
    assert_eq!(
        config.languages["rust"].log_callees,
        ["trace!", "new *Error"]
    );
    assert_eq!(config.custom_rules.len(), 1);
    assert_eq!(config.custom_rules[0].id, "custom/no-unwrap");
    let rules = RuleSet::new(&config, &RuleFilter::default())
        .unwrap_or_else(|e| panic!("the documented configuration builds no rules: {e}"));
    assert!(
        rules.is_active("agent/colocated-tests"),
        "the rule table must enable colocated-tests"
    );
    assert!(
        rules.is_active("custom/no-unwrap"),
        "the custom rule must be active"
    );
}
