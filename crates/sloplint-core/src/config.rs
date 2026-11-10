use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::callee::CalleePattern;
use crate::finding::{Confidence, FailOn, Severity};
use crate::lang::ALL_CONFIG_KEYS;
use crate::rules::CATALOG;
use crate::rules::custom::{CustomRuleSpec, validate_custom_rule_spec};

pub fn static_language_key(key: &str) -> Option<&'static str> {
    ALL_CONFIG_KEYS
        .iter()
        .find(|&&candidate| candidate == key)
        .copied()
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Strictness {
    Minimal,
    Basic,
    #[default]
    Standard,
    Strict,
}

impl Strictness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Basic => "basic",
            Self::Standard => "standard",
            Self::Strict => "strict",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Threshold,
    Ratchet,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectConfig {
    pub strictness: Strictness,
    pub languages: Vec<String>,
    pub mode: Mode,
    pub baseline: String,
    pub fail_on: FailOn,
    pub min_confidence: Confidence,
    pub exclude: Vec<String>,
    pub extend_exclude: Vec<String>,
    pub test_globs: Vec<String>,
    pub bench_globs: Vec<String>,
    pub example_globs: Vec<String>,
    pub support_globs: Vec<String>,
    pub entry_globs: Vec<String>,
    pub data_globs: Vec<String>,
    /// TOML values must fit `0..=u32::MAX` to match parser byte positions.
    pub max_file_bytes: u64,
    pub custom_rule_files: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            strictness: Strictness::Standard,
            languages: owned(ALL_CONFIG_KEYS),
            mode: Mode::Threshold,
            baseline: "sloplint-baseline.json".to_string(),
            fail_on: FailOn::At(Severity::Major),
            min_confidence: Confidence::Low,
            exclude: split_csv(DEFAULT_EXCLUDE),
            extend_exclude: Vec::new(),
            test_globs: split_csv(DEFAULT_TEST_GLOBS),
            bench_globs: split_csv(DEFAULT_BENCH_GLOBS),
            example_globs: split_csv(DEFAULT_EXAMPLE_GLOBS),
            support_globs: split_csv(DEFAULT_SUPPORT_GLOBS),
            entry_globs: split_csv(DEFAULT_ENTRY_GLOBS),
            data_globs: split_csv(DEFAULT_DATA_GLOBS),
            max_file_bytes: 2_097_152,
            custom_rule_files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LanguageConfig {
    pub log_callees: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub project: ProjectConfig,
    /// Always holds every known config key.
    pub languages: BTreeMap<String, LanguageConfig>,
    pub rules: BTreeMap<String, toml::Table>,
    pub custom_rules: Vec<CustomRuleSpec>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project: ProjectConfig::default(),
            languages: default_languages(),
            rules: BTreeMap::new(),
            custom_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{origin}: {message}")]
    Toml { origin: String, message: String },
    #[error("unknown rule `{id}`; valid ids: {}", valid.join(", "))]
    UnknownRule { id: String, valid: Vec<String> },
    #[error("{rule}: {message}")]
    RuleOption { rule: String, message: String },
    #[error("{key}: invalid glob `{glob}`: {message}")]
    Glob {
        key: String,
        glob: String,
        message: String,
    },
    #[error("custom rule `{id}`: {message}")]
    CustomRule { id: String, message: String },
    #[error("unknown language `{value}`")]
    UnknownLanguage { value: String },
}

impl ConfigError {
    pub fn from_toml(origin: &str, source: &str, error: &toml::de::Error) -> ConfigError {
        let message = match error.span() {
            Some(span) => {
                let (line, column) = line_column(source, span.start);
                format!("line {line}, column {column}: {}", error.message())
            }
            None => error.message().to_string(),
        };
        ConfigError::Toml {
            origin: origin.to_string(),
            message,
        }
    }
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let before = &source.as_bytes()[..offset.min(source.len())];
    let line = before.iter().filter(|&&byte| byte == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |newline| newline + 1);
    let column = String::from_utf8_lossy(&before[line_start..])
        .chars()
        .count()
        + 1;
    (line, column)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    strictness: Option<Strictness>,
    languages: Option<Vec<String>>,
    mode: Option<Mode>,
    baseline: Option<String>,
    fail_on: Option<FailOn>,
    min_confidence: Option<Confidence>,
    exclude: Option<Vec<String>>,
    extend_exclude: Option<Vec<String>>,
    test_globs: Option<Vec<String>>,
    bench_globs: Option<Vec<String>>,
    example_globs: Option<Vec<String>>,
    support_globs: Option<Vec<String>>,
    entry_globs: Option<Vec<String>>,
    data_globs: Option<Vec<String>>,
    max_file_bytes: Option<u32>,
    custom_rule_files: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLanguage {
    log_callees: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    project: Option<RawProject>,
    languages: Option<BTreeMap<String, RawLanguage>>,
    rules: Option<BTreeMap<String, toml::Table>>,
    custom_rules: Option<Vec<CustomRuleSpec>>,
}

pub fn parse_config(toml_src: &str, origin: &str) -> Result<Config, ConfigError> {
    let raw: RawConfig = toml::from_str(toml_src)
        .map_err(|error| ConfigError::from_toml(origin, toml_src, &error))?;
    let mut config = Config::default();
    if let Some(project) = raw.project {
        merge_project(&mut config, project)?;
    }
    if let Some(languages) = raw.languages {
        merge_languages(&mut config, languages)?;
    }
    if let Some(rules) = raw.rules {
        config.rules = rules;
    }
    if let Some(custom_rules) = raw.custom_rules {
        config.custom_rules = custom_rules;
    }
    validate(&config)?;
    Ok(config)
}

fn merge_project(config: &mut Config, project: RawProject) -> Result<(), ConfigError> {
    let target = &mut config.project;
    overwrite(&mut target.strictness, project.strictness);
    if let Some(languages) = project.languages {
        validate_language_keys(&languages)?;
        target.languages = languages;
    }
    overwrite(&mut target.mode, project.mode);
    overwrite(&mut target.baseline, project.baseline);
    overwrite(&mut target.fail_on, project.fail_on);
    overwrite(&mut target.min_confidence, project.min_confidence);
    overwrite(&mut target.exclude, project.exclude);
    overwrite(&mut target.extend_exclude, project.extend_exclude);
    overwrite(&mut target.test_globs, project.test_globs);
    overwrite(&mut target.bench_globs, project.bench_globs);
    overwrite(&mut target.example_globs, project.example_globs);
    overwrite(&mut target.support_globs, project.support_globs);
    overwrite(&mut target.entry_globs, project.entry_globs);
    overwrite(&mut target.data_globs, project.data_globs);
    overwrite(
        &mut target.max_file_bytes,
        project.max_file_bytes.map(u64::from),
    );
    overwrite(&mut target.custom_rule_files, project.custom_rule_files);
    Ok(())
}

fn overwrite<T>(target: &mut T, source: Option<T>) {
    if let Some(value) = source {
        *target = value;
    }
}

fn validate_language_keys(languages: &[String]) -> Result<(), ConfigError> {
    for key in languages {
        if static_language_key(key).is_none() {
            return Err(ConfigError::UnknownLanguage { value: key.clone() });
        }
    }
    Ok(())
}

fn merge_languages(
    config: &mut Config,
    raw: BTreeMap<String, RawLanguage>,
) -> Result<(), ConfigError> {
    for (key, language) in raw {
        let Some(static_key) = static_language_key(&key) else {
            return Err(ConfigError::UnknownLanguage { value: key });
        };
        if let Some(log_callees) = language.log_callees {
            config
                .languages
                .insert(static_key.to_string(), LanguageConfig { log_callees });
        }
    }
    Ok(())
}

fn validate(config: &Config) -> Result<(), ConfigError> {
    validate_globs(config)?;
    validate_log_callees(config)?;
    validate_rules_table(config)?;
    validate_custom_rules(config)
}

fn validate_globs(config: &Config) -> Result<(), ConfigError> {
    let lists = [
        ("project.exclude", &config.project.exclude),
        ("project.extend_exclude", &config.project.extend_exclude),
        ("project.test_globs", &config.project.test_globs),
        ("project.bench_globs", &config.project.bench_globs),
        ("project.example_globs", &config.project.example_globs),
        ("project.support_globs", &config.project.support_globs),
        ("project.entry_globs", &config.project.entry_globs),
        ("project.data_globs", &config.project.data_globs),
        (
            "project.custom_rule_files",
            &config.project.custom_rule_files,
        ),
    ];
    for (key, patterns) in lists {
        for pattern in patterns {
            globset::Glob::new(pattern).map_err(|error| ConfigError::Glob {
                key: key.to_string(),
                glob: pattern.clone(),
                message: error.to_string(),
            })?;
        }
    }
    Ok(())
}

fn validate_log_callees(config: &Config) -> Result<(), ConfigError> {
    for (key, language) in &config.languages {
        for pattern in &language.log_callees {
            CalleePattern::parse(pattern).map_err(|message| ConfigError::RuleOption {
                rule: format!("languages.{key}"),
                message,
            })?;
        }
    }
    Ok(())
}

fn validate_rules_table(config: &Config) -> Result<(), ConfigError> {
    let mut valid: Vec<String> = CATALOG.iter().map(|info| info.id.to_string()).collect();
    valid.extend(config.custom_rules.iter().map(|spec| spec.id.clone()));
    valid.sort();
    for (id, table) in &config.rules {
        if valid.binary_search(id).is_err() {
            return Err(ConfigError::UnknownRule {
                id: id.clone(),
                valid: valid.clone(),
            });
        }
        let custom = id.starts_with("custom/");
        for (key, value) in table {
            if key == "level" {
                if value.as_str().and_then(crate::rules::parse_level).is_none() {
                    return Err(ConfigError::RuleOption {
                        rule: id.clone(),
                        message: format!(
                            "invalid level {value}; use off, info, minor, major, critical, or blocker"
                        ),
                    });
                }
            } else if custom {
                return Err(ConfigError::RuleOption {
                    rule: id.clone(),
                    message: "custom rules accept only the `level` key".to_string(),
                });
            }
        }
    }
    Ok(())
}

fn validate_custom_rules(config: &Config) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for spec in &config.custom_rules {
        validate_custom_rule_spec(spec, &mut seen).map_err(|message| ConfigError::CustomRule {
            id: spec.id.clone(),
            message,
        })?;
    }
    Ok(())
}

fn owned(patterns: &[&str]) -> Vec<String> {
    patterns
        .iter()
        .map(|pattern| (*pattern).to_string())
        .collect()
}

fn split_csv(csv: &str) -> Vec<String> {
    csv.split(',').map(str::to_string).collect()
}

fn language_config(callees: &str) -> LanguageConfig {
    LanguageConfig {
        log_callees: split_csv(callees),
    }
}

fn default_languages() -> BTreeMap<String, LanguageConfig> {
    DEFAULT_LOG_CALLEES
        .into_iter()
        .map(|(key, callees)| (key.to_string(), language_config(callees)))
        .collect()
}

const DEFAULT_LOG_CALLEES: [(&str, &str); 23] = [
    ("rust", "trace!,debug!,info!,warn!,error!"),
    (
        "typescript",
        "console.log,console.info,console.warn,console.error,console.debug,logger.trace,logger.debug,logger.info,logger.warn,logger.error",
    ),
    (
        "javascript",
        "console.log,console.info,console.warn,console.error,console.debug,logger.trace,logger.debug,logger.info,logger.warn,logger.error",
    ),
    (
        "python",
        "logging.debug,logging.info,logging.warning,logging.error,logging.exception,logging.critical,logger.debug,logger.info,logger.warning,logger.error,logger.exception,logger.critical",
    ),
    (
        "go",
        "log.Print,log.Printf,log.Println,log.Fatal,log.Fatalf,log.Panicf,slog.Debug,slog.Info,slog.Warn,slog.Error",
    ),
    (
        "java",
        "log.trace,log.debug,log.info,log.warn,log.error,log.fatal,logger.trace,logger.debug,logger.info,logger.warn,logger.error,logger.fatal,LOG.trace,LOG.debug,LOG.info,LOG.warn,LOG.error,LOG.fatal",
    ),
    (
        "kotlin",
        "log.trace,log.debug,log.info,log.warn,log.error,logger.trace,logger.debug,logger.info,logger.warn,logger.error,LOG.trace,LOG.debug,LOG.info,LOG.warn,LOG.error",
    ),
    (
        "csharp",
        "Console.WriteLine,Console.Error.WriteLine,Debug.WriteLine,Trace.WriteLine,logger.LogTrace,logger.LogDebug,logger.LogInformation,logger.LogWarning,logger.LogError,logger.LogCritical",
    ),
    ("c", "printf,puts,perror,warn,warnx"),
    (
        "cpp",
        "printf,puts,perror,warn,warnx,spdlog::info,spdlog::warn,spdlog::error,spdlog::debug,spdlog::trace,spdlog::critical",
    ),
    ("objc", "NSLog,NSLogv,printf,puts,perror,warn,warnx"),
    (
        "php",
        "error_log,trigger_error,syslog,logger.debug,logger.info,logger.warning,logger.error,logger.critical,Log::debug,Log::info,Log::warning,Log::error",
    ),
    (
        "ruby",
        "logger.trace,logger.debug,logger.info,logger.warn,logger.error,logger.fatal,Rails.logger.debug,Rails.logger.info,Rails.logger.warn,Rails.logger.error,warn,puts",
    ),
    ("bash", "echo,printf,logger,warn,error,die,log"),
    (
        "powershell",
        "Write-Host,Write-Output,Write-Error,Write-Warning,Write-Verbose,Write-Debug,Write-Information",
    ),
    (
        "swift",
        "print,debugPrint,NSLog,os_log,logger.debug,logger.info,logger.notice,logger.warning,logger.error,logger.fault",
    ),
    ("r", "message,warning,print,cat"),
    (
        "scala",
        "logger.trace,logger.debug,logger.info,logger.warn,logger.error,log.debug,log.info,log.warn,log.error,println",
    ),
    (
        "dart",
        "print,debugPrint,log,logger.finest,logger.finer,logger.fine,logger.config,logger.info,logger.warning,logger.severe,logger.shout,logger.log",
    ),
    (
        "zig",
        "std.log.err,std.log.warn,std.log.info,std.log.debug,log.err,log.warn,log.info,log.debug",
    ),
    (
        "ocaml",
        "Logs.debug,Logs.info,Logs.warn,Logs.err,Logs.app,Printf.eprintf,Format.eprintf,prerr_endline",
    ),
    (
        "odin",
        "log.debug,log.info,log.warn,log.error,log.fatal,log.panic,fmt.eprintln,fmt.eprintf",
    ),
    ("starlark", "print"),
];

const DEFAULT_EXCLUDE: &str = "**/node_modules/**,**/target/**,**/vendor/**,**/dist/**,**/build/**,**/.venv/**,**/venv/**,**/__pycache__/**,**/*.d.ts,**/.git/**,**/.sloplint/**";

const DEFAULT_TEST_GLOBS: &str = "**/tests/**,**/test/**,**/__tests__/**,**/*_test.go,**/test_*.py,**/*_test.py,**/*.test.ts,**/*.test.tsx,**/*.spec.ts,**/*.spec.tsx,**/*_test.c,**/test_*.c,**/*_test.cc,**/*_test.cpp,**/*_test.cxx,**/*_unittest.cc,**/test_*.cc,**/test_*.cpp,**/*Tests.m,**/*Tests.cs,**/*Test.cs,**/*.Tests/**,**/*_test.dart,**/*Spec.scala,**/*Test.scala,**/*Suite.scala,**/*.test.js,**/*.test.jsx,**/*.test.mjs,**/*.test.cjs,**/*.spec.js,**/*.spec.jsx,**/*.spec.mjs,**/*.spec.cjs,**/*Test.java,**/*Tests.java,**/*Test.kt,**/*Tests.kt,**/Tests/**,**/*Tests.swift,**/*Test.php,**/*_spec.rb,**/*_test.rb,**/spec/**,**/*.bats,**/test_*.sh,**/*_test.sh,**/tests/**/*.sh,**/*.Tests.ps1,**/*.tests.ps1,**/tests/**/*.ps1";

const DEFAULT_BENCH_GLOBS: &str = "**/benches/**,**/bench/**,**/benchmarks/**,**/benchmark/**,**/fuzz/**,**/fuzz_targets/**,**/*_bench.rs,**/*.bench.ts,**/*.bench.tsx,**/*.bench.js,**/*.bench.mjs,**/*_bench.py,**/bench_*.py,**/*_benchmark.cc,**/*_benchmark.cpp,**/*_bench.cc,**/*_bench.cpp,**/*_fuzzer.cc,**/*_fuzzer.cpp,**/src/jmh/**";

const DEFAULT_EXAMPLE_GLOBS: &str = "**/examples/**,**/example/**,**/_examples/**";

const DEFAULT_SUPPORT_GLOBS: &str = "**/conftest.py,**/testutil/**,**/testutils/**,**/test_util/**,**/test_utils/**,**/test_helpers/**,**/test_support/**,**/testing/**,**/tests/common/**,**/tests/support/**,**/spec/support/**,**/__mocks__/**";

const DEFAULT_ENTRY_GLOBS: &str = "**/src/main.rs,**/src/bin/**,**/build.rs,**/main.go,**/cmd/**,**/__main__.py,**/manage.py,**/scripts/**";

const DEFAULT_DATA_GLOBS: &str =
    "**/testdata/**,**/test-data/**,**/__fixtures__/**,**/__snapshots__/**";

#[cfg(test)]
mod tests {
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
}
