use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/file-token-budget",
    family: Family::Agent,
    summary: "files longer than the profile's line budget",
    why: "Large files exceed what a reader or agent can hold and edit reliably.",
    confidence: Confidence::High,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Keep,
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

struct FileTokenBudget {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (1500, 3000, 4500, 6000),
        Strictness::Basic => (800, 1500, 2250, 3000),
        Strictness::Standard => (600, 1000, 1500, 2000),
        Strictness::Strict => (500, 800, 1200, 1600),
    };
    Preset {
        level: Level::At(Severity::Major),
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
    Ok(Box::new(FileTokenBudget { thresholds }))
}

impl FileRule for FileTokenBudget {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let Some(severity) = self.thresholds.grade(facts.line_count) else {
            return;
        };
        let minor_at = self.thresholds.minor_at;
        let major_at = self.thresholds.major_at;
        let critical_at = self.thresholds.critical_at;
        let blocker_at = self.thresholds.blocker_at;
        let fix = format!("split the file by concern into modules under {minor_at} lines");
        out.push(Finding::at_file(
            facts.path,
            INFO.id,
            severity,
            format!("file has {} lines (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})", facts.line_count),
            fix,
        ));
    }
}
