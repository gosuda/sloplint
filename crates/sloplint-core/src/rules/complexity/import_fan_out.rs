//! `complexity/import-fan-out`: flags files importing from too many distinct modules.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Pos};
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// Catalog entry for `complexity/import-fan-out`.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/import-fan-out",
    family: Family::Complexity,
    summary: "files importing from too many distinct modules",
    why: "Many imported modules mean many files to open before an edit is safe.",
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

struct ImportFanOut {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Off, 15, 25),
        Strictness::Basic => (Level::Off, 15, 25),
        Strictness::Standard => (Level::Warn, 15, 25),
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
    Ok(Box::new(ImportFanOut { thresholds }))
}

impl FileRule for ImportFanOut {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let modules: BTreeSet<&str> = facts
            .imports
            .iter()
            .map(|import| import.module.as_ref())
            .collect();
        let count = u32::try_from(modules.len()).unwrap_or(u32::MAX);
        let Some(severity) = self.thresholds.grade(count) else {
            return;
        };
        let pos = facts
            .imports
            .first()
            .map_or(Pos { line: 0, byte: 0 }, |import| import.pos);
        let message = format!("file imports from {count} distinct modules");
        let fix = "split orchestration from domain logic: move code that needs only some of these modules into its own module"
            .to_string();
        out.push(Finding::at(facts, pos, INFO.id, severity, message, fix));
    }
}
