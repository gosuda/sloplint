use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

const MESSAGE: &str = "assertion without a failure message";
const RUST_COMPARISON_FIX: &str = "use assert_eq!/assert_ne! or add a message naming the invariant";
const GENERIC_FIX: &str = "add a message that names the violated invariant and the observed value";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/informative-assertions",
    family: Family::Agent,
    summary: "assertions without a failure message",
    why: "A bare assertion failure says nothing about the broken invariant or the values involved.",
    confidence: Confidence::High,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Keep,
        support: Shift::Keep,
        bench: Shift::Off,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic => Level::At(Severity::Minor),
        Strictness::Standard | Strictness::Strict => Level::At(Severity::Major),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = parse_options(options)?;
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
                Severity::Major,
                MESSAGE.to_string(),
                fix.to_string(),
            ));
        }
    }
}
