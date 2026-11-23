use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::RolePolicy;
use crate::rules::{
    Build, EmptyOptions, Family, FileRule, Preset, RuleInfo, Settings, parse_options,
};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/redundant-else",
    family: Family::Slop,
    summary: "else blocks after unconditional exits",
    why: "An else after an unconditional exit adds nesting without separating reachable paths.",
    confidence: Confidence::Low,
    roles: RolePolicy::KEEP,
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
            Strictness::Strict => Level::At(Severity::Info),
        },
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    parse_options::<EmptyOptions>(options)?;
    Ok(Box::new(RedundantElse))
}

struct RedundantElse;

impl FileRule for RedundantElse {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for &pos in &facts.redundant_elses {
            out.push(Finding::at(
                facts,
                pos,
                INFO.id,
                Severity::Minor,
                "`else` follows a branch that already exits".to_string(),
                "remove `else` and dedent its body, preserving any nested block scopes".to_string(),
            ));
        }
    }
}
