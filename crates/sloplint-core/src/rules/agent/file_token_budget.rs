//! `agent/file-token-budget`: files longer than the profile's line budget.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Pos};
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// `agent/file-token-budget`: flags files whose line count exceeds the budget.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/file-token-budget",
    family: Family::Agent,
    summary: "files longer than the profile's line budget",
    why: "Large files exceed what a reader or agent can hold and edit reliably.",
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

struct FileTokenBudget {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (warn_at, deny_at) = match strictness {
        Strictness::Minimal => (1500, 3000),
        Strictness::Basic => (800, 1500),
        Strictness::Standard => (600, 1000),
        Strictness::Strict => (500, 800),
    };
    Preset {
        level: Level::Deny,
        options: threshold_options(warn_at, deny_at),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    let thresholds = Thresholds::new(parsed.warn_at, parsed.deny_at)?;
    Ok(Box::new(FileTokenBudget { thresholds }))
}

impl FileRule for FileTokenBudget {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let Some(severity) = self.thresholds.grade(facts.line_count) else {
            return;
        };
        let fix = format!(
            "split the file by concern into modules under {} lines",
            self.thresholds.warn_at
        );
        out.push(Finding::at_source(
            facts.path,
            "",
            Pos { line: 1, byte: 0 },
            INFO.id,
            severity,
            format!("file has {} lines", facts.line_count),
            fix,
        ));
    }
}
