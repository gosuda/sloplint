//! `agent/explicit-boundary-types`: public signatures missing type annotations.

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

use super::EmptyOptions;

const APPLICABLE: &[Lang] = &[
    Lang::TypeScript,
    Lang::Tsx,
    Lang::JavaScript,
    Lang::Python,
    Lang::Kotlin,
    Lang::Scala,
    Lang::Swift,
    Lang::Dart,
    Lang::CSharp,
];

const FIX: &str = "annotate every parameter and the return type";

/// `agent/explicit-boundary-types`: flags public signatures lacking annotations.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/explicit-boundary-types",
    family: Family::Agent,
    summary: "public functions with unannotated parameters or return types",
    why: "Unannotated public signatures force callers to read the body to learn the contract.",
    exempt_tests: true,
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
    Ok(Box::new(ExplicitBoundaryTypes))
}

struct ExplicitBoundaryTypes;

impl FileRule for ExplicitBoundaryTypes {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if !APPLICABLE.contains(&facts.lang) {
            return;
        }
        for function in &facts.functions {
            if !function.public || function.in_test || function.missing_annotations.is_empty() {
                continue;
            }
            let missing = function.missing_annotations.join(", ");
            let name = function.name;
            let message = format!("public `{name}` lacks type annotations for: {missing}");
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Warn,
                message,
                FIX.to_string(),
            ));
        }
    }
}
