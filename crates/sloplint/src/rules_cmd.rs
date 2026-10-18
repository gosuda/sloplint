//! `sloplint rules`: effective rule levels and options as agent text or JSON.

use std::io;
use std::path::PathBuf;

use serde::Serialize;
use sloplint_core::rules::custom::custom_level;
use sloplint_core::{Config, ConfigError, Level, Preset, RuleFilter, RuleSet, Strictness};

use crate::RunError;
use crate::cli::Format;
use crate::project::{self, Overrides};

const PROFILES: [Strictness; 4] = [
    Strictness::Minimal,
    Strictness::Basic,
    Strictness::Standard,
    Strictness::Strict,
];

/// Inputs to `sloplint rules`.
#[derive(Debug, Clone, Default)]
pub struct RulesRequest {
    /// Directory that discovery starts from.
    pub cwd: PathBuf,
    /// Explicit configuration file.
    pub config: Option<PathBuf>,
    /// Strictness override.
    pub strictness: Option<Strictness>,
    /// Show only this rule, with its rationale and presets, when set.
    pub id: Option<String>,
    /// Output format; `human` prints the agent text.
    pub format: Format,
}

struct Listed {
    id: String,
    family: &'static str,
    level: Level,
    options: toml::Table,
    summary: String,
    why: String,
    presets: Option<[Preset; 4]>,
}

#[derive(Serialize)]
struct JsonDoc<'a> {
    strictness: Strictness,
    rules: Vec<JsonRule<'a>>,
}

#[derive(Serialize)]
struct JsonRule<'a> {
    id: &'a str,
    family: &'a str,
    level: Level,
    options: &'a toml::Table,
    summary: &'a str,
    why: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    presets: Option<JsonPresets<'a>>,
}

#[derive(Serialize)]
struct JsonPresets<'a> {
    minimal: JsonPreset<'a>,
    basic: JsonPreset<'a>,
    standard: JsonPreset<'a>,
    strict: JsonPreset<'a>,
}

#[derive(Serialize)]
struct JsonPreset<'a> {
    level: Level,
    options: &'a toml::Table,
}

/// Renders the effective rule table for `request` in its requested format.
pub fn render(request: &RulesRequest) -> Result<String, RunError> {
    let location = project::locate(&request.cwd, None, request.config.as_deref());
    let loaded = project::load(
        &location,
        Overrides {
            strictness: request.strictness,
            ..Overrides::default()
        },
    )?;
    let rules = RuleSet::new(&loaded.config, &RuleFilter::default())?;
    let mut listed = builtin_entries(&rules);
    listed.extend(custom_entries(&loaded.config));
    let shown = select(&listed, request.id.as_deref())?;
    let strictness = loaded.config.project.strictness;
    match (&request.format, request.id.is_some()) {
        (Format::Json, _) => json(strictness, &shown).map_err(|source| RunError::Io {
            path: request.cwd.clone(),
            source: io::Error::other(source),
        }),
        (_, false) => Ok(list_text(strictness, &shown)),
        (_, true) => Ok(detail_text(strictness, &shown)),
    }
}

fn list_text(strictness: Strictness, shown: &[&Listed]) -> String {
    let mut lines = vec![header(strictness)];
    lines.extend(shown.iter().map(|entry| entry_line(entry)));
    document(lines)
}

fn detail_text(strictness: Strictness, shown: &[&Listed]) -> String {
    let mut lines = vec![header(strictness)];
    for entry in shown {
        lines.push(entry_line(entry));
        lines.push(format!("why: {}", entry.why));
        if let Some(presets) = &entry.presets {
            lines.extend(preset_lines(presets));
        }
    }
    document(lines)
}

fn header(strictness: Strictness) -> String {
    format!("strictness={}", strictness.as_str())
}

fn entry_line(entry: &Listed) -> String {
    format!(
        "{} level={}{} | {}",
        entry.id,
        level_name(entry.level),
        options_text(&entry.options),
        entry.summary
    )
}

fn preset_lines(presets: &[Preset; 4]) -> Vec<String> {
    PROFILES
        .iter()
        .zip(presets.iter())
        .map(|(profile, preset)| {
            format!(
                "preset.{}: level={}{}",
                profile.as_str(),
                level_name(preset.level),
                options_text(&preset.options)
            )
        })
        .collect()
}

fn document(lines: Vec<String>) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn select<'a>(listed: &'a [Listed], id: Option<&str>) -> Result<Vec<&'a Listed>, RunError> {
    let Some(id) = id else {
        return Ok(listed.iter().collect());
    };
    let found: Vec<&Listed> = listed.iter().filter(|entry| entry.id == id).collect();
    if found.is_empty() {
        let mut valid: Vec<String> = listed.iter().map(|entry| entry.id.clone()).collect();
        valid.sort();
        return Err(ConfigError::UnknownRule {
            id: id.to_string(),
            valid,
        }
        .into());
    }
    Ok(found)
}

fn builtin_entries(rules: &RuleSet) -> Vec<Listed> {
    let mut entries: Vec<Listed> = rules
        .resolved()
        .iter()
        .map(|resolved| Listed {
            id: resolved.info.id.to_string(),
            family: resolved.info.family.as_str(),
            level: resolved.level,
            options: resolved.options.clone(),
            summary: resolved.info.summary.to_string(),
            why: resolved.info.why.to_string(),
            presets: Some(PROFILES.map(|profile| (resolved.info.preset)(profile))),
        })
        .collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

fn custom_entries(config: &Config) -> Vec<Listed> {
    config
        .custom_rules
        .iter()
        .map(|spec| Listed {
            id: spec.id.clone(),
            family: "custom",
            level: custom_level(config, spec),
            options: toml::Table::new(),
            summary: spec.message.clone(),
            why: spec.why.clone(),
            presets: None,
        })
        .collect()
}

fn json(strictness: Strictness, shown: &[&Listed]) -> Result<String, serde_json::Error> {
    let rules = shown
        .iter()
        .map(|entry| JsonRule {
            id: &entry.id,
            family: entry.family,
            level: entry.level,
            options: &entry.options,
            summary: &entry.summary,
            why: &entry.why,
            presets: entry.presets.as_ref().map(json_presets),
        })
        .collect();
    let mut text = serde_json::to_string(&JsonDoc { strictness, rules })?;
    text.push('\n');
    Ok(text)
}

fn json_presets(presets: &[Preset; 4]) -> JsonPresets<'_> {
    let [minimal, basic, standard, strict] = presets;
    JsonPresets {
        minimal: json_preset(minimal),
        basic: json_preset(basic),
        standard: json_preset(standard),
        strict: json_preset(strict),
    }
}

fn json_preset(preset: &Preset) -> JsonPreset<'_> {
    JsonPreset {
        level: preset.level,
        options: &preset.options,
    }
}

fn options_text(options: &toml::Table) -> String {
    let mut pairs: Vec<(&String, &toml::Value)> = options.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(key, value)| format!(" {key}={value}"))
        .collect()
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Off => "off",
        Level::Warn => "warn",
        Level::Deny => "deny",
    }
}
