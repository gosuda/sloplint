pub(crate) mod agent;
pub(crate) mod complexity;
pub mod custom;
pub(crate) mod grep;
pub mod meta;
pub(crate) mod slop;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::callee::CalleePattern;
use crate::config::{Config, ConfigError, Strictness, static_language_key};
use crate::facts::{FileFacts, Pos, WorldFacts};
use crate::finding::{Confidence, Finding, Level, Severity, SourceText};
use crate::role::{CodeRole, RolePolicy, role_at};
use crate::suppress::RulePattern;

pub use self::custom::CustomRuleSpec;

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
    complexity::nested_mutation::INFO,
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
            .filter(|entry| entry.options.contains_key("minor_at"))
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
            let mut found = Vec::new();
            rule.check(facts, &mut found);
            self.finish_file_findings(info, facts, &mut found);
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
                self.finish_world_findings(info, files, &mut found);
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
            let Some(entry) = self
                .custom
                .get(usize::try_from(hit.rule).unwrap_or(usize::MAX))
            else {
                continue;
            };
            if !self.filter.matches(&entry.id) {
                continue;
            }
            if !entry.matches_path(facts.path) {
                continue;
            }
            let Some(severity) = entry.level.rung() else {
                continue;
            };
            let message = custom::interpolate(&entry.message, &hit.captures);
            let mut finding = Finding::at(
                facts,
                hit.pos,
                &entry.id,
                severity,
                message,
                entry.fix.clone(),
            );
            finding.confidence = entry.confidence;
            out.push(finding);
        }
        out
    }

    pub fn parse_error(&self, text: &SourceText, pos: Pos) -> Option<Finding> {
        if !self.is_active(meta::PARSE_ERROR.id) {
            return None;
        }
        let mut findings = [meta::parse_error_finding(text, pos)];
        self.stamp_meta(&mut findings);
        let [finding] = findings;
        Some(finding)
    }

    pub fn custom_caps(&self, facts: &FileFacts<'_>, capped: &[&str]) -> Vec<Finding> {
        if !self.is_active(meta::CUSTOM_CAP.id) {
            return Vec::new();
        }
        let mut findings: Vec<Finding> = capped
            .iter()
            .map(|rule| {
                Finding::at(
                    facts,
                    Pos { line: 1, byte: 0 },
                    meta::CUSTOM_CAP.id,
                    Severity::Major,
                    format!("custom rule {rule} hit the per-file query cap; its matches for this file are partial"),
                    "anchor the query to specific parents or add predicates so it cannot match unbounded sibling combinations"
                        .to_string(),
                )
            })
            .collect();
        self.stamp_meta(&mut findings);
        findings
    }

    pub fn unused_suppression(&self, text: &SourceText, line: u32, rules: &str) -> Option<Finding> {
        if !self.is_active(meta::UNUSED_SUPPRESSION.id) {
            return None;
        }
        let mut findings = [meta::unused_suppression_finding(text, line, rules)];
        self.stamp_meta(&mut findings);
        let [finding] = findings;
        Some(finding)
    }

    fn stamp_meta(&self, findings: &mut [Finding]) {
        for finding in findings.iter_mut() {
            let graded = self.graded.contains(&*finding.rule);
            if let Some(Level::At(ceiling)) = self.levels.get(&*finding.rule).copied() {
                if graded {
                    if finding.severity > ceiling {
                        finding.severity = ceiling;
                    }
                } else {
                    finding.severity = ceiling;
                }
            }
            let confidence = self.confidence_for(&finding.rule);
            finding.confidence = confidence;
        }
    }

    pub fn cap_levels(&self, findings: &mut [Finding]) {
        self.stamp_meta(findings);
    }

    fn confidence_for(&self, rule: &str) -> Confidence {
        if let Some(info) = CATALOG.iter().find(|info| info.id == rule) {
            return info.confidence;
        }
        if let Some(entry) = self.custom.iter().find(|entry| &*entry.id == rule) {
            return entry.confidence;
        }
        Confidence::Medium
    }

    fn finish_file_findings(
        &self,
        info: &RuleInfo,
        facts: &FileFacts<'_>,
        found: &mut Vec<Finding>,
    ) {
        let Some(Level::At(ceiling)) = self.levels.get(info.id).copied() else {
            found.clear();
            return;
        };
        let graded = self.graded.contains(info.id);
        found.retain_mut(|finding| {
            if graded {
                if finding.severity > ceiling {
                    finding.severity = ceiling;
                }
            } else {
                finding.severity = ceiling;
            }
            let shift = info.roles.shift(facts.role_at(finding.line));
            match shift.apply(finding.severity) {
                Some(severity) => {
                    finding.severity = severity;
                    finding.confidence = info.confidence;
                    true
                }
                None => false,
            }
        });
    }

    fn finish_world_findings(
        &self,
        info: &RuleInfo,
        files: &[WorldFile<'_>],
        found: &mut Vec<Finding>,
    ) {
        let Some(Level::At(ceiling)) = self.levels.get(info.id).copied() else {
            found.clear();
            return;
        };
        let graded = self.graded.contains(info.id);
        let index: HashMap<&str, &WorldFacts> =
            files.iter().map(|file| (file.path, file.facts)).collect();
        found.retain_mut(|finding| {
            if graded {
                if finding.severity > ceiling {
                    finding.severity = ceiling;
                }
            } else {
                finding.severity = ceiling;
            }
            let role = index
                .get(&*finding.path)
                .map(|facts| role_at(facts.role, &facts.regions, finding.line))
                .unwrap_or(CodeRole::Library);
            match info.roles.shift(role).apply(finding.severity) {
                Some(severity) => {
                    finding.severity = severity;
                    finding.confidence = info.confidence;
                    true
                }
                None => false,
            }
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::role::Shift;

    fn world_facts(role: CodeRole) -> WorldFacts {
        WorldFacts {
            lang: Lang::Rust,
            role,
            regions: Vec::new(),
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
        }
    }

    fn test_preset(_: Strictness) -> Preset {
        Preset {
            level: Level::At(Severity::Major),
            options: toml::Table::new(),
        }
    }

    fn test_build(_: toml::Table) -> Result<Box<dyn WorldRule>, String> {
        Err("unused".to_string())
    }

    #[test]
    fn world_findings_shift_by_file_role() {
        let info = RuleInfo {
            id: "test/world-shift",
            family: Family::Agent,
            summary: "test",
            why: "test",
            confidence: Confidence::High,
            roles: RolePolicy {
                entry: Shift::Keep,
                test: Shift::Off,
                support: Shift::Keep,
                bench: Shift::Lower1,
                example: Shift::Keep,
            },
            preset: test_preset,
            build: Build::World(test_build),
        };
        let set = RuleSet {
            file_rules: Vec::new(),
            world_rules: Vec::new(),
            resolved: Vec::new(),
            levels: BTreeMap::from([("test/world-shift".to_string(), Level::At(Severity::Major))]),
            graded: BTreeSet::new(),
            active: BTreeSet::new(),
            filter: RuleFilter::default(),
            custom: Vec::new(),
        };
        let library = world_facts(CodeRole::Library);
        let tested = world_facts(CodeRole::Test);
        let bench = world_facts(CodeRole::Bench);
        let files = [
            WorldFile {
                path: "a.rs",
                facts: &library,
            },
            WorldFile {
                path: "m.rs",
                facts: &tested,
            },
            WorldFile {
                path: "z.rs",
                facts: &bench,
            },
        ];
        let mut found = vec![
            Finding::at_file(
                "z.rs",
                info.id,
                Severity::Major,
                "z".to_string(),
                "f".to_string(),
            ),
            Finding::at_file(
                "a.rs",
                info.id,
                Severity::Major,
                "a".to_string(),
                "f".to_string(),
            ),
            Finding::at_file(
                "m.rs",
                info.id,
                Severity::Major,
                "m".to_string(),
                "f".to_string(),
            ),
        ];
        set.finish_world_findings(&info, &files, &mut found);
        let mut got: Vec<(&str, Severity)> = found
            .iter()
            .map(|finding| (&*finding.path, finding.severity))
            .collect();
        got.sort_unstable();
        assert_eq!(got, [("a.rs", Severity::Major), ("z.rs", Severity::Minor)]);
        assert!(
            found
                .iter()
                .all(|finding| finding.confidence == Confidence::High),
            "world findings carry the rule's confidence"
        );
    }
}
