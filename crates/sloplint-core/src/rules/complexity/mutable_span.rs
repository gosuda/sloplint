use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/mutable-span",
    family: Family::Complexity,
    summary: "bindings that stay mutable across too many lines",
    why: "A long-lived mutable binding forces the reader to replay every write.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Off,
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

struct MutableSpan {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 30, 60, 90, 120),
        Strictness::Basic => (Level::At(Severity::Minor), 30, 60, 90, 120),
        Strictness::Standard => (Level::At(Severity::Major), 20, 40, 60, 80),
        Strictness::Strict => (Level::At(Severity::Major), 15, 30, 45, 60),
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
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "`{name}` stays mutable for {span} lines (lines {first}-{last}) (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
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
