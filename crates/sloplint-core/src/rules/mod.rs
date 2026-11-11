pub(crate) mod agent;
pub(crate) mod complexity;
pub mod custom;
pub(crate) mod grep;
pub mod meta;
mod ruleset;
pub(crate) mod slop;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::callee::CalleePattern;
use crate::config::{Config, ConfigError, Strictness, static_language_key};
use crate::facts::{FileFacts, Pos, WorldFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::RolePolicy;
use crate::suppress::RulePattern;

pub use self::custom::CustomRuleSpec;
pub use ruleset::RuleSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    Complexity,
    Grep,
    Slop,
    Agent,
    Meta,
    Custom,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complexity => "complexity",
            Self::Grep => "grep",
            Self::Slop => "slop",
            Self::Agent => "agent",
            Self::Meta => "meta",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuleInfo {
    pub id: &'static str,
    pub family: Family,
    pub summary: &'static str,
    pub why: &'static str,
    pub confidence: Confidence,
    pub roles: RolePolicy,
    pub preset: fn(Strictness) -> Preset,
    pub build: Build,
}

#[derive(Debug, Clone)]
pub struct Preset {
    pub level: Level,
    pub options: toml::Table,
}

pub type FileRuleBuilder = fn(toml::Table, &Settings) -> Result<Box<dyn FileRule>, String>;

pub type WorldRuleBuilder = fn(toml::Table) -> Result<Box<dyn WorldRule>, String>;

#[derive(Debug, Clone, Copy)]
pub enum Build {
    File(FileRuleBuilder),
    World(WorldRuleBuilder),
    Meta,
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub log_callees: BTreeMap<&'static str, Vec<CalleePattern>>,
}

impl Settings {
    pub fn from_config(config: &Config) -> Result<Settings, ConfigError> {
        let mut log_callees = BTreeMap::new();
        for (key, language) in &config.languages {
            if let Some(static_key) = static_language_key(key) {
                let mut patterns = Vec::with_capacity(language.log_callees.len());
                for pattern in &language.log_callees {
                    patterns.push(CalleePattern::parse(pattern).map_err(|message| {
                        ConfigError::RuleOption {
                            rule: format!("languages.{key}"),
                            message,
                        }
                    })?);
                }
                log_callees.insert(static_key, patterns);
            }
        }
        Ok(Settings { log_callees })
    }
}

pub trait FileRule: Send + Sync {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>);
}

pub trait WorldRule: Send + Sync {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>);
}

pub struct WorldFile<'a> {
    pub path: &'a str,
    pub facts: &'a WorldFacts,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyOptions {}

pub(crate) fn parse_options<T: DeserializeOwned>(table: toml::Table) -> Result<T, String> {
    toml::Value::Table(table)
        .try_into()
        .map_err(|error| error.to_string())
}

pub(crate) fn string_values(items: &[&str]) -> toml::Value {
    toml::Value::Array(
        items
            .iter()
            .map(|item| toml::Value::String((*item).to_string()))
            .collect(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub minor_at: u32,
    pub major_at: u32,
    pub critical_at: u32,
    pub blocker_at: u32,
}

impl Thresholds {
    pub fn new(
        minor_at: u32,
        major_at: u32,
        critical_at: u32,
        blocker_at: u32,
    ) -> Result<Thresholds, String> {
        for (key, value) in [
            ("minor_at", minor_at),
            ("major_at", major_at),
            ("critical_at", critical_at),
            ("blocker_at", blocker_at),
        ] {
            if value == 0 {
                return Err(format!(
                    "{key} {value} must be at least 1 (minor_at <= major_at <= critical_at <= blocker_at)"
                ));
            }
        }
        for (low_key, low, high_key, high) in [
            ("minor_at", minor_at, "major_at", major_at),
            ("major_at", major_at, "critical_at", critical_at),
            ("critical_at", critical_at, "blocker_at", blocker_at),
        ] {
            if low > high {
                return Err(format!("{low_key} {low} must not exceed {high_key} {high}"));
            }
        }
        Ok(Thresholds {
            minor_at,
            major_at,
            critical_at,
            blocker_at,
        })
    }

    pub fn grade(self, value: u32) -> Option<Severity> {
        if value >= self.blocker_at {
            Some(Severity::Blocker)
        } else if value >= self.critical_at {
            Some(Severity::Critical)
        } else if value >= self.major_at {
            Some(Severity::Major)
        } else if value >= self.minor_at {
            Some(Severity::Minor)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuleFilter(pub Vec<RulePattern>);

impl RuleFilter {
    pub fn matches(&self, rule_id: &str) -> bool {
        self.0.is_empty() || self.0.iter().any(|pattern| pattern.matches(rule_id))
    }
}

pub struct CustomHit<'s> {
    pub rule: u32,
    pub pos: Pos,
    pub captures: Vec<(&'s str, &'s str)>,
}

#[derive(Debug, Clone)]
pub struct ResolvedRule {
    pub info: &'static RuleInfo,
    pub level: Level,
    pub options: toml::Table,
}

pub static CATALOG: &[RuleInfo] = &[
    agent::ambient_global_state::INFO,
    agent::catch_all_match::INFO,
    agent::colocated_tests::INFO,
    agent::explicit_boundary_types::INFO,
    agent::file_token_budget::INFO,
    agent::informative_assertions::INFO,
    agent::stringly_api::INFO,
    agent::typed_error_boundary::INFO,
    agent::unique_patch_anchors::INFO,
    complexity::boolean_params::INFO,
    complexity::cognitive_depth::INFO,
    complexity::import_fan_out::INFO,
    complexity::mutable_span::INFO,
    complexity::nesting_depth::INFO,
    grep::contiguous_log_literal::INFO,
    grep::generic_name::INFO,
    grep::homonym_definitions::INFO,
    grep::multiline_argument::INFO,
    grep::no_dynamic_symbol::INFO,
    grep::no_rename_import::INFO,
    grep::no_wildcard_import::INFO,
    meta::CUSTOM_CAP,
    meta::INVALID_SUPPRESSION,
    meta::PARSE_ERROR,
    meta::UNUSED_SUPPRESSION,
    slop::barrel_file::INFO,
    slop::echo_comment::INFO,
    slop::field_copy::INFO,
    slop::passthrough_wrapper::INFO,
    slop::single_impl_abstraction::INFO,
    slop::zombie_catch::INFO,
];

pub(crate) fn parse_level(s: &str) -> Option<Level> {
    if s == "off" {
        return Some(Level::Off);
    }
    Severity::parse(s).map(Level::At)
}

pub(crate) fn threshold_options(
    minor_at: u32,
    major_at: u32,
    critical_at: u32,
    blocker_at: u32,
) -> toml::Table {
    let mut options = toml::Table::new();
    for (key, value) in [
        ("minor_at", minor_at),
        ("major_at", major_at),
        ("critical_at", critical_at),
        ("blocker_at", blocker_at),
    ] {
        options.insert(key.to_string(), toml::Value::Integer(i64::from(value)));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_rejects_invalid_callee() {
        let mut config = Config::default();
        config
            .languages
            .get_mut("rust")
            .unwrap_or_else(|| panic!("default config must contain rust"))
            .log_callees
            .push("new".to_string());
        let Err(ConfigError::RuleOption { rule, .. }) = Settings::from_config(&config) else {
            panic!("invalid callee must be rejected");
        };
        assert_eq!(
            rule, "languages.rust",
            "error must name the language: {rule}"
        );
    }

    #[test]
    fn from_config_accepts_defaults() {
        assert!(
            Settings::from_config(&Config::default()).is_ok(),
            "defaults must build settings"
        );
    }
}
