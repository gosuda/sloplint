use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/nested-mutation",
    family: Family::Complexity,
    summary: "outer bindings written deep inside loops or closures",
    why: "Writes to outer state from inner loops or closures couple iterations and hide data flow.",
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

struct NestedMutation {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 3, 4, 6, 8),
        Strictness::Basic => (Level::Off, 3, 4, 6, 8),
        Strictness::Standard => (Level::At(Severity::Minor), 3, 4, 6, 8),
        Strictness::Strict => (Level::At(Severity::Major), 2, 3, 5, 6),
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
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "`{name}` (declared line {declared}) is written {depth} loop or closure levels deep (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = "accumulate in the inner scope and combine outside it, or replace the loops with a fold or collect"
                .to_string();
            out.push(Finding::at(facts, pos, INFO.id, severity, message, fix));
        }
    }
}
