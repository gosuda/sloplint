use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{CodeRole, RolePolicy, Shift};
use crate::rules::{
    Build, EmptyOptions, Family, FileRule, Preset, RuleInfo, Settings, parse_options,
};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/no-sleep-in-tests",
    family: Family::Agent,
    summary: "wall-clock sleeps in tests",
    why: "A fixed delay neither proves completion nor adapts to scheduler load.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Off,
        test: Shift::Keep,
        support: Shift::Keep,
        bench: Shift::Off,
        example: Shift::Off,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = parse_options(options)?;
    Ok(Box::new(NoSleepInTests))
}

struct NoSleepInTests;

impl FileRule for NoSleepInTests {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for &pos in &facts.blocking_sleeps {
            if !matches!(facts.role_at(pos.line), CodeRole::Test | CodeRole::Support) {
                continue;
            }
            out.push(Finding::at(
                facts,
                pos,
                INFO.id,
                Severity::Minor,
                "wall-clock sleep in test code".to_string(),
                "wait for an observable condition or completion signal with a bounded timeout; use virtual time for timer behavior".to_string(),
            ));
        }
    }
}
