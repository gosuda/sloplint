//! Configuration rejection boundaries: rule tables and custom-rule specs must
//! fail with descriptive errors whatever the strictness profile or rule level.

use std::collections::BTreeSet;
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
    "level = \"minor\"\n",
    "level = \"major\"\n",
];

const NESTED: &str = "complexity/nested-mutation";
const PROJECT_TABLE: &str = "[project]\nstrictness = \"strict\"\n";
const RULES_TABLE: &str = "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n";

const CUT_CASES: [(u32, u32, u32, u32, bool); 8] = [
    (0, 2, 3, 4, false),
    (1, 1, 1, 1, true),
    (1, 2, 3, 4, true),
    (2, 1, 3, 4, false),
    (1, 3, 2, 4, false),
    (1, 2, 4, 3, false),
    (1, u32::MAX, u32::MAX, u32::MAX, true),
    (u32::MAX, u32::MAX, u32::MAX, u32::MAX, true),
];

const FLOORS: [(&str, &str, &str, bool); 21] = [
    ("agent/unique-patch-anchors", "window", "1", true),
    ("agent/unique-patch-anchors", "window", "2", false),
    ("agent/unique-patch-anchors", "min_repeats", "1", true),
    ("agent/unique-patch-anchors", "min_repeats", "2", false),
    ("grep/homonym-definitions", "min_files", "1", true),
    ("grep/homonym-definitions", "min_files", "2", false),
    ("slop/field-copy", "min_fields", "1", true),
    ("slop/field-copy", "min_fields", "2", false),
    ("grep/contiguous-log-literal", "min_len", "0", true),
    ("grep/contiguous-log-literal", "min_len", "1", false),
    ("slop/echo-comment", "min_similarity", "0.0", true),
    ("slop/echo-comment", "min_similarity", "1.5", true),
    ("slop/echo-comment", "min_similarity", "nan", true),
    ("slop/echo-comment", "min_similarity", "-0.0", true),
    ("slop/echo-comment", "min_similarity", "-1.0", true),
    ("slop/echo-comment", "min_similarity", "inf", true),
    ("slop/echo-comment", "min_similarity", "-inf", true),
    ("slop/echo-comment", "min_similarity", "0.5", false),
    ("slop/echo-comment", "min_similarity", "1.0", false),
    ("agent/colocated-tests", "exclude", "[\"[\"]", true),
    ("agent/colocated-tests", "exclude", "[\"src/**\"]", false),
];

const WRONG_TYPES: [(&str, &str, &str); 7] = [
    (NESTED, "minor_at", "\"3\""),
    (NESTED, "minor_at", "-1"),
    (NESTED, "major_at", "1.5"),
    (NESTED, "minor_at", "true"),
    ("slop/echo-comment", "min_similarity", "\"high\""),
    ("agent/colocated-tests", "exclude", "\"src/**\""),
    ("grep/homonym-definitions", "min_files", "[1]"),
];

const CUSTOM_DEFAULTS: [(&str, &str); 7] = [
    ("id", "custom/ok"),
    ("language", "rust"),
    ("severity", "minor"),
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

fn rule_option(error: &ConfigError, rule: &str, token: Option<&str>, context: &str) {
    let ConfigError::RuleOption {
        rule: named,
        message,
    } = error
    else {
        panic!("{context}: expected a rule option error, got: {error}");
    };
    assert_eq!(named, rule, "{context}: error names the wrong rule");
    if let Some(token) = token {
        assert!(
            message.contains(token),
            "{context}: `{message}` lacks `{token}`"
        );
    }
    assert!(
        error.to_string().contains(rule),
        "{context}: display omits the rule: {error}"
    );
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

fn custom_error(
    config: &str,
    files: &[(&str, &str)],
    id: &str,
    token: Option<&str>,
    context: &str,
) {
    let (_tmp, root) = project(config, files);
    let error = config_error(run_at(&root, Strictness::Standard), context);
    let ConfigError::CustomRule { id: named, message } = &error else {
        panic!("{context}: expected a custom rule error, got: {error}");
    };
    assert_eq!(named, id, "{context}: error names the wrong rule");
    if let Some(token) = token {
        assert!(
            message.contains(token),
            "{context}: `{message}` lacks `{token}`"
        );
    }
    assert!(
        error.to_string().contains(id),
        "{context}: display omits the rule: {error}"
    );
}

fn file_error(files: &[(&str, &str)], origin: &str, token: Option<&str>, context: &str) {
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
    if let Some(token) = token {
        assert!(
            message.contains(token),
            "{context}: `{message}` lacks `{token}`"
        );
    }
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
            options.contains_key("minor_at")
                && options.contains_key("major_at")
                && options.contains_key("critical_at")
                && options.contains_key("blocker_at")
        })
        .map(|info| info.id)
        .collect();
    assert!(
        rules.len() >= 9 && rules.contains(NESTED),
        "threshold rules vanished: {rules:?}"
    );
    for rule in rules {
        for (minor_at, major_at, critical_at, blocker_at, valid) in CUT_CASES {
            let body = format!(
                "minor_at = {minor_at}\nmajor_at = {major_at}\ncritical_at = {critical_at}\nblocker_at = {blocker_at}\n"
            );
            for (level, strictness) in level_profiles(&PROFILES) {
                let src = table(rule, level, &body);
                let context = format!(
                    "{rule} {minor_at}/{major_at}/{critical_at}/{blocker_at} {level:?} {strictness:?}"
                );
                if valid {
                    accepted(&src, strictness, &context);
                    continue;
                }
                let error = rejected(&src, strictness, &context);
                rule_option(&error, rule, None, &context);
            }
        }
    }
}

#[test]
fn audit_config_partial_threshold_overrides_validate_against_the_preset() {
    for (level, strictness) in level_profiles(&PROFILES) {
        let minor = preset_int(NESTED, strictness, "minor_at");
        let major = preset_int(NESTED, strictness, "major_at");
        let critical = preset_int(NESTED, strictness, "critical_at");
        let blocker = preset_int(NESTED, strictness, "blocker_at");
        let context =
            format!("preset {minor}/{major}/{critical}/{blocker} {level:?} {strictness:?}");
        let high = table(NESTED, level, &format!("minor_at = {}\n", major + 1));
        let error = rejected(&high, strictness, &context);
        rule_option(&error, NESTED, None, &context);
        let low = table(NESTED, level, &format!("major_at = {}\n", minor - 1));
        let error = rejected(&low, strictness, &context);
        rule_option(&error, NESTED, None, &context);
        let descending = table(
            NESTED,
            level,
            &format!("critical_at = {minor}\nmajor_at = {critical}\n"),
        );
        let error = rejected(&descending, strictness, &context);
        rule_option(&error, NESTED, None, &context);
        let zero = table(NESTED, level, "blocker_at = 0\n");
        let error = rejected(&zero, strictness, &context);
        rule_option(&error, NESTED, None, &context);
        let at_major = table(NESTED, level, &format!("minor_at = {major}\n"));
        accepted(&at_major, strictness, &context);
        let at_minor = table(NESTED, level, &format!("major_at = {minor}\n"));
        accepted(&at_minor, strictness, &context);
    }
}

#[test]
fn audit_config_numeric_floors_are_enforced_for_disabled_rules() {
    for (rule, key, literal, is_rejected) in FLOORS {
        for (level, strictness) in level_profiles(&PROFILES) {
            let src = table(rule, level, &format!("{key} = {literal}\n"));
            let context = format!("{rule} {key}={literal} {level:?} {strictness:?}");
            if !is_rejected {
                accepted(&src, strictness, &context);
                continue;
            }
            rule_option(&rejected(&src, strictness, &context), rule, None, &context);
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
            rule_option(&error, info.id, Some("not_an_option"), &context);
        }
    }
    let meta = table("meta/parse-error", "level = \"off\"\n", "bogus = 1\n");
    let error = rejected(&meta, Strictness::Strict, "meta rule option");
    rule_option(
        &error,
        "meta/parse-error",
        Some("bogus"),
        "meta rule option",
    );
    let typo = table(NESTED, "level = \"off\"\n", "minor_att = 2\n");
    let error = rejected(&typo, Strictness::Minimal, "misspelled key");
    rule_option(&error, NESTED, Some("minor_att"), "misspelled key");
}

#[test]
fn audit_config_wrong_type_option_values_name_the_key() {
    for (rule, key, literal) in WRONG_TYPES {
        for (level, strictness) in level_profiles(&[Strictness::Minimal, Strictness::Strict]) {
            let src = table(rule, level, &format!("{key} = {literal}\n"));
            let context = format!("{rule} {key}={literal} {level:?} {strictness:?}");
            let error = rejected(&src, strictness, &context);
            rule_option(&error, rule, Some(key), &context);
        }
    }
}

#[test]
fn audit_config_invalid_level_values_name_the_value() {
    for literal in ["\"fatal\"", "\"warn\"", "\"deny\"", "\"Warn\"", "3", "true"] {
        let src = format!("[rules.\"{NESTED}\"]\nlevel = {literal}\n");
        let error = parse_config(&src, "sloplint.toml").err();
        let error = error.unwrap_or_else(|| panic!("level = {literal} was accepted"));
        rule_option(&error, NESTED, Some(literal.trim_matches('"')), literal);
    }
}

#[test]
fn audit_config_bad_rule_tables_fail_through_run_with_exit_two() {
    let cases = [
        (
            NESTED,
            "minor_at = 9\nmajor_at = 1\ncritical_at = 2\nblocker_at = 3\n",
        ),
        (
            NESTED,
            "minor_at = 0\nmajor_at = 4\ncritical_at = 5\nblocker_at = 6\n",
        ),
        (NESTED, "bogus = 1\n"),
        ("meta/parse-error", "bogus = 1\n"),
        ("grep/homonym-definitions", "min_files = 1\n"),
        ("agent/colocated-tests", "exclude = [\"[\"]\n"),
    ];
    for (rule, body) in cases {
        for level in ["", "level = \"off\"\n"] {
            let (_tmp, root) = project(&table(rule, level, body), &[]);
            for strictness in [Strictness::Minimal, Strictness::Strict] {
                let context = format!("{rule} {body:?} {level:?} {strictness:?}");
                let error = config_error(run_at(&root, strictness), &context);
                rule_option(&error, rule, None, &context);
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
        rule_option(&error, "grep/homonym-definitions", None, &context);
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
        [Severity::Major],
        "preset reports major for depth 8"
    );
    let minor = table(rule, "level = \"minor\"\n", "");
    assert_eq!(
        nesting_severities(&minor),
        [Severity::Minor],
        "level minor caps major"
    );
    let major = table(rule, "level = \"major\"\n", "");
    assert_eq!(
        nesting_severities(&major),
        [Severity::Major],
        "level major keeps major"
    );
    let off = table(rule, "level = \"off\"\n", "");
    assert!(
        nesting_severities(&off).is_empty(),
        "level off reports nothing"
    );
    let raised = table(
        rule,
        "",
        "minor_at = 9\nmajor_at = 9\ncritical_at = 9\nblocker_at = 9\n",
    );
    assert!(
        nesting_severities(&raised).is_empty(),
        "thresholds above depth 8 pass"
    );
    let both = format!(
        "{off}{}",
        table(NESTED, "level = \"off\"\n", "minor_at = 3\n")
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
        "off nested-mutation with minor_at override must stay inactive"
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
        custom_error(&custom_rule(&[("id", bad)]), &[], bad, None, bad);
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
    let inline = format!("{}{}", custom_rule(&id), custom_rule(&id));
    custom_error(&inline, &[], "custom/dup", None, "inline twice");
    let dup = custom_rule(&id);
    let mixed = format!("{}{dup}", files_config(&["extra.toml"]));
    let extra = [("extra.toml", dup.as_str())];
    custom_error(&mixed, &extra, "custom/dup", None, "inline and file");
    let pair = [("a.toml", dup.as_str()), ("b.toml", dup.as_str())];
    let config = files_config(&["a.toml", "b.toml"]);
    custom_error(&config, &pair, "custom/dup", None, "two files");
}

#[test]
fn audit_custom_rule_fields_must_not_be_blank() {
    for field in ["message", "why", "fix", "query"] {
        for blank in ["", " ", "\t", " \n "] {
            let src = custom_rule(&[(field, blank)]);
            custom_error(
                &src,
                &[],
                "custom/ok",
                Some(field),
                &format!("{field}={blank:?}"),
            );
        }
    }
    let blank = custom_rule(&[("message", " ")]);
    let file = [("rules.toml", blank.as_str())];
    let config = files_config(&["rules.toml"]);
    custom_error(
        &config,
        &file,
        "custom/ok",
        Some("message"),
        "blank message in a file",
    );
}

#[test]
fn audit_custom_rule_language_and_globs_must_be_valid() {
    for language in ["cobol", "Rust", "rust ", ""] {
        let src = custom_rule(&[("language", language)]);
        let token = (!language.trim().is_empty()).then(|| language.trim());
        custom_error(&src, &[], "custom/ok", token, language);
    }
    for (key, glob) in [("include", "["), ("exclude", "{a")] {
        let src = format!("{}{key} = [{glob:?}]\n", custom_rule(&[]));
        custom_error(&src, &[], "custom/ok", Some(glob), key);
    }
}

#[test]
fn audit_custom_rule_queries_must_compile_with_a_match_capture() {
    let cases = [
        (
            "unbalanced",
            custom_rule(&[("query", "(call_expression @match")]),
        ),
        (
            "unknown node",
            custom_rule(&[("query", "(no_such_node) @match")]),
        ),
        ("wrong grammar", custom_rule(&[("language", "python")])),
        (
            "no capture",
            custom_rule(&[("query", "(call_expression) @call")]),
        ),
        ("placeholder", custom_rule(&[("message", "found {nope}")])),
    ];
    for (context, src) in cases {
        custom_error(&src, &[], "custom/ok", None, context);
        let disabled = format!("{src}{}", table("custom/ok", "level = \"off\"\n", ""));
        custom_error(
            &disabled,
            &[],
            "custom/ok",
            None,
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
        ("minor_at = 1\n", None),
        ("bogus = true\n", None),
        ("level = \"fatal\"\n", Some("fatal")),
    ];
    for (body, token) in cases {
        let (_tmp, root) = project(&format!("{spec}{}", table("custom/ok", "", body)), &[]);
        let error = config_error(run_at(&root, Strictness::Standard), body);
        rule_option(&error, "custom/ok", token, body);
    }
}

#[test]
fn audit_config_rejects_legacy_severity_words() {
    for (src, token) in [
        (table(NESTED, "level = \"warn\"\n", ""), "warn"),
        (table(NESTED, "level = \"deny\"\n", ""), "deny"),
        (table(NESTED, "", "warn_at = 2\n"), "warn_at"),
        (table(NESTED, "", "deny_at = 2\n"), "deny_at"),
    ] {
        let error = rejected(&src, Strictness::Standard, &src);
        rule_option(&error, NESTED, Some(token), &src);
    }
    for (src, token) in [
        (custom_rule(&[("severity", "warn")]), "warn"),
        (custom_rule(&[("severity", "deny")]), "deny"),
    ] {
        let error = rejected(&src, Strictness::Standard, &src);
        let ConfigError::Toml { message, .. } = &error else {
            panic!("{token}: expected a TOML error, got: {error}");
        };
        assert!(
            message.contains(token),
            "{token}: `{message}` lacks `{token}`"
        );
        assert!(
            error.to_string().contains(token),
            "{token}: display omits `{token}`: {error}"
        );
    }
}

#[test]
fn audit_custom_rule_files_reject_bad_content_naming_the_file() {
    let singular = custom_rule(&[]).replace("[[custom_rules]]", "[[custom_rule]]");
    let missing_fix = custom_rule(&[]).replace("fix = \"test fix\"\n", "");
    let typo_key = format!("{}severty = \"minor\"\n", custom_rule(&[]));
    let fatal = custom_rule(&[("severity", "fatal")]);
    let cases = [
        (
            "root key",
            "rules/team.toml",
            PROJECT_TABLE,
            Some("project"),
        ),
        ("rule table", "rules.toml", RULES_TABLE, Some("rules")),
        (
            "singular",
            "rules.toml",
            singular.as_str(),
            Some("custom_rule"),
        ),
        (
            "missing field",
            "rules.toml",
            missing_fix.as_str(),
            Some("fix"),
        ),
        ("spec key", "rules.toml", typo_key.as_str(), Some("severty")),
        ("severity", "rules.toml", fatal.as_str(), Some("fatal")),
        ("wrong type", "rules.toml", "custom_rules = \"x\"\n", None),
    ];
    for (context, path, text, token) in cases {
        file_error(&[(path, text)], path, token, context);
    }
    let good = custom_rule(&[]);
    let pair = [("a.toml", good.as_str()), ("b.toml", PROJECT_TABLE)];
    file_error(&pair, "b.toml", Some("project"), "second file is bad");
    let valid = [("empty.toml", ""), ("ok.toml", good.as_str())];
    let config = files_config(&["empty.toml", "ok.toml"]);
    accepted_run(
        &config,
        &valid,
        Strictness::Standard,
        "empty and valid files",
    );
}
