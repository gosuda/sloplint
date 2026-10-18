//! `complexity/nesting-depth`: flags functions whose nesting depth exceeds the thresholds.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// Catalog entry for `complexity/nesting-depth`.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/nesting-depth",
    family: Family::Complexity,
    summary: "functions whose nesting depth exceeds the thresholds",
    why: "Deep nesting hides the condition that guards each line.",
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

struct NestingDepth {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Deny, 6, 8),
        Strictness::Basic => (Level::Deny, 5, 7),
        Strictness::Standard => (Level::Deny, 5, 6),
        Strictness::Strict => (Level::Deny, 4, 5),
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
    Ok(Box::new(NestingDepth { thresholds }))
}

impl FileRule for NestingDepth {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            let Some(severity) = self.thresholds.grade(function.max_depth) else {
                continue;
            };
            let value = function.max_depth;
            let name = function.name;
            let warn_at = self.thresholds.warn_at;
            let deepest_line = function.deepest_line;
            let message = format!("nesting depth {value} in `{name}`");
            let fix = format!(
                "reduce nesting in `{name}` below {warn_at}: use guard clauses or extract the block at line {deepest_line} into a helper"
            );
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                severity,
                message,
                fix,
            ));
        }
    }
}
