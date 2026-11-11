use std::collections::BTreeSet;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::{Config, ConfigError};
use crate::finding::{Confidence, Level, Severity};
use crate::lang::ALL_CONFIG_KEYS;

fn default_confidence() -> Confidence {
    Confidence::Medium
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomRuleSpec {
    pub id: String,
    pub language: String,
    pub severity: Severity,
    #[serde(default = "default_confidence")]
    pub confidence: Confidence,
    pub message: String,
    pub why: String,
    pub fix: String,
    pub query: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

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
    if !ALL_CONFIG_KEYS.contains(&spec.language.as_str()) {
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
    message_placeholders(&spec.message).map_err(|error| error.to_string())?;
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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlaceholderError {
    #[error("message has an unclosed `{{` at byte {offset}; write `{{{{` for a literal brace")]
    Unclosed { offset: usize },
    #[error(
        "message has an empty placeholder `{{}}` at byte {offset}; name a capture or write `{{{{}}}}` for literal braces"
    )]
    Empty { offset: usize },
}

pub fn message_placeholders(message: &str) -> Result<Vec<String>, PlaceholderError> {
    let mut names = Vec::new();
    let mut chars = message.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        match c {
            '{' => {
                if chars.next_if(|&(_, next)| next == '{').is_none() {
                    names.push(read_placeholder(&mut chars, offset)?);
                }
            }
            '}' => {
                let _ = chars.next_if(|&(_, next)| next == '}');
            }
            _ => {}
        }
    }
    Ok(names)
}

fn read_placeholder(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    offset: usize,
) -> Result<String, PlaceholderError> {
    let mut name = String::new();
    for (_, next) in chars.by_ref() {
        if next != '}' {
            name.push(next);
            continue;
        }
        if name.is_empty() {
            return Err(PlaceholderError::Empty { offset });
        }
        return Ok(name);
    }
    Err(PlaceholderError::Unclosed { offset })
}

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
        (true, None) => {}
        (false, _) => {
            out.push('{');
            out.push_str(&name);
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
    pub(crate) level: Level,
    pub(crate) confidence: Confidence,
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

    pub(crate) fn severity_at(&self, rel_path: &str) -> Option<Severity> {
        if !self.matches_path(rel_path) {
            return None;
        }
        self.level.rung()
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
            level: custom_level(config, spec),
            confidence: spec.confidence,
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

pub fn custom_level(config: &Config, spec: &CustomRuleSpec) -> Level {
    config
        .rules
        .get(&spec.id)
        .and_then(|table| table.get("level"))
        .and_then(|value| value.as_str())
        .and_then(crate::rules::parse_level)
        .unwrap_or(Level::At(spec.severity))
}

#[cfg(test)]
mod tests {
    use super::interpolate;

    #[test]
    fn absent_optional_capture_renders_as_empty_string() {
        assert_eq!(interpolate("a {x} b", &[]), "a  b");
    }

    #[test]
    fn unclosed_placeholder_keeps_its_brace() {
        assert_eq!(interpolate("a {x", &[]), "a {x");
    }

    #[test]
    fn present_capture_substitutes_its_first_line() {
        assert_eq!(interpolate("[{x}]", &[("x", "first\nsecond")]), "[first]");
    }
}
