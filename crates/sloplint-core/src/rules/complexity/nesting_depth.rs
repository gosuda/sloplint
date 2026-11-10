use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/nesting-depth",
    family: Family::Complexity,
    summary: "functions whose nesting depth exceeds the thresholds",
    why: "Deep nesting hides the condition that guards each line.",
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

struct NestingDepth {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::At(Severity::Major), 6, 8, 12, 16),
        Strictness::Basic => (Level::At(Severity::Major), 5, 7, 11, 14),
        Strictness::Standard => (Level::At(Severity::Major), 5, 6, 9, 12),
        Strictness::Strict => (Level::At(Severity::Major), 4, 5, 8, 10),
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
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let deepest_line = function.deepest_line;
            let message = format!(
                "nesting depth {value} in `{name}` (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = format!(
                "reduce nesting in `{name}` below {minor_at}: use guard clauses or extract the block at line {deepest_line} into a helper"
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
