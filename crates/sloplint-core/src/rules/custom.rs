//! Custom `[[custom_rules]]` specs: validation, interpolation, and path filtering.

use std::collections::BTreeSet;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::{Config, ConfigError, LANGUAGE_KEYS};
use crate::finding::{Level, Severity};

/// One declarative custom rule from `[[custom_rules]]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomRuleSpec {
    /// Rule id; must match `custom/[a-z0-9]+(-[a-z0-9]+)*`.
    pub id: String,
    /// Config key of the language the query runs against.
    pub language: String,
    /// Severity of every finding this rule reports.
    pub severity: Severity,
    /// Finding message; `{name}` placeholders become capture text.
    pub message: String,
    /// Why the pattern is a defect, shown by `sloplint rules` and the human renderer.
    pub why: String,
    /// Instruction that fixes a reported hit.
    pub fix: String,
    /// Tree-sitter query carrying a `@match` capture.
    pub query: String,
    /// Path globs the rule applies to; empty means every analyzed file.
    #[serde(default)]
    pub include: Vec<String>,
    /// Path globs the rule never applies to.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Validates one spec's schema; `seen` accumulates ids for duplicate detection.
pub fn validate_custom_rule_spec(
    spec: &CustomRuleSpec,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !valid_id(&spec.id) {
        return Err(format!(
            "id must match custom/[a-z0-9]+(-[a-z0-9]+)*, got `{}`",
            spec.id
        ));
    }
    if !seen.insert(spec.id.clone()) {
        return Err(format!("duplicate id `{}`", spec.id));
    }
    if !LANGUAGE_KEYS.contains(&spec.language.as_str()) {
        return Err(format!("unknown language `{}`", spec.language));
    }
    for (field, value) in [
        ("message", &spec.message),
        ("why", &spec.why),
        ("fix", &spec.fix),
        ("query", &spec.query),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} must not be empty"));
        }
    }
    for pattern in spec.include.iter().chain(spec.exclude.iter()) {
        Glob::new(pattern).map_err(|error| format!("invalid glob `{pattern}`: {error}"))?;
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("custom/") else {
        return false;
    };
    !rest.is_empty()
        && rest.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Extracts `{name}` placeholders from a message; `{{` and `}}` are literal escapes.
pub fn message_placeholders(message: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    continue;
                }
                let mut name = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if closed {
                    names.push(name);
                } else {
                    break;
                }
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
            }
            _ => {}
        }
    }
    names
}

/// Renders a message; each placeholder becomes the first line of its capture, clipped to 60 chars.
pub fn interpolate(message: &str, captures: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => placeholder(&mut chars, &mut out, captures),
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                out.push('}');
            }
            _ => out.push(c),
        }
    }
    out
}

fn placeholder(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
    captures: &[(&str, &str)],
) {
    if chars.peek() == Some(&'{') {
        chars.next();
        out.push('{');
        return;
    }
    let mut name = String::new();
    let mut closed = false;
    for next in chars.by_ref() {
        if next == '}' {
            closed = true;
            break;
        }
        name.push(next);
    }
    let captured = captures.iter().find(|(n, _)| *n == name).map(|(_, t)| *t);
    match (closed, captured) {
        (true, Some(text)) => out.push_str(&clip(text)),
        _ => {
            out.push('{');
            out.push_str(&name);
            if closed {
                out.push('}');
            }
        }
    }
}

fn clip(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    line.chars().take(60).collect()
}

pub(crate) struct CustomEntry {
    pub(crate) id: Box<str>,
    pub(crate) message: String,
    pub(crate) why: String,
    pub(crate) fix: String,
    pub(crate) severity: Severity,
    pub(crate) level: Level,
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl CustomEntry {
    pub(crate) fn matches_path(&self, rel_path: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|set| set.is_match(rel_path))
            && self
                .exclude
                .as_ref()
                .is_none_or(|set| !set.is_match(rel_path))
    }
}

pub(crate) fn entries(config: &Config) -> Result<Vec<CustomEntry>, ConfigError> {
    let mut out = Vec::new();
    for (index, spec) in config.custom_rules.iter().enumerate() {
        out.push(CustomEntry {
            id: spec.id.as_str().into(),
            message: spec.message.clone(),
            why: spec.why.clone(),
            fix: spec.fix.clone(),
            severity: spec.severity,
            level: custom_level(config, spec),
            include: compile(&spec.include, &format!("custom_rules[{index}].include"))?,
            exclude: compile(&spec.exclude, &format!("custom_rules[{index}].exclude"))?,
        });
    }
    Ok(out)
}

fn compile(patterns: &[String], key: &str) -> Result<Option<GlobSet>, ConfigError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(glob(pattern, key)?);
    }
    builder
        .build()
        .map(Some)
        .map_err(|error| ConfigError::Glob {
            key: key.to_string(),
            glob: patterns.join(", "),
            message: error.to_string(),
        })
}

fn glob(pattern: &str, key: &str) -> Result<Glob, ConfigError> {
    Glob::new(pattern).map_err(|error| ConfigError::Glob {
        key: key.to_string(),
        glob: pattern.to_string(),
        message: error.to_string(),
    })
}

/// Effective level of a custom rule: its severity unless `[rules."<id>"] level` overrides it.
pub fn custom_level(config: &Config, spec: &CustomRuleSpec) -> Level {
    config
        .rules
        .get(&spec.id)
        .and_then(|table| table.get("level"))
        .and_then(|value| value.as_str())
        .and_then(crate::rules::parse_level)
        .unwrap_or(match spec.severity {
            Severity::Warn => Level::Warn,
            Severity::Deny => Level::Deny,
        })
}
