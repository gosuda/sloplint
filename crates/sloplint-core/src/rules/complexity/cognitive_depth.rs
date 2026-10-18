//! `complexity/cognitive-depth`: flags functions whose cognitive complexity exceeds the thresholds.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// Catalog entry for `complexity/cognitive-depth`.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/cognitive-depth",
    family: Family::Complexity,
    summary: "functions whose cognitive complexity exceeds the thresholds",
    why: "Each nested branch multiplies the paths a reader must hold in working memory.",
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

struct CognitiveDepth {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Deny, 20, 30),
        Strictness::Basic => (Level::Deny, 15, 25),
        Strictness::Standard => (Level::Deny, 12, 20),
        Strictness::Strict => (Level::Deny, 10, 15),
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
    Ok(Box::new(CognitiveDepth { thresholds }))
}

impl FileRule for CognitiveDepth {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            let Some(severity) = self.thresholds.grade(function.cognitive) else {
                continue;
            };
            let value = function.cognitive;
            let name = function.name;
            let warn_at = self.thresholds.warn_at;
            let deny_at = self.thresholds.deny_at;
            let message = format!(
                "cognitive complexity {value} in `{name}` (warn at {warn_at}, deny at {deny_at})"
            );
            let fix = format!(
                "flatten `{name}`: turn nested conditions into early returns, then move the deepest block into a named helper"
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
