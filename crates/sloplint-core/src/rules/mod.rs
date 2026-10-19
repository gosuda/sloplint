//! The rule catalog: metadata, thresholds, filters, and the resolver.

pub(crate) mod agent;
pub(crate) mod complexity;
pub mod custom;
pub(crate) mod grep;
pub mod meta;
pub(crate) mod slop;

use std::collections::{BTreeMap, BTreeSet};

use crate::callee::CalleePattern;
use crate::config::{Config, ConfigError, Strictness, static_language_key};
use crate::facts::{FileFacts, Pos, WorldFacts};
use crate::finding::{Finding, Level, Severity};
use crate::suppress::RulePattern;

pub use self::custom::CustomRuleSpec;

/// Rule families; every rule id starts with its family prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// Working-memory load metrics.
    Complexity,
    /// Text-search recall and precision.
    Grep,
    /// Zero-information code.
    Slop,
    /// Code an autonomous agent can discover, load, patch, and verify.
    Agent,
    /// Linter self-checks emitted outside the rule run.
    Meta,
    /// User-defined `[[custom_rules]]`.
    Custom,
}

impl Family {
    /// The id prefix used in config and CLI output.
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

/// Static metadata and constructor for one builtin or meta rule.
#[derive(Debug, Clone, Copy)]
pub struct RuleInfo {
    /// Full rule id, such as `complexity/cognitive-depth`.
    pub id: &'static str,
    /// Family the id belongs to.
    pub family: Family,
    /// One-line description shown by `sloplint rules`.
    pub summary: &'static str,
    /// Rationale shown alongside fix instructions.
    pub why: &'static str,
    /// Findings in test files are dropped when true.
    pub exempt_tests: bool,
    /// Preset for one strictness profile.
    pub preset: fn(Strictness) -> Preset,
    /// Constructor kind; `Meta` rules have none.
    pub build: Build,
}

/// A rule's complete option table and level for one strictness profile.
#[derive(Debug, Clone)]
pub struct Preset {
    /// Level the profile assigns.
    pub level: Level,
    /// Every option key the rule accepts at this profile.
    pub options: toml::Table,
}

/// Constructor signature for a per-file rule.
pub type FileRuleBuilder = fn(toml::Table, &Settings) -> Result<Box<dyn FileRule>, String>;

/// Constructor signature for a cross-file world rule.
pub type WorldRuleBuilder = fn(toml::Table) -> Result<Box<dyn WorldRule>, String>;

/// How a rule object is constructed from merged options.
#[derive(Debug, Clone, Copy)]
pub enum Build {
    /// Per-file rule built from options and language settings.
    File(FileRuleBuilder),
    /// Cross-file rule built from options only.
    World(WorldRuleBuilder),
    /// Emitted outside the rule run; no constructor.
    Meta,
}

/// Language-scoped patterns shared with every file rule.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Log callee patterns keyed by `Lang::config_key`.
    pub log_callees: BTreeMap<&'static str, Vec<CalleePattern>>,
}

impl Settings {
    /// Builds settings from the resolved config's language tables.
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

/// Detection over one analyzed file.
pub trait FileRule: Send + Sync {
    /// Appends findings for `facts`; rules only detect, never cap or filter.
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>);
}

/// Detection across every analyzed file, sorted by path.
pub trait WorldRule: Send + Sync {
    /// Appends findings for the whole file set.
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>);
}

/// One file's world facts in the world-rule input slice.
pub struct WorldFile<'a> {
    /// Repo-relative path with `/` separators.
    pub path: &'a str,
    /// Serializable per-file summary.
    pub facts: &'a WorldFacts,
}

/// Shared warn/deny thresholds every metric rule grades with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// Lowest value reported as warn.
    pub warn_at: u32,
    /// Lowest value reported as deny.
    pub deny_at: u32,
}

impl Thresholds {
    /// Creates thresholds; requires `1 <= warn_at <= deny_at`.
    pub fn new(warn_at: u32, deny_at: u32) -> Result<Thresholds, String> {
        if warn_at == 0 || warn_at > deny_at {
            return Err(format!(
                "warn_at {warn_at} must be at least 1 and at most deny_at {deny_at}"
            ));
        }
        Ok(Thresholds { warn_at, deny_at })
    }

    /// Grades a metric value into a severity, or `None` below `warn_at`.
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

/// CLI `--rule` filter; an empty filter matches every rule.
#[derive(Debug, Clone, Default)]
pub struct RuleFilter(pub Vec<RulePattern>);

impl RuleFilter {
    /// Whether `rule_id` passes the filter.
    pub fn matches(&self, rule_id: &str) -> bool {
        self.0.is_empty() || self.0.iter().any(|pattern| pattern.matches(rule_id))
    }
}

/// One custom-rule query hit produced by the syntax crate.
pub struct CustomHit<'s> {
    /// Index into the config's `custom_rules` order.
    pub rule: u32,
    /// Position of the `@match` capture start.
    pub pos: Pos,
    /// `(capture name, node text)` pairs for message interpolation.
    pub captures: Vec<(&'s str, &'s str)>,
}

/// Effective resolution of one catalog rule, for `sloplint rules`.
#[derive(Debug, Clone)]
pub struct ResolvedRule {
    /// Catalog metadata.
    pub info: &'static RuleInfo,
    /// Level after the user table overrode the preset.
    pub level: Level,
    /// Preset options with every user key written over them.
    pub options: toml::Table,
}

/// Every builtin and meta rule, listed by id in sorted order.
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

/// Active rules built from one config plus a filter; safe to share across threads.
pub struct RuleSet {
    file_rules: Vec<(&'static RuleInfo, Box<dyn FileRule>)>,
    world_rules: Vec<(&'static RuleInfo, Box<dyn WorldRule>)>,
    resolved: Vec<ResolvedRule>,
    levels: BTreeMap<String, Level>,
    active: BTreeSet<String>,
    filter: RuleFilter,
    custom: Vec<custom::CustomEntry>,
}

impl RuleSet {
    /// Rejects unknown `--rule` selectors before any rule is built.
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

    /// Resolves every catalog rule: the preset of the active strictness with the user table over it.
    ///
    /// User options are checked even when the rule is off or filtered out.
    /// Only enabled rules that pass the filter remain active.
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

    /// Runs every active file rule over one file, then applies test exemptions and the level cap.
    pub fn check_file(&self, facts: &FileFacts<'_>) -> Vec<Finding> {
        let mut out = Vec::new();
        for (info, rule) in &self.file_rules {
            let mut found = Vec::new();
            rule.check(facts, &mut found);
            if info.exempt_tests && facts.is_test_file {
                continue;
            }
            cap_with_level(self.levels.get(info.id), &mut found);
            out.append(&mut found);
        }
        out
    }

    /// Runs every active world rule over the path-sorted file set and caps their levels.
    pub fn check_world(&self, files: &[WorldFile<'_>]) -> Vec<Finding> {
        let mut out = Vec::new();
        for (info, rule) in &self.world_rules {
            let mut found = Vec::new();
            rule.check(files, &mut found);
            cap_with_level(self.levels.get(info.id), &mut found);
            out.append(&mut found);
        }
        out
    }

    /// Turns custom-rule hits into findings with path globs, the level cap, and interpolation applied.
    pub fn custom_findings(&self, facts: &FileFacts<'_>, hits: &[CustomHit<'_>]) -> Vec<Finding> {
        let mut out = Vec::new();
        for hit in hits {
            let Some(entry) = self.custom.get(hit.rule as usize) else {
                continue;
            };
            if entry.level == Level::Off || !self.filter.matches(&entry.id) {
                continue;
            }
            if !entry.matches_path(facts.path) {
                continue;
            }
            let severity = match entry.level {
                Level::Warn => Severity::Warn,
                _ => entry.severity,
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

    /// Builds the `meta/parse-error` finding when the rule is active; other rules skip the file.
    pub fn parse_error(&self, path: &str, source: &str, pos: Pos) -> Option<Finding> {
        if !self.is_active(meta::PARSE_ERROR.id) {
            return None;
        }
        let mut finding = [meta::parse_error_finding(path, source, pos)];
        self.cap_levels(&mut finding);
        Some(
            finding
                .into_iter()
                .next()
                .unwrap_or_else(|| meta::parse_error_finding(path, source, pos)),
        )
    }

    /// Builds the `meta/unused-suppression` finding when the rule is active.
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
        let mut finding = [meta::unused_suppression_finding(path, source, line, rules)];
        self.cap_levels(&mut finding);
        Some(
            finding
                .into_iter()
                .next()
                .unwrap_or_else(|| meta::unused_suppression_finding(path, source, line, rules)),
        )
    }

    /// Applies the level cap to findings emitted outside the rule run, such as suppression meta findings.
    pub fn cap_levels(&self, findings: &mut [Finding]) {
        for finding in findings.iter_mut() {
            let level = self.levels.get(&*finding.rule).copied();
            if level == Some(Level::Warn) && finding.severity == Severity::Deny {
                finding.severity = Severity::Warn;
            }
        }
    }

    /// Rationale for a builtin, meta, or custom rule id.
    pub fn why(&self, rule: &str) -> Option<&str> {
        if let Some(info) = CATALOG.iter().find(|info| info.id == rule) {
            return Some(info.why);
        }
        self.custom
            .iter()
            .find(|entry| &*entry.id == rule)
            .map(|entry| entry.why.as_str())
    }

    /// Whether the id names a catalog or custom rule.
    pub fn is_known(&self, rule: &str) -> bool {
        CATALOG.iter().any(|info| info.id == rule)
            || self.custom.iter().any(|entry| &*entry.id == rule)
    }

    /// Whether the rule is enabled above `off` and passes the filter.
    pub fn is_active(&self, rule: &str) -> bool {
        self.active.contains(rule)
    }

    /// Effective level and options of every catalog rule, in catalog order.
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
