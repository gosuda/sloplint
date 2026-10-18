//! `agent/informative-assertions`: assertions without failure messages.

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

use super::EmptyOptions;

const MESSAGE: &str = "assertion without a failure message";
const RUST_COMPARISON_FIX: &str = "use assert_eq!/assert_ne! or add a message naming the invariant";
const GENERIC_FIX: &str = "add a message that names the violated invariant and the observed value";

/// `agent/informative-assertions`: flags bare assertions that say nothing on failure.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/informative-assertions",
    family: Family::Agent,
    summary: "assertions without a failure message",
    why: "A bare assertion failure says nothing about the broken invariant or the values involved.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic => Level::Warn,
        Strictness::Standard | Strictness::Strict => Level::Deny,
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
    Ok(Box::new(InformativeAssertions))
}

struct InformativeAssertions;

impl FileRule for InformativeAssertions {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for assertion in &facts.assertions {
            if assertion.has_message {
                continue;
            }
            if facts.lang == Lang::Python && assertion.comparison {
                continue;
            }
            let fix = if facts.lang == Lang::Rust && assertion.comparison {
                RUST_COMPARISON_FIX
            } else {
                GENERIC_FIX
            };
            out.push(Finding::at(
                facts,
                assertion.pos,
                INFO.id,
                Severity::Deny,
                MESSAGE.to_string(),
                fix.to_string(),
            ));
        }
    }
}
