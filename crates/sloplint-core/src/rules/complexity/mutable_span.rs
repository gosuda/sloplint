//! `complexity/mutable-span`: flags bindings that stay mutable across too many lines.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// Catalog entry for `complexity/mutable-span`.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/mutable-span",
    family: Family::Complexity,
    summary: "bindings that stay mutable across too many lines",
    why: "A long-lived mutable binding forces the reader to replay every write.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    warn_at: u32,
    deny_at: u32,
}

struct MutableSpan {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Off, 30, 60),
        Strictness::Basic => (Level::Warn, 30, 60),
        Strictness::Standard => (Level::Deny, 20, 40),
        Strictness::Strict => (Level::Deny, 15, 30),
    };
    Preset {
        level,
        options: threshold_options(warn_at, deny_at),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    let thresholds = Thresholds::new(parsed.warn_at, parsed.deny_at)?;
    Ok(Box::new(MutableSpan { thresholds }))
}

impl FileRule for MutableSpan {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for binding in &facts.bindings {
            let span = binding.last_write_line.saturating_sub(binding.pos.line);
            let Some(severity) = self.thresholds.grade(span) else {
                continue;
            };
            let name = binding.name;
            let first = binding.pos.line;
            let last = binding.last_write_line;
            let message = format!("`{name}` stays mutable for {span} lines (lines {first}-{last})");
            let fix = format!(
                "derive `{name}` in one expression, or move lines {}-{last} into a helper that returns its final value",
                first.saturating_add(1),
            );
            out.push(Finding::at(
                facts,
                binding.pos,
                INFO.id,
                severity,
                message,
                fix,
            ));
        }
    }
}
