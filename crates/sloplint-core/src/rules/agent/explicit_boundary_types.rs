use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

const APPLICABLE: &[Lang] = &[
    Lang::TypeScript,
    Lang::Tsx,
    Lang::Python,
    Lang::Kotlin,
    Lang::Scala,
    Lang::Dart,
];

const FIX: &str = "annotate every parameter and the return type";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/explicit-boundary-types",
    family: Family::Agent,
    summary: "public functions with unannotated parameters or return types",
    why: "Unannotated public signatures force callers to read the body to learn the contract.",
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
    Ok(Box::new(ExplicitBoundaryTypes))
}

struct ExplicitBoundaryTypes;

impl FileRule for ExplicitBoundaryTypes {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if !APPLICABLE.contains(&facts.lang) {
            return;
        }
        for function in &facts.functions {
            if !function.public || function.missing_annotations.is_empty() {
                continue;
            }
            let missing = function.missing_annotations.join(", ");
            let name = function.name;
            let message = format!("public `{name}` lacks type annotations for: {missing}");
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Minor,
                message,
                FIX.to_string(),
            ));
        }
    }
}
