//! `agent/typed-error-boundary`: public functions returning untyped errors.

use crate::config::Strictness;
use crate::facts::{FileFacts, Function};
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

use super::EmptyOptions;

const RUST_FIX: &str = "return a crate error enum (thiserror) that names each failure case";
const ZIG_FIX: &str = "declare the failure cases as an explicit error set on the return type";
const SWIFT_FIX: &str = "throw a typed error enum and declare it in the throws clause";

const RUST_MARKERS: &[&str] = &[
    "Box<dynError",
    "Box<dynstd::error::Error",
    "Box<dyncore::error::Error",
    "anyhow::",
    "eyre::",
];

/// `agent/typed-error-boundary`: flags untyped error types on public returns.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/typed-error-boundary",
    family: Family::Agent,
    summary: "public functions whose return type hides the failure cases",
    why: "An untyped error hides which failures a caller must handle.",
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
    Ok(Box::new(TypedErrorBoundary))
}

struct TypedErrorBoundary;

impl FileRule for TypedErrorBoundary {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if !matches!(facts.lang, Lang::Rust | Lang::Zig | Lang::Swift) {
            return;
        }
        for function in &facts.functions {
            if !function.public || function.in_test || function.name == "main" {
                continue;
            }
            let Some((message, fix)) = untyped_boundary(facts, function) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Warn,
                message,
                fix.to_string(),
            ));
        }
    }
}

fn untyped_boundary(
    facts: &FileFacts<'_>,
    function: &Function<'_>,
) -> Option<(String, &'static str)> {
    let name = function.name;
    if facts.lang == Lang::Swift {
        if !function.untyped_throws {
            return None;
        }
        let message = format!("public `{name}` throws an untyped error");
        return Some((message, SWIFT_FIX));
    }
    let return_type = function.return_type?;
    let fix = untyped_fix(facts, return_type)?;
    let message = format!("public `{name}` returns an untyped error (`{return_type}`)");
    Some((message, fix))
}

fn untyped_fix(facts: &FileFacts<'_>, return_type: &str) -> Option<&'static str> {
    match facts.lang {
        Lang::Rust => rust_untyped(facts, return_type).then_some(RUST_FIX),
        Lang::Zig => zig_untyped(return_type).then_some(ZIG_FIX),
        _ => None,
    }
}

fn rust_untyped(facts: &FileFacts<'_>, return_type: &str) -> bool {
    let compact: String = return_type.chars().filter(|c| !c.is_whitespace()).collect();
    RUST_MARKERS.iter().any(|marker| compact.contains(marker))
        || (compact.starts_with("Result<") && imports_result(facts))
}

fn imports_result(facts: &FileFacts<'_>) -> bool {
    facts
        .imports
        .iter()
        .any(|import| import.name == "Result" && matches!(&*import.module, "anyhow" | "eyre"))
}

fn zig_untyped(return_type: &str) -> bool {
    return_type.starts_with("anyerror!") || return_type == "anyerror"
}
