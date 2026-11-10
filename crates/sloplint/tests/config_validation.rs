//! Configuration rejection boundaries: rule tables and custom-rule specs must
//! fail with descriptive errors whatever the strictness profile or rule level.

#[path = "config_validation/custom_rules.rs"]
mod custom_rules;
#[path = "config_validation/rule_tables.rs"]
mod rule_tables;

use std::path::{Path, PathBuf};

use sloplint::{Report, RunError, RunOptions, run};
use sloplint_core::config::{Mode, Strictness, parse_config};
use sloplint_core::{CATALOG, ConfigError, RuleFilter, RulePattern, RuleSet, Severity};

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

const SPAN: &str = "complexity/mutable-span";
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

const FLOORS: [(&str, &str, &str, bool); 12] = [
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
    ("agent/colocated-tests", "exclude", "[\"[\"]", true),
    ("agent/colocated-tests", "exclude", "[\"src/**\"]", false),
];

const WRONG_TYPES: [(&str, &str, &str); 7] = [
    (SPAN, "minor_at", "\"3\""),
    (SPAN, "minor_at", "-1"),
    (SPAN, "major_at", "1.5"),
    (SPAN, "minor_at", "true"),
    ("agent/unique-patch-anchors", "window", "\"high\""),
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
