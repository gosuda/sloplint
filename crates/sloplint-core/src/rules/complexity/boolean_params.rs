use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/boolean-params",
    family: Family::Complexity,
    summary: "public functions taking boolean parameters",
    why: "Boolean arguments are unreadable at the call site and double the paths per flag.",
    confidence: Confidence::High,
    roles: RolePolicy {
        entry: Shift::Lower1,
        test: Shift::Off,
        support: Shift::Lower1,
        bench: Shift::Off,
        example: Shift::Off,
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

struct BooleanParams {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 2, 3, 5, 6),
        Strictness::Basic => (Level::Off, 2, 3, 5, 6),
        Strictness::Standard => (Level::At(Severity::Minor), 2, 3, 5, 6),
        Strictness::Strict => (Level::At(Severity::Major), 1, 2, 3, 4),
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
    Ok(Box::new(BooleanParams { thresholds }))
}

impl FileRule for BooleanParams {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if !function.public || function.interface_bound {
                continue;
            }
            let flagged: Vec<&str> = function
                .params
                .iter()
                .filter(|param| param.bool_default || param.ty.is_some_and(is_boolean_type))
                .map(|param| param.name)
                .collect();
            if flagged.is_empty() {
                continue;
            }
            let count = u32::try_from(flagged.len()).unwrap_or(u32::MAX);
            let Some(severity) = self.thresholds.grade(count) else {
                continue;
            };
            let names = flagged.join(", ");
            let name = function.name;
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "`{name}` takes {count} boolean parameter(s): {names} (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = format!(
                "replace {names} with a two-variant enum or with separately named functions"
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

fn is_boolean_type(ty: &str) -> bool {
    let normalized: String = ty
        .chars()
        .filter(|c| *c != '&' && !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    matches!(normalized.as_str(), "bool" | "boolean" | "_bool" | "[bool]")
}
