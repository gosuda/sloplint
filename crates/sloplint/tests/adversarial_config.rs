//! Configuration rejection boundaries: rule tables and custom-rule specs must
//! fail with descriptive errors whatever the strictness profile or rule level.

use std::collections::BTreeSet;
use std::fmt::Display;
use std::path::{Path, PathBuf};

use sloplint::{Report, RunError, RunOptions, run};
use sloplint_core::config::{Mode, Strictness, parse_config};
use sloplint_core::{CATALOG, ConfigError, Level, RuleFilter, RulePattern, RuleSet, Severity};

const PROFILES: [Strictness; 4] = [
    Strictness::Minimal,
    Strictness::Basic,
    Strictness::Standard,
    Strictness::Strict,
];

const LEVEL_LINES: [&str; 4] = [
    "",
    "level = \"off\"\n",
    "level = \"warn\"\n",
    "level = \"deny\"\n",
];

const NESTED: &str = "complexity/nested-mutation";
const AT_LEAST_1: &str = "must be at least 1";
const AT_LEAST_2: &str = "must be at least 2";
const SIMILARITY: &str = "must be greater than 0 and at most 1";
const BAD_GLOB: &str = "invalid glob `[`";
const UNKNOWN_BOGUS: &str = "unknown field `bogus`";
const LEVEL_ONLY: &str = "custom rules accept only the `level` key";
const PROJECT_TABLE: &str = "[project]\nstrictness = \"strict\"\n";
const RULES_TABLE: &str = "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n";

const THRESHOLD_CASES: [(u32, u32, bool); 10] = [
    (0, 0, false),
    (0, 5, false),
    (1, 0, false),
    (1, 1, true),
    (1, 2, true),
    (2, 1, false),
    (5, 4, false),
    (5, 5, true),
    (1, u32::MAX, true),
    (u32::MAX, u32::MAX, true),
];

const FLOORS: [(&str, &str, &str, Option<&str>); 21] = [
    (
        "agent/unique-patch-anchors",
        "window",
        "1",
        Some(AT_LEAST_2),
    ),
    ("agent/unique-patch-anchors", "window", "2", None),
    (
        "agent/unique-patch-anchors",
        "min_repeats",
        "1",
        Some(AT_LEAST_2),
    ),
    ("agent/unique-patch-anchors", "min_repeats", "2", None),
    (
        "grep/homonym-definitions",
        "min_files",
        "1",
        Some(AT_LEAST_2),
    ),
    ("grep/homonym-definitions", "min_files", "2", None),
    ("slop/field-copy", "min_fields", "1", Some(AT_LEAST_2)),
    ("slop/field-copy", "min_fields", "2", None),
    (
        "grep/contiguous-log-literal",
        "min_len",
        "0",
        Some(AT_LEAST_1),
    ),
    ("grep/contiguous-log-literal", "min_len", "1", None),
    (
        "slop/echo-comment",
        "min_similarity",
        "0.0",
        Some(SIMILARITY),
    ),
    (
        "slop/echo-comment",
        "min_similarity",
        "1.5",
        Some(SIMILARITY),
    ),
    (
        "slop/echo-comment",
        "min_similarity",
        "nan",
        Some(SIMILARITY),
    ),
    (
        "slop/echo-comment",
        "min_similarity",
        "-0.0",
        Some(SIMILARITY),
    ),
    (
        "slop/echo-comment",
        "min_similarity",
        "-1.0",
        Some(SIMILARITY),
    ),
    (
        "slop/echo-comment",
        "min_similarity",
        "inf",
        Some(SIMILARITY),
    ),
    (
        "slop/echo-comment",
        "min_similarity",
        "-inf",
        Some(SIMILARITY),
    ),
    ("slop/echo-comment", "min_similarity", "0.5", None),
    ("slop/echo-comment", "min_similarity", "1.0", None),
    (
        "agent/colocated-tests",
        "exclude",
        "[\"[\"]",
        Some(BAD_GLOB),
    ),
    ("agent/colocated-tests", "exclude", "[\"src/**\"]", None),
];

const WRONG_TYPES: [(&str, &str, &str, &str, &str); 7] = [
    (NESTED, "warn_at", "\"3\"", "\"3\"", "u32"),
    (NESTED, "warn_at", "-1", "-1", "u32"),
    (NESTED, "deny_at", "1.5", "1.5", "u32"),
    (NESTED, "warn_at", "true", "true", "u32"),
    (
        "slop/echo-comment",
        "min_similarity",
        "\"high\"",
        "\"high\"",
        "f64",
    ),
    (
        "agent/colocated-tests",
        "exclude",
        "\"src/**\"",
        "src/**",
        "sequence",
    ),
    (
        "grep/homonym-definitions",
        "min_files",
        "[1]",
        "sequence",
        "u32",
    ),
];

const CUSTOM_DEFAULTS: [(&str, &str); 7] = [
    ("id", "custom/ok"),
    ("language", "rust"),
    ("severity", "warn"),
    ("message", "hit"),
    ("why", "test why"),
    ("fix", "test fix"),
    ("query", "(call_expression) @match"),
];

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir {parent:?}: {e}"));
    }
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {path:?}: {e}"));
}

fn project(config: &str, files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path().to_path_buf();
    write(&root, "src/lib.rs", "pub fn handle(flag: bool) {}\n");
    write(&root, "sloplint.toml", config);
    for (rel, text) in files {
        write(&root, rel, text);
    }
    (tmp, root)
}

fn options(root: &Path, strictness: Strictness) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(strictness),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    }
}

fn run_at(root: &Path, strictness: Strictness) -> Result<Report, RunError> {
    run(&options(root, strictness))
}

fn level_profiles(profiles: &[Strictness]) -> Vec<(&'static str, Strictness)> {
    let mut grid = Vec::new();
    for level in LEVEL_LINES {
        grid.extend(profiles.iter().map(|strictness| (level, *strictness)));
    }
    grid
}

fn selector(pattern: &str) -> RuleFilter {
    let parsed = RulePattern::parse(pattern).unwrap_or_else(|e| panic!("selector: {e}"));
    RuleFilter(vec![parsed])
}

fn config_error(result: Result<Report, RunError>, context: &str) -> ConfigError {
    let Err(error) = result else {
        panic!("{context}: configuration must be rejected");
    };
    assert_eq!(error.exit_code(), 2, "{context}: must exit 2, got: {error}");
    match error {
        RunError::Config(config) => config,
        other => panic!("{context}: expected a configuration error, got: {other}"),
    }
}

fn accepted_run(config: &str, files: &[(&str, &str)], strictness: Strictness, context: &str) {
    let (_tmp, root) = project(config, files);
    if let Err(error) = run_at(&root, strictness) {
        panic!("{context}: valid configuration rejected: {error}");
    }
}

fn rule_set(src: &str, strictness: Strictness) -> Result<RuleSet, ConfigError> {
    let mut config = parse_config(src, "sloplint.toml")?;
    config.project.strictness = strictness;
    RuleSet::new(&config, &RuleFilter::default())
}

fn rejected(src: &str, strictness: Strictness, context: &str) -> ConfigError {
    match rule_set(src, strictness) {
        Ok(_) => panic!("{context}: configuration was accepted:\n{src}"),
        Err(error) => error,
    }
}

fn accepted(src: &str, strictness: Strictness, context: &str) -> RuleSet {
    rule_set(src, strictness).unwrap_or_else(|e| panic!("{context}: valid config rejected: {e}"))
}

fn rule_option(error: &ConfigError, rule: &str, fragment: &str, context: &str) {
    let ConfigError::RuleOption {
        rule: named,
        message,
    } = error
    else {
        panic!("{context}: expected a rule option error, got: {error}");
    };
    assert_eq!(named, rule, "{context}: error names the wrong rule");
    assert!(
        message.contains(fragment),
        "{context}: `{message}` lacks `{fragment}`"
    );
    assert!(
        error.to_string().contains(rule),
        "{context}: display omits the rule: {error}"
    );
}

fn threshold_error(
    error: &ConfigError,
    rule: &str,
    warn_at: impl Display,
    deny_at: impl Display,
    context: &str,
) {
    rule_option(error, rule, &format!("warn_at {warn_at}"), context);
    rule_option(error, rule, &format!("deny_at {deny_at}"), context);
}

fn table(rule: &str, level: &str, body: &str) -> String {
    format!("[rules.\"{rule}\"]\n{level}{body}")
}

fn preset_int(rule: &str, strictness: Strictness, key: &str) -> i64 {
    let info = CATALOG.iter().find(|info| info.id == rule);
    let info = info.unwrap_or_else(|| panic!("{rule} is not in the catalog"));
    let options = (info.preset)(strictness).options;
    let value = options.get(key).and_then(toml::Value::as_integer);
    value.unwrap_or_else(|| panic!("{rule} preset has no integer `{key}` at {strictness:?}"))
}

fn custom_rule(overrides: &[(&str, &str)]) -> String {
    let mut out = String::from("[[custom_rules]]\n");
    for (key, default) in CUSTOM_DEFAULTS {
        let found = overrides.iter().find(|(name, _)| *name == key);
        let value = found.map_or(default, |(_, value)| *value);
        out.push_str(&format!("{key} = {value:?}\n"));
    }
    out
}

fn files_config(paths: &[&str]) -> String {
    let list: Vec<String> = paths.iter().map(|path| format!("{path:?}")).collect();
    format!("[project]\ncustom_rule_files = [{}]\n", list.join(", "))
}

fn custom_error(config: &str, files: &[(&str, &str)], id: &str, fragment: &str, context: &str) {
    let (_tmp, root) = project(config, files);
    let error = config_error(run_at(&root, Strictness::Standard), context);
    let ConfigError::CustomRule { id: named, message } = &error else {
        panic!("{context}: expected a custom rule error, got: {error}");
    };
    assert_eq!(named, id, "{context}: error names the wrong rule");
    assert!(
        message.contains(fragment),
        "{context}: `{message}` lacks `{fragment}`"
    );
    assert!(
        error.to_string().contains(id),
        "{context}: display omits the rule: {error}"
    );
}

fn file_error(files: &[(&str, &str)], origin: &str, fragment: &str, context: &str) {
    let paths: Vec<&str> = files.iter().map(|(path, _)| *path).collect();
    let (_tmp, root) = project(&files_config(&paths), files);
    let error = config_error(run_at(&root, Strictness::Standard), context);
    let ConfigError::Toml {
        origin: named,
        message,
    } = &error
    else {
        panic!("{context}: expected a TOML error naming the file, got: {error}");
    };
    assert_eq!(named, origin, "{context}: error names the wrong file");
    assert!(
        message.contains(fragment),
        "{context}: `{message}` lacks `{fragment}`"
    );
    assert!(
        error.to_string().contains(origin),
        "{context}: display omits the file: {error}"
    );
}

fn assert_unknown_rule(root: &Path, id: &str, strictness: Strictness, context: &str) {
    let error = config_error(run_at(root, strictness), context);
    let ConfigError::UnknownRule { id: named, valid } = &error else {
        panic!("{context}: expected UnknownRule, got: {error}");
    };
    assert_eq!(named, id, "{context}: error names the wrong id");
    assert!(valid.is_sorted(), "{context}: valid ids must be sorted");
    assert!(
        valid.iter().any(|v| v == "meta/parse-error"),
        "{context}: ids lack meta: {valid:?}"
    );
    assert!(
        error.to_string().contains(id),
        "{context}: display omits the id: {error}"
    );
}

#[test]
fn audit_config_unknown_rule_id_is_rejected_at_every_profile_and_level() {
    let ids = "typo/x complexity/cognitive-dept complexity/* custom/ghost Grep/x";
    for id in ids.split(' ') {
        for level in LEVEL_LINES {
            let (_tmp, root) = project(&table(id, level, ""), &[]);
            for strictness in PROFILES {
                let context = format!("{id} {level:?} {strictness:?}");
                assert_unknown_rule(&root, id, strictness, &context);
            }
        }
    }
}

#[test]
fn audit_config_threshold_boundaries_hold_for_every_rule_profile_and_level() {
    let rules: BTreeSet<&str> = CATALOG
        .iter()
        .filter(|info| {
            let options = (info.preset)(Strictness::Strict).options;
            options.contains_key("warn_at") && options.contains_key("deny_at")
        })
        .map(|info| info.id)
        .collect();
    assert!(
        rules.len() >= 9 && rules.contains(NESTED),
        "threshold rules vanished: {rules:?}"
    );
    for rule in rules {
        for (warn_at, deny_at, valid) in THRESHOLD_CASES {
            let body = format!("warn_at = {warn_at}\ndeny_at = {deny_at}\n");
            for (level, strictness) in level_profiles(&PROFILES) {
                let src = table(rule, level, &body);
                let context = format!("{rule} {warn_at}/{deny_at} {level:?} {strictness:?}");
                if valid {
                    accepted(&src, strictness, &context);
                    continue;
                }
                let error = rejected(&src, strictness, &context);
                threshold_error(&error, rule, warn_at, deny_at, &context);
            }
        }
    }
}

#[test]
fn audit_config_partial_threshold_overrides_validate_against_the_preset() {
    for (level, strictness) in level_profiles(&PROFILES) {
        let warn = preset_int(NESTED, strictness, "warn_at");
        let deny = preset_int(NESTED, strictness, "deny_at");
        let context = format!("preset {warn}/{deny} {level:?} {strictness:?}");
        let high = table(NESTED, level, &format!("warn_at = {}\n", deny + 1));
        let error = rejected(&high, strictness, &context);
        threshold_error(&error, NESTED, deny + 1, deny, &context);
        let low = table(NESTED, level, &format!("deny_at = {}\n", warn - 1));
        let error = rejected(&low, strictness, &context);
        threshold_error(&error, NESTED, warn, warn - 1, &context);
        let at_deny = table(NESTED, level, &format!("warn_at = {deny}\n"));
        accepted(&at_deny, strictness, &context);
        let at_warn = table(NESTED, level, &format!("deny_at = {warn}\n"));
        accepted(&at_warn, strictness, &context);
    }
}

#[test]
fn audit_config_numeric_floors_are_enforced_for_disabled_rules() {
    for (rule, key, literal, rejection) in FLOORS {
        for (level, strictness) in level_profiles(&PROFILES) {
            let src = table(rule, level, &format!("{key} = {literal}\n"));
            let context = format!("{rule} {key}={literal} {level:?} {strictness:?}");
            let Some(fragment) = rejection else {
                accepted(&src, strictness, &context);
                continue;
            };
            rule_option(
                &rejected(&src, strictness, &context),
                rule,
                fragment,
                &context,
            );
        }
    }
}

#[test]
fn audit_config_unknown_option_is_rejected_for_every_rule_profile_and_level() {
    for info in CATALOG {
        for (level, strictness) in level_profiles(&PROFILES) {
            let src = table(info.id, level, "not_an_option = 1\n");
            let context = format!("{} {level:?} {strictness:?}", info.id);
            let error = rejected(&src, strictness, &context);
            rule_option(&error, info.id, "unknown field `not_an_option`", &context);
        }
    }
    let meta = table("meta/parse-error", "level = \"off\"\n", "bogus = 1\n");
    let error = rejected(&meta, Strictness::Strict, "meta rule option");
    rule_option(&error, "meta/parse-error", "`bogus`", "meta rule option");
    let typo = table(NESTED, "level = \"off\"\n", "warn_att = 2\n");
    let error = rejected(&typo, Strictness::Minimal, "misspelled key");
    for fragment in ["unknown field `warn_att`", "`warn_at`", "`deny_at`"] {
        rule_option(&error, NESTED, fragment, "misspelled key");
    }
}

#[test]
fn audit_config_wrong_type_option_values_name_the_key_value_and_type() {
    for (rule, key, literal, value, expected) in WRONG_TYPES {
        for (level, strictness) in level_profiles(&[Strictness::Minimal, Strictness::Strict]) {
            let src = table(rule, level, &format!("{key} = {literal}\n"));
            let context = format!("{rule} {key}={literal} {level:?} {strictness:?}");
            let error = rejected(&src, strictness, &context);
            let key_fragment = format!("`{key}`");
            for fragment in [key_fragment.as_str(), value, expected] {
                rule_option(&error, rule, fragment, &context);
            }
        }
    }
}

#[test]
fn audit_config_invalid_level_values_name_the_key_and_value() {
    for literal in ["\"fatal\"", "\"Warn\"", "3", "true"] {
        let src = format!("[rules.\"{NESTED}\"]\nlevel = {literal}\n");
        let error = parse_config(&src, "sloplint.toml").err();
        let error = error.unwrap_or_else(|| panic!("level = {literal} was accepted"));
        rule_option(&error, NESTED, "level", literal);
        rule_option(&error, NESTED, literal.trim_matches('"'), literal);
    }
}

#[test]
fn audit_config_bad_rule_tables_fail_through_run_with_exit_two() {
    let cases = [
        (NESTED, "warn_at = 9\ndeny_at = 1\n", AT_LEAST_1),
        (NESTED, "warn_at = 0\ndeny_at = 4\n", AT_LEAST_1),
        (NESTED, "bogus = 1\n", UNKNOWN_BOGUS),
        ("meta/parse-error", "bogus = 1\n", UNKNOWN_BOGUS),
        ("grep/homonym-definitions", "min_files = 1\n", AT_LEAST_2),
        ("agent/colocated-tests", "exclude = [\"[\"]\n", BAD_GLOB),
    ];
    for (rule, body, fragment) in cases {
        for level in ["", "level = \"off\"\n"] {
            let (_tmp, root) = project(&table(rule, level, body), &[]);
            for strictness in [Strictness::Minimal, Strictness::Strict] {
                let context = format!("{rule} {body:?} {level:?} {strictness:?}");
                let error = config_error(run_at(&root, strictness), &context);
                rule_option(&error, rule, fragment, &context);
            }
        }
    }
}

#[test]
fn audit_config_rule_filter_does_not_hide_invalid_rule_tables() {
    let filter = selector("complexity/cognitive-depth");
    let bad_src = table("grep/homonym-definitions", "", "min_files = 1\n");
    let mut bad = parse_config(&bad_src, "sloplint.toml").unwrap_or_else(|e| panic!("{e}"));
    let mut good = parse_config("", "sloplint.toml").unwrap_or_else(|e| panic!("{e}"));
    for strictness in PROFILES {
        bad.project.strictness = strictness;
        good.project.strictness = strictness;
        let context = format!("{strictness:?}");
        let Err(error) = RuleSet::new(&bad, &filter) else {
            panic!("{context}: a filtered-out rule hid its invalid table");
        };
        rule_option(&error, "grep/homonym-definitions", "min_files 1", &context);
        let Ok(set) = RuleSet::new(&good, &filter) else {
            panic!("{context}: valid configuration rejected under a filter");
        };
        assert!(
            set.is_active("complexity/cognitive-depth"),
            "{context}: selected rule off"
        );
        assert!(
            !set.is_active("complexity/nesting-depth"),
            "{context}: filtered rule on"
        );
    }
}

#[test]
fn audit_config_profiles_activate_exactly_their_preset_rules() {
    for strictness in PROFILES {
        let set = accepted("", strictness, &format!("{strictness:?}"));
        for info in CATALOG {
            let expected = (info.preset)(strictness).level != Level::Off;
            assert_eq!(
                set.is_active(info.id),
                expected,
                "{} under {strictness:?}",
                info.id
            );
        }
    }
}

fn nesting_severities(config: &str) -> Vec<Severity> {
    let source = format!(
        "fn deep(v: u8) {{\n{}{}}}\n",
        "if v > 0 {\n".repeat(8),
        "}\n".repeat(8)
    );
    let (_tmp, root) = project(config, &[("src/deep.rs", source.as_str())]);
    let rule_filter = selector("complexity/nesting-depth");
    let report = run(&RunOptions {
        rule_filter,
        ..options(&root, Strictness::Standard)
    });
    let report = report.unwrap_or_else(|e| panic!("run: {e}"));
    report
        .findings
        .iter()
        .map(|finding| finding.severity)
        .collect()
}

#[test]
fn audit_config_level_caps_and_disables_still_apply_after_validation() {
    let rule = "complexity/nesting-depth";
    assert_eq!(
        nesting_severities(""),
        [Severity::Deny],
        "preset denies depth 8"
    );
    let warn = table(rule, "level = \"warn\"\n", "");
    assert_eq!(
        nesting_severities(&warn),
        [Severity::Warn],
        "level warn caps deny"
    );
    let deny = table(rule, "level = \"deny\"\n", "");
    assert_eq!(
        nesting_severities(&deny),
        [Severity::Deny],
        "level deny keeps deny"
    );
    let off = table(rule, "level = \"off\"\n", "");
    assert!(
        nesting_severities(&off).is_empty(),
        "level off reports nothing"
    );
    let raised = table(rule, "", "warn_at = 9\ndeny_at = 9\n");
    assert!(
        nesting_severities(&raised).is_empty(),
        "thresholds above depth 8 pass"
    );
    let both = format!(
        "{off}{}",
        table(NESTED, "level = \"off\"\n", "warn_at = 3\n")
    );
    assert!(
        nesting_severities(&both).is_empty(),
        "valid options elsewhere change nothing"
    );
    let set = accepted(
        &both,
        Strictness::Standard,
        "off with valid override stays buildable",
    );
    assert!(!set.is_active(rule), "off nesting-depth must stay inactive");
    assert!(
        !set.is_active(NESTED),
        "off nested-mutation with warn_at override must stay inactive"
    );
    let mut filtered_config = parse_config(&raised, "sloplint.toml")
        .unwrap_or_else(|e| panic!("valid override rejected: {e}"));
    filtered_config.project.strictness = Strictness::Standard;
    let filtered = RuleSet::new(&filtered_config, &selector(NESTED))
        .unwrap_or_else(|e| panic!("filtered valid override rejected: {e}"));
    assert!(
        !filtered.is_active(rule),
        "filter-excluded nesting-depth with valid override must stay inactive"
    );
}

#[test]
fn audit_custom_rule_ids_must_be_well_formed() {
    let bad_ids = "custom/Upper custom/ custom Custom/x custom/a--b custom/-a custom/a_b \
                   custom/a/b grep/generic-name";
    for bad in bad_ids.split(' ').chain([""]) {
        let fragment = format!("id must match custom/[a-z0-9]+(-[a-z0-9]+)*, got `{bad}`");
        custom_error(&custom_rule(&[("id", bad)]), &[], bad, &fragment, bad);
    }
    for good in ["custom/a", "custom/a-b-c", "custom/a1-2b", "custom/0"] {
        accepted_run(
            &custom_rule(&[("id", good)]),
            &[],
            Strictness::Standard,
            good,
        );
    }
}

#[test]
fn audit_custom_rule_ids_must_be_unique_across_config_and_files() {
    let id = [("id", "custom/dup")];
    let fragment = "duplicate id `custom/dup`";
    let inline = format!("{}{}", custom_rule(&id), custom_rule(&id));
    custom_error(&inline, &[], "custom/dup", fragment, "inline twice");
    let dup = custom_rule(&id);
    let mixed = format!("{}{dup}", files_config(&["extra.toml"]));
    let extra = [("extra.toml", dup.as_str())];
    custom_error(&mixed, &extra, "custom/dup", fragment, "inline and file");
    let pair = [("a.toml", dup.as_str()), ("b.toml", dup.as_str())];
    let config = files_config(&["a.toml", "b.toml"]);
    custom_error(&config, &pair, "custom/dup", fragment, "two files");
}

#[test]
fn audit_custom_rule_fields_must_not_be_blank() {
    for field in ["message", "why", "fix", "query"] {
        for blank in ["", " ", "\t", " \n "] {
            let fragment = format!("{field} must not be empty");
            let src = custom_rule(&[(field, blank)]);
            custom_error(
                &src,
                &[],
                "custom/ok",
                &fragment,
                &format!("{field}={blank:?}"),
            );
        }
    }
    let blank = custom_rule(&[("message", " ")]);
    let file = [("rules.toml", blank.as_str())];
    let config = files_config(&["rules.toml"]);
    let fragment = "message must not be empty";
    custom_error(
        &config,
        &file,
        "custom/ok",
        fragment,
        "blank message in a file",
    );
}

#[test]
fn audit_custom_rule_language_and_globs_must_be_valid() {
    for language in ["cobol", "Rust", "rust ", ""] {
        let fragment = format!("unknown language `{language}`");
        let src = custom_rule(&[("language", language)]);
        custom_error(&src, &[], "custom/ok", &fragment, language);
    }
    for (key, glob) in [("include", "["), ("exclude", "{a")] {
        let src = format!("{}{key} = [{glob:?}]\n", custom_rule(&[]));
        let fragment = format!("invalid glob `{glob}`");
        custom_error(&src, &[], "custom/ok", &fragment, key);
    }
}

#[test]
fn audit_custom_rule_queries_must_compile_with_a_match_capture() {
    let no_match = "query must define a @match capture";
    let placeholder = "message placeholder {nope} names no capture in the query";
    let cases = [
        (
            "unbalanced",
            custom_rule(&[("query", "(call_expression @match")]),
            "query error",
        ),
        (
            "unknown node",
            custom_rule(&[("query", "(no_such_node) @match")]),
            "query error",
        ),
        (
            "wrong grammar",
            custom_rule(&[("language", "python")]),
            "query error",
        ),
        (
            "no capture",
            custom_rule(&[("query", "(call_expression) @call")]),
            no_match,
        ),
        (
            "placeholder",
            custom_rule(&[("message", "found {nope}")]),
            placeholder,
        ),
    ];
    for (context, src, fragment) in cases {
        custom_error(&src, &[], "custom/ok", fragment, context);
        let disabled = format!("{src}{}", table("custom/ok", "level = \"off\"\n", ""));
        custom_error(
            &disabled,
            &[],
            "custom/ok",
            fragment,
            &format!("{context}, level off"),
        );
    }
    let callee = "(call_expression function: (identifier) @callee) @match";
    for message in ["calls {callee}", "literal {{braces}}"] {
        let src = custom_rule(&[("message", message), ("query", callee)]);
        accepted_run(&src, &[], Strictness::Standard, message);
    }
}

#[test]
fn audit_custom_rule_tables_accept_only_a_valid_level() {
    let spec = custom_rule(&[]);
    let cases = [
        ("warn_at = 1\n", LEVEL_ONLY),
        ("bogus = true\n", LEVEL_ONLY),
        ("level = \"fatal\"\n", "invalid level"),
    ];
    for (body, fragment) in cases {
        let (_tmp, root) = project(&format!("{spec}{}", table("custom/ok", "", body)), &[]);
        let error = config_error(run_at(&root, Strictness::Standard), body);
        rule_option(&error, "custom/ok", fragment, body);
    }
}

#[test]
fn audit_custom_rule_files_reject_bad_content_naming_the_file() {
    let singular = custom_rule(&[]).replace("[[custom_rules]]", "[[custom_rule]]");
    let missing_fix = custom_rule(&[]).replace("fix = \"test fix\"\n", "");
    let typo_key = format!("{}severty = \"warn\"\n", custom_rule(&[]));
    let fatal = custom_rule(&[("severity", "fatal")]);
    let cases = [
        (
            "root key",
            "rules/team.toml",
            PROJECT_TABLE,
            "unknown field `project`",
        ),
        (
            "rule table",
            "rules.toml",
            RULES_TABLE,
            "unknown field `rules`",
        ),
        (
            "singular",
            "rules.toml",
            singular.as_str(),
            "unknown field `custom_rule`",
        ),
        (
            "missing field",
            "rules.toml",
            missing_fix.as_str(),
            "missing field `fix`",
        ),
        (
            "spec key",
            "rules.toml",
            typo_key.as_str(),
            "unknown field `severty`",
        ),
        ("severity", "rules.toml", fatal.as_str(), "fatal"),
        (
            "wrong type",
            "rules.toml",
            "custom_rules = \"x\"\n",
            "sequence",
        ),
    ];
    for (context, path, text, fragment) in cases {
        file_error(&[(path, text)], path, fragment, context);
    }
    let good = custom_rule(&[]);
    let pair = [("a.toml", good.as_str()), ("b.toml", PROJECT_TABLE)];
    file_error(
        &pair,
        "b.toml",
        "unknown field `project`",
        "second file is bad",
    );
    let valid = [("empty.toml", ""), ("ok.toml", good.as_str())];
    let config = files_config(&["empty.toml", "ok.toml"]);
    accepted_run(
        &config,
        &valid,
        Strictness::Standard,
        "empty and valid files",
    );
}
