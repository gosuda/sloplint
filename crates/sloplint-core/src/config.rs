//! The `sloplint.toml` configuration model: defaults, parsing, and validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::callee::CalleePattern;
use crate::finding::FailOn;
use crate::rules::CATALOG;
use crate::rules::custom::{CustomRuleSpec, validate_custom_rule_spec};

/// Every valid `project.languages` entry and `[languages.<key>]` table name.
pub const LANGUAGE_KEYS: [&str; 23] = [
    "rust",
    "typescript",
    "javascript",
    "python",
    "go",
    "java",
    "kotlin",
    "csharp",
    "c",
    "cpp",
    "objc",
    "php",
    "ruby",
    "bash",
    "powershell",
    "swift",
    "r",
    "scala",
    "dart",
    "zig",
    "ocaml",
    "odin",
    "starlark",
];

/// Maps a config key back to itself as a `'static` string when it names a language.
pub fn static_language_key(key: &str) -> Option<&'static str> {
    LANGUAGE_KEYS
        .iter()
        .find(|&&candidate| candidate == key)
        .copied()
}

/// Strictness profile that selects every rule's level and thresholds from one preset matrix.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Strictness {
    /// Gates only egregious complexity, giant files, and swallowed errors.
    Minimal,
    /// Adds precise, low-false-positive greppability and agent rules at warn.
    Basic,
    /// Enables every heuristic rule with moderate thresholds. The default.
    #[default]
    Standard,
    /// The full posture; twelve rules at deny.
    Strict,
}

impl Strictness {
    /// Config-file and CLI spelling of this profile.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Basic => "basic",
            Self::Standard => "standard",
            Self::Strict => "strict",
        }
    }
}

/// Gate mode selecting between absolute thresholds and a recorded baseline.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Fail when reported findings reach `fail_on`.
    #[default]
    Threshold,
    /// Fail only on findings absent from the baseline.
    Ratchet,
}

/// Project-wide settings under `[project]`.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectConfig {
    /// Profile selecting every rule preset.
    pub strictness: Strictness,
    /// Languages enabled for analysis, as config keys.
    pub languages: Vec<String>,
    /// Gate mode for `sloplint check`.
    pub mode: Mode,
    /// Baseline file path relative to the project root.
    pub baseline: String,
    /// Lowest reported severity that fails the run.
    pub fail_on: FailOn,
    /// Exclude globs; a file matching any one is never analyzed.
    pub exclude: Vec<String>,
    /// Globs appended to the exclude list without replacing it.
    pub extend_exclude: Vec<String>,
    /// Globs marking test files, which exempt rules skip.
    pub test_globs: Vec<String>,
    /// Files larger than this many bytes are skipped.
    pub max_file_bytes: u64,
    /// Globs naming extra files that hold `[[custom_rules]]` tables.
    pub custom_rule_files: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            strictness: Strictness::Standard,
            languages: owned(&LANGUAGE_KEYS),
            mode: Mode::Threshold,
            baseline: "sloplint-baseline.json".to_string(),
            fail_on: FailOn::Deny,
            exclude: split_csv(DEFAULT_EXCLUDE),
            extend_exclude: Vec::new(),
            test_globs: split_csv(DEFAULT_TEST_GLOBS),
            max_file_bytes: 2_097_152,
            custom_rule_files: Vec::new(),
        }
    }
}

/// Per-language settings under `[languages.<key>]`.
#[derive(Clone, Debug, Serialize)]
pub struct LanguageConfig {
    /// Callee patterns counted as log or error calls by the grep rules.
    pub log_callees: Vec<String>,
}

/// A fully resolved `sloplint.toml`: defaults merged with every user table.
#[derive(Clone, Debug, Serialize)]
pub struct Config {
    /// Project-wide settings.
    pub project: ProjectConfig,
    /// Per-language settings keyed by config key; every known key is present.
    pub languages: BTreeMap<String, LanguageConfig>,
    /// Per-rule override tables keyed by rule id.
    pub rules: BTreeMap<String, toml::Table>,
    /// Declarative custom query rules.
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

/// Why a config failed to load; display strings quote the offending input.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The file is not valid TOML or violates a table's shape.
    #[error("{origin}: {message}")]
    Toml { origin: String, message: String },
    /// A `[rules."<id>"]` key names no catalog or custom rule.
    #[error("unknown rule `{id}`; valid ids: {}", valid.join(", "))]
    UnknownRule { id: String, valid: Vec<String> },
    /// A rule option or language callee pattern is malformed.
    #[error("{rule}: {message}")]
    RuleOption { rule: String, message: String },
    /// A rule's thresholds are inverted or below one.
    #[error("{rule}: warn_at {warn_at} must be at least 1 and at most deny_at {deny_at}")]
    Threshold {
        rule: String,
        warn_at: u32,
        deny_at: u32,
    },
    /// A glob pattern does not compile.
    #[error("{key}: invalid glob `{glob}`: {message}")]
    Glob {
        key: String,
        glob: String,
        message: String,
    },
    /// A `[[custom_rules]]` entry violates its schema.
    #[error("custom rule `{id}`: {message}")]
    CustomRule { id: String, message: String },
    /// A language key is not one of the twenty-three known config keys.
    #[error("unknown language `{value}`")]
    UnknownLanguage { value: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    strictness: Option<Strictness>,
    languages: Option<Vec<String>>,
    mode: Option<Mode>,
    baseline: Option<String>,
    fail_on: Option<FailOn>,
    exclude: Option<Vec<String>>,
    extend_exclude: Option<Vec<String>>,
    test_globs: Option<Vec<String>>,
    max_file_bytes: Option<u64>,
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

/// Parses `sloplint.toml` text into a fully defaulted, validated [`Config`].
pub fn parse_config(toml_src: &str, origin: &str) -> Result<Config, ConfigError> {
    let raw: RawConfig = toml::from_str(toml_src).map_err(|error| ConfigError::Toml {
        origin: origin.to_string(),
        message: error.message().to_string(),
    })?;
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
    if let Some(strictness) = project.strictness {
        target.strictness = strictness;
    }
    if let Some(languages) = project.languages {
        for key in &languages {
            if static_language_key(key).is_none() {
                return Err(ConfigError::UnknownLanguage { value: key.clone() });
            }
        }
        target.languages = languages;
    }
    if let Some(mode) = project.mode {
        target.mode = mode;
    }
    if let Some(baseline) = project.baseline {
        target.baseline = baseline;
    }
    if let Some(fail_on) = project.fail_on {
        target.fail_on = fail_on;
    }
    if let Some(exclude) = project.exclude {
        target.exclude = exclude;
    }
    if let Some(extend_exclude) = project.extend_exclude {
        target.extend_exclude = extend_exclude;
    }
    if let Some(test_globs) = project.test_globs {
        target.test_globs = test_globs;
    }
    if let Some(max_file_bytes) = project.max_file_bytes {
        target.max_file_bytes = max_file_bytes;
    }
    if let Some(custom_rule_files) = project.custom_rule_files {
        target.custom_rule_files = custom_rule_files;
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
                        message: format!("invalid level {value}; use off, warn, or deny"),
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
    (
        "rust",
        "trace!,debug!,info!,warn!,error!,anyhow!,bail!,eyre!,panic!",
    ),
    (
        "typescript",
        "console.log,console.info,console.warn,console.error,console.debug,logger.trace,logger.debug,logger.info,logger.warn,logger.error,new *Error",
    ),
    (
        "javascript",
        "console.log,console.info,console.warn,console.error,console.debug,logger.trace,logger.debug,logger.info,logger.warn,logger.error,new *Error",
    ),
    (
        "python",
        "logging.debug,logging.info,logging.warning,logging.error,logging.exception,logging.critical,logger.debug,logger.info,logger.warning,logger.error,logger.exception,logger.critical,*Error,*Exception",
    ),
    (
        "go",
        "log.Print,log.Printf,log.Println,log.Fatal,log.Fatalf,log.Panicf,fmt.Errorf,errors.New,slog.Debug,slog.Info,slog.Warn,slog.Error",
    ),
    (
        "java",
        "log.trace,log.debug,log.info,log.warn,log.error,log.fatal,logger.trace,logger.debug,logger.info,logger.warn,logger.error,logger.fatal,LOG.trace,LOG.debug,LOG.info,LOG.warn,LOG.error,LOG.fatal,System.err.println,System.out.println,printStackTrace",
    ),
    (
        "kotlin",
        "log.trace,log.debug,log.info,log.warn,log.error,logger.trace,logger.debug,logger.info,logger.warn,logger.error,LOG.trace,LOG.debug,LOG.info,LOG.warn,LOG.error,println,System.err.println,printStackTrace",
    ),
    (
        "csharp",
        "Console.WriteLine,Console.Error.WriteLine,Debug.WriteLine,Trace.WriteLine,logger.LogTrace,logger.LogDebug,logger.LogInformation,logger.LogWarning,logger.LogError,logger.LogCritical,new *Exception",
    ),
    ("c", "printf,puts,perror,warn,warnx"),
    (
        "cpp",
        "printf,puts,perror,warn,warnx,spdlog::info,spdlog::warn,spdlog::error,spdlog::debug,spdlog::trace,spdlog::critical,*Error,*Exception,*_error",
    ),
    ("objc", "NSLog,NSLogv,printf,puts,perror,warn,warnx"),
    (
        "php",
        "error_log,trigger_error,syslog,logger.debug,logger.info,logger.warning,logger.error,logger.critical,Log::debug,Log::info,Log::warning,Log::error,new *Exception",
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
    ("r", "message,warning,print,cat,flog.*"),
    (
        "scala",
        "logger.trace,logger.debug,logger.info,logger.warn,logger.error,log.debug,log.info,log.warn,log.error,println,new *Exception,new *Error",
    ),
    (
        "dart",
        "print,debugPrint,log,logger.finest,logger.finer,logger.fine,logger.config,logger.info,logger.warning,logger.severe,logger.shout,logger.log,*Exception,*Error",
    ),
    (
        "zig",
        "std.log.err,std.log.warn,std.log.info,std.log.debug,log.err,log.warn,log.info,log.debug,panic!,compileError!",
    ),
    (
        "ocaml",
        "Logs.debug,Logs.info,Logs.warn,Logs.err,Logs.app,Printf.eprintf,Format.eprintf,prerr_endline,failwith,invalid_arg",
    ),
    (
        "odin",
        "log.debug,log.info,log.warn,log.error,log.fatal,log.panic,fmt.eprintln,fmt.eprintf,panic",
    ),
    ("starlark", "print,fail"),
];

const DEFAULT_EXCLUDE: &str = "**/node_modules/**,**/target/**,**/vendor/**,**/dist/**,**/build/**,**/.venv/**,**/venv/**,**/__pycache__/**,**/*.d.ts,**/.git/**,**/.sloplint/**";

const DEFAULT_TEST_GLOBS: &str = "**/tests/**,**/test/**,**/__tests__/**,**/testdata/**,**/benches/**,**/*_test.go,**/test_*.py,**/*_test.py,**/conftest.py,**/*.test.ts,**/*.test.tsx,**/*.spec.ts,**/*.spec.tsx,**/*_test.c,**/test_*.c,**/*_test.cc,**/*_test.cpp,**/*_test.cxx,**/*_unittest.cc,**/test_*.cc,**/test_*.cpp,**/*Tests.m,**/*Tests.cs,**/*Test.cs,**/*.Tests/**,**/*_test.dart,**/*Spec.scala,**/*Test.scala,**/*Suite.scala,**/*.test.js,**/*.test.jsx,**/*.test.mjs,**/*.test.cjs,**/*.spec.js,**/*.spec.jsx,**/*.spec.mjs,**/*.spec.cjs,**/*Test.java,**/*Tests.java,**/*Test.kt,**/*Tests.kt,**/Tests/**,**/*Tests.swift,**/*Test.php,**/*_spec.rb,**/*_test.rb,**/spec/**,**/*.bats,**/test_*.sh,**/*_test.sh,**/tests/**/*.sh,**/*.Tests.ps1,**/*.tests.ps1,**/tests/**/*.ps1";
