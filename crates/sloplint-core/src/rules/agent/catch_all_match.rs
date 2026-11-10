use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::EmptyOptions;
use crate::rules::parse_options;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

const MESSAGE: &str = "wildcard arm in a match over enum-like patterns";
const FIX: &str =
    "list the remaining variants explicitly so a new variant fails the exhaustiveness check here";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/catch-all-match",
    family: Family::Agent,
    summary: "wildcard match arms over enum-like patterns",
    why: "A wildcard arm hides new variants from exhaustiveness checking.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic | Strictness::Standard | Strictness::Strict => Level::At(Severity::Minor),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = parse_options(options)?;
    Ok(Box::new(CatchAllMatch))
}

struct CatchAllMatch;

impl FileRule for CatchAllMatch {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for arm in &facts.wildcard_arms {
            if !arm.enum_like {
                continue;
            }
            out.push(Finding::at(
                facts,
                arm.pos,
                INFO.id,
                Severity::Minor,
                MESSAGE.to_string(),
                FIX.to_string(),
            ));
        }
    }
}
