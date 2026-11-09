pub(crate) mod agent;
pub(crate) mod complexity;
pub mod custom;
pub(crate) mod grep;
pub mod meta;
pub(crate) mod slop;

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;

use crate::callee::CalleePattern;
use crate::config::{Config, ConfigError, Strictness, static_language_key};
use crate::facts::{FileFacts, Pos, WorldFacts};
use crate::finding::{Finding, Level, Severity};
use crate::suppress::RulePattern;

pub use self::custom::CustomRuleSpec;

/// Every rule id starts with its family prefix.
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
    pub exempt_tests: bool,
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
    pub fn from_config(config: &Config) -> Settings {
        let mut log_callees = BTreeMap::new();
        for (key, language) in &config.languages {
            if let Some(static_key) = static_language_key(key) {
                let patterns = language
                    .log_callees
                    .iter()
                    .filter_map(|pattern| CalleePattern::parse(pattern).ok())
                    .collect();
                log_callees.insert(static_key, patterns);
            }
        }
        Settings { log_callees }
    }
}

pub trait FileRule: Send + Sync {
    /// Rules only detect; the caller caps and filters.
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>);
}

/// `files` arrives sorted by path.
pub trait WorldRule: Send + Sync {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>);
}

pub struct WorldFile<'a> {
    /// Repo-relative, `/`-separated.
    pub path: &'a str,
    pub facts: &'a WorldFacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub warn_at: u32,
    pub deny_at: u32,
}

impl Thresholds {
    pub fn new(warn_at: u32, deny_at: u32) -> Result<Thresholds, String> {
        if warn_at == 0 || warn_at > deny_at {
            return Err(format!(
                "warn_at {warn_at} must be at least 1 and at most deny_at {deny_at}"
            ));
        }
        Ok(Thresholds { warn_at, deny_at })
    }

    pub fn grade(self, value: u32) -> Option<Severity> {
        if value >= self.deny_at {
            Some(Severity::Deny)
        } else if value >= self.warn_at {
            Some(Severity::Warn)
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
    complexity::nested_mutation::INFO,
    complexity::nesting_depth::INFO,
    grep::contiguous_log_literal::INFO,
    grep::generic_name::INFO,
    grep::homonym_definitions::INFO,
    grep::multiline_argument::INFO,
    grep::no_dynamic_symbol::INFO,
    grep::no_rename_import::INFO,
    grep::no_wildcard_import::INFO,
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
    match s {
        "off" => Some(Level::Off),
        "warn" => Some(Level::Warn),
        "deny" => Some(Level::Deny),
        _ => None,
    }
}

pub(crate) fn threshold_options(warn_at: u32, deny_at: u32) -> toml::Table {
    let mut options = toml::Table::new();
    options.insert(
        "warn_at".to_string(),
        toml::Value::Integer(i64::from(warn_at)),
    );
    options.insert(
        "deny_at".to_string(),
        toml::Value::Integer(i64::from(deny_at)),
    );
    options
}

pub struct RuleSet {
    file_rules: Vec<(&'static RuleInfo, Box<dyn FileRule>)>,
    world_rules: Vec<(&'static RuleInfo, Box<dyn WorldRule>)>,
    resolved: Vec<ResolvedRule>,
    levels: BTreeMap<String, Level>,
    graded: BTreeSet<String>,
    active: BTreeSet<String>,
    filter: RuleFilter,
    custom: Vec<custom::CustomEntry>,
}

impl RuleSet {
    pub fn validate_filter(config: &Config, filter: &RuleFilter) -> Result<(), ConfigError> {
        for pattern in &filter.0 {
            let known = CATALOG
                .iter()
                .map(|info| info.id)
                .chain(config.custom_rules.iter().map(|spec| spec.id.as_str()))
                .any(|id| pattern.matches(id));
            if !known {
                let id = match pattern {
                    RulePattern::Exact(id) => id.to_string(),
                    RulePattern::Family(family) => format!("{family}/*"),
                };
                let mut valid: Vec<String> =
                    CATALOG.iter().map(|info| info.id.to_string()).collect();
                valid.extend(config.custom_rules.iter().map(|spec| spec.id.clone()));
                valid.sort();
                valid.dedup();
                return Err(ConfigError::UnknownRule { id, valid });
            }
        }
        Ok(())
    }

    pub fn new(config: &Config, filter: &RuleFilter) -> Result<RuleSet, ConfigError> {
        Self::validate_filter(config, filter)?;
        let settings = Settings::from_config(config);
        let mut resolved = Vec::with_capacity(CATALOG.len());
        for info in CATALOG {
            let user = config.rules.get(info.id);
            let preset = (info.preset)(config.project.strictness);
            let level = user
                .and_then(|table| table.get("level"))
                .and_then(|value| value.as_str())
                .and_then(parse_level)
                .unwrap_or(preset.level);
            let options = merge_options(info, &preset, user)?;
            resolved.push(ResolvedRule {
                info,
                level,
                options,
            });
        }
        let levels: BTreeMap<String, Level> = resolved
            .iter()
            .map(|entry| (entry.info.id.to_string(), entry.level))
            .collect();
        let graded: BTreeSet<String> = resolved
            .iter()
            .filter(|entry| entry.options.contains_key("warn_at"))
            .map(|entry| entry.info.id.to_string())
            .collect();
        let mut file_rules = Vec::new();
        let mut world_rules = Vec::new();
        let mut active = BTreeSet::new();
        for entry in &resolved {
            let wanted = entry.level != Level::Off && filter.matches(entry.info.id);
            let overridden = config
                .rules
                .get(entry.info.id)
                .is_some_and(|table| table.keys().any(|key| key != "level"));
            if !wanted && !overridden {
                continue;
            }
            let option_err = |message: String| ConfigError::RuleOption {
                rule: entry.info.id.to_string(),
                message,
            };
            match entry.info.build {
                Build::File(build) => {
                    let rule = build(entry.options.clone(), &settings).map_err(option_err)?;
                    if wanted {
                        file_rules.push((entry.info, rule));
                    }
                }
                Build::World(build) => {
                    let rule = build(entry.options.clone()).map_err(option_err)?;
                    if wanted {
                        world_rules.push((entry.info, rule));
                    }
                }
                Build::Meta => {}
            }
            if wanted {
                active.insert(entry.info.id.to_string());
            }
        }
        let mut set = RuleSet {
            file_rules,
            world_rules,
            resolved,
            levels,
            graded,
            active,
            filter: filter.clone(),
            custom: custom::entries(config)?,
        };
        let custom_levels: Vec<(String, Level)> = set
            .custom
            .iter()
            .map(|entry| (entry.id.to_string(), entry.level))
            .collect();
        for (id, level) in custom_levels {
            set.levels.insert(id.clone(), level);
            if level != Level::Off && set.filter.matches(&id) {
                set.active.insert(id);
            }
        }
        Ok(set)
    }

    pub fn check_file(&self, facts: &FileFacts<'_>) -> Vec<Finding> {
        let mut out = Vec::new();
        for (info, rule) in &self.file_rules {
            if info.exempt_tests && facts.is_test_file {
                continue;
            }
            let mut found = Vec::new();
            rule.check(facts, &mut found);
            self.cap_rule_findings(info, &mut found);
            out.append(&mut found);
        }
        out
    }

    pub fn check_world(&self, files: &[WorldFile<'_>]) -> Vec<Finding> {
        self.world_rules
            .par_iter()
            .map(|(info, rule)| {
                let mut found = Vec::new();
                rule.check(files, &mut found);
                self.cap_rule_findings(info, &mut found);
                found
            })
            .collect::<Vec<Vec<Finding>>>()
            .into_iter()
            .flatten()
            .collect()
    }

    pub fn custom_findings(&self, facts: &FileFacts<'_>, hits: &[CustomHit<'_>]) -> Vec<Finding> {
        let mut out = Vec::new();
        for hit in hits {
            let Some(entry) = self.custom.get(hit.rule as usize) else {
                continue;
            };
            if !self.filter.matches(&entry.id) {
                continue;
            }
            if !entry.matches_path(facts.path) {
                continue;
            }
            let severity = match entry.level {
                Level::Off => continue,
                Level::Warn => Severity::Warn,
                Level::Deny => Severity::Deny,
            };
            let message = custom::interpolate(&entry.message, &hit.captures);
            out.push(Finding::at(
                facts,
                hit.pos,
                &entry.id,
                severity,
                message,
                entry.fix.clone(),
            ));
        }
        out
    }

    pub fn parse_error(&self, path: &str, source: &str, pos: Pos) -> Option<Finding> {
        if !self.is_active(meta::PARSE_ERROR.id) {
            return None;
        }
        let mut findings = [meta::parse_error_finding(path, source, pos)];
        self.cap_levels(&mut findings);
        let [finding] = findings;
        Some(finding)
    }

    pub fn unused_suppression(
        &self,
        path: &str,
        source: &str,
        line: u32,
        rules: &str,
    ) -> Option<Finding> {
        if !self.is_active(meta::UNUSED_SUPPRESSION.id) {
            return None;
        }
        let mut findings = [meta::unused_suppression_finding(path, source, line, rules)];
        self.cap_levels(&mut findings);
        let [finding] = findings;
        Some(finding)
    }

    fn cap_rule_findings(&self, info: &RuleInfo, found: &mut [Finding]) {
        let level = self.levels.get(info.id);
        cap_with_level(level, found);
        if !self.graded.contains(info.id) {
            raise_with_level(level, found);
        }
    }

    pub fn cap_levels(&self, findings: &mut [Finding]) {
        for finding in findings.iter_mut() {
            match self.levels.get(&*finding.rule).copied() {
                Some(Level::Warn) if finding.severity == Severity::Deny => {
                    finding.severity = Severity::Warn;
                }
                Some(Level::Deny) if finding.severity == Severity::Warn => {
                    finding.severity = Severity::Deny;
                }
                _ => {}
            }
        }
    }

    pub fn why(&self, rule: &str) -> Option<&str> {
        if let Some(info) = CATALOG.iter().find(|info| info.id == rule) {
            return Some(info.why);
        }
        self.custom
            .iter()
            .find(|entry| &*entry.id == rule)
            .map(|entry| entry.why.as_str())
    }

    pub fn is_known(&self, rule: &str) -> bool {
        CATALOG.iter().any(|info| info.id == rule)
            || self.custom.iter().any(|entry| &*entry.id == rule)
    }

    pub fn is_active(&self, rule: &str) -> bool {
        self.active.contains(rule)
    }

    pub fn resolved(&self) -> &[ResolvedRule] {
        &self.resolved
    }
}

fn merge_options(
    info: &RuleInfo,
    preset: &Preset,
    user: Option<&toml::Table>,
) -> Result<toml::Table, ConfigError> {
    let mut options = preset.options.clone();
    for (key, value) in user.into_iter().flatten() {
        if key == "level" {
            continue;
        }
        if !preset.options.contains_key(key) {
            return Err(unknown_option(info.id, key, &preset.options));
        }
        options.insert(key.clone(), value.clone());
    }
    Ok(options)
}

fn unknown_option(rule: &str, key: &str, known: &toml::Table) -> ConfigError {
    let names: Vec<String> = known.keys().map(|name| format!("`{name}`")).collect();
    let hint = if names.is_empty() {
        "there are no fields".to_string()
    } else {
        format!("expected one of {}", names.join(", "))
    };
    ConfigError::RuleOption {
        rule: rule.to_string(),
        message: format!("unknown field `{key}`, {hint}"),
    }
}

fn cap_with_level(level: Option<&Level>, found: &mut [Finding]) {
    if level == Some(&Level::Warn) {
        for finding in found.iter_mut() {
            if finding.severity == Severity::Deny {
                finding.severity = Severity::Warn;
            }
        }
    }
}

fn raise_with_level(level: Option<&Level>, found: &mut [Finding]) {
    if level == Some(&Level::Deny) {
        for finding in found.iter_mut() {
            if finding.severity == Severity::Warn {
                finding.severity = Severity::Deny;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn config_with_level(rule: &str, level: &str) -> Config {
        let mut table = toml::Table::new();
        table.insert("level".to_string(), toml::Value::String(level.to_string()));
        let mut config = Config::default();
        config.rules.insert(rule.to_string(), table);
        config
    }

    fn world_facts(barrel: Option<Vec<Vec<String>>>) -> WorldFacts {
        WorldFacts {
            lang: Lang::TypeScript,
            is_test_file: false,
            is_entry_file: false,
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel,
        }
    }

    fn file_facts(line_count: u32) -> FileFacts<'static> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/big.rs",
            source: "",
            line_starts: vec![0],
            is_test_file: false,
            is_entry_file: false,
            line_count,
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn barrel_chains(target: &str) -> Option<Vec<Vec<String>>> {
        Some(vec![vec![target.to_string()]])
    }

    #[test]
    fn world_rule_with_level_deny_escalates_its_warn_findings() {
        let config = config_with_level("slop/barrel-file", "deny");
        let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
        let index_facts = world_facts(barrel_chains("src/other.ts"));
        let other_facts = world_facts(barrel_chains("src/index.ts"));
        let files = [
            WorldFile {
                path: "src/index.ts",
                facts: &index_facts,
            },
            WorldFile {
                path: "src/other.ts",
                facts: &other_facts,
            },
        ];
        let found = rules.check_world(&files);
        assert!(!found.is_empty(), "the barrel chain must be reported");
        assert!(
            found
                .iter()
                .all(|finding| finding.severity == Severity::Deny),
            "level deny escalates every barrel-file finding: {:?}",
            found
                .iter()
                .map(|finding| finding.severity)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn meta_parse_error_with_level_deny_escalates_to_deny() {
        let config = config_with_level("meta/parse-error", "deny");
        let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
        let finding = rules
            .parse_error("src/broken.rs", "fn", Pos { line: 1, byte: 0 })
            .expect("the parse error is reported");
        assert_eq!(finding.severity, Severity::Deny);
    }

    #[test]
    fn threshold_rule_with_level_deny_keeps_its_grade() {
        let config = config_with_level("agent/file-token-budget", "deny");
        let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
        let warn_tier = rules.check_file(&file_facts(700));
        assert_eq!(warn_tier.len(), 1);
        assert_eq!(
            warn_tier[0].severity,
            Severity::Warn,
            "the warn tier keeps its own grade"
        );
        let deny_tier = rules.check_file(&file_facts(1200));
        assert_eq!(deny_tier.len(), 1);
        assert_eq!(deny_tier[0].severity, Severity::Deny);
    }
}
