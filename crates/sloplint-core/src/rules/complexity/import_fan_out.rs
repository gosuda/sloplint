use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Pos};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/import-fan-out",
    family: Family::Complexity,
    summary: "files importing from too many distinct modules",
    why: "Many imported modules mean many files to open before an edit is safe.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Lower1,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    minor_at: u32,
    major_at: u32,
    critical_at: u32,
    blocker_at: u32,
}

struct ImportFanOut {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 15, 25, 38, 50),
        Strictness::Basic => (Level::Off, 15, 25, 38, 50),
        Strictness::Standard => (Level::At(Severity::Minor), 15, 25, 38, 50),
        Strictness::Strict => (Level::At(Severity::Major), 10, 15, 23, 30),
    };
    Preset {
        level,
        options: threshold_options(minor_at, major_at, critical_at, blocker_at),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = parse_options(options)?;
    let thresholds = Thresholds::new(
        parsed.minor_at,
        parsed.major_at,
        parsed.critical_at,
        parsed.blocker_at,
    )?;
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
        let minor_at = self.thresholds.minor_at;
        let major_at = self.thresholds.major_at;
        let critical_at = self.thresholds.critical_at;
        let blocker_at = self.thresholds.blocker_at;
        let message = format!(
            "file imports from {count} distinct modules (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
        );
        let fix = "split orchestration from domain logic: move code that needs only some of these modules into its own module"
            .to_string();
        out.push(Finding::at(facts, pos, INFO.id, severity, message, fix));
    }
}
