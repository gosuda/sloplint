use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::*;

use super::*;
use crate::finding::SourceText;
use crate::role::{CodeRole, role_at};

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
        let settings = Settings::from_config(config)?;
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
            let wanted = entry.level != Level::Off;
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
            if level != Level::Off {
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

    pub fn drop_shadowed_nesting(&self, findings: &mut Vec<Finding>) {
        if !self.is_active("complexity/cognitive-depth")
            || !self.selects("complexity/cognitive-depth")
        {
            return;
        }
        let mut fired = BTreeSet::new();
        for finding in findings.iter() {
            if finding.rule.as_ref() == "complexity/cognitive-depth" {
                fired.insert((&*finding.path, finding.line, finding.column));
            }
        }
        if fired.is_empty() {
            return;
        }
        let mut drop = vec![false; findings.len()];
        for (index, finding) in findings.iter().enumerate() {
            if finding.rule.as_ref() != "complexity/nesting-depth" {
                continue;
            }
            drop[index] = fired.contains(&(&*finding.path, finding.line, finding.column));
        }
        let mut drops = drop.into_iter();
        findings.retain(|_| !drops.next().unwrap_or(false));
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

    pub fn selects(&self, id: &str) -> bool {
        self.filter.matches(id)
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
#[path = "ruleset_tests.rs"]
mod tests;
