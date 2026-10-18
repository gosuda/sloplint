//! `complexity/nested-mutation`: flags outer bindings written deep inside loops or closures.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// Catalog entry for `complexity/nested-mutation`.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/nested-mutation",
    family: Family::Complexity,
    summary: "outer bindings written deep inside loops or closures",
    why: "Writes to outer state from inner loops or closures couple iterations and hide data flow.",
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

struct NestedMutation {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Off, 3, 4),
        Strictness::Basic => (Level::Off, 3, 4),
        Strictness::Standard => (Level::Warn, 3, 4),
        Strictness::Strict => (Level::Deny, 2, 3),
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
    Ok(Box::new(NestedMutation { thresholds }))
}

impl FileRule for NestedMutation {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for binding in &facts.bindings {
            let Some(severity) = self.thresholds.grade(binding.max_nested_depth) else {
                continue;
            };
            let pos = binding.nested_write_pos.unwrap_or(binding.pos);
            let name = binding.name;
            let declared = binding.pos.line;
            let depth = binding.max_nested_depth;
            let message = format!(
                "`{name}` (declared line {declared}) is written {depth} loop or closure levels deep"
            );
            let fix = "accumulate in the inner scope and combine outside it, or replace the loops with a fold or collect"
                .to_string();
            out.push(Finding::at(facts, pos, INFO.id, severity, message, fix));
        }
    }
}
