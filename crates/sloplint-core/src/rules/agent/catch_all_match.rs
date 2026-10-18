//! `agent/catch-all-match`: wildcard arms over enum-like patterns.

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

use super::EmptyOptions;

const MESSAGE: &str = "wildcard arm in a match over enum-like patterns";
const FIX: &str =
    "list the remaining variants explicitly so a new variant fails the exhaustiveness check here";

/// `agent/catch-all-match`: flags `_` arms that hide new enum variants.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/catch-all-match",
    family: Family::Agent,
    summary: "wildcard match arms over enum-like patterns",
    why: "A wildcard arm hides new variants from exhaustiveness checking.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic | Strictness::Standard | Strictness::Strict => Level::Warn,
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
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
                Severity::Warn,
                MESSAGE.to_string(),
                FIX.to_string(),
            ));
        }
    }
}
