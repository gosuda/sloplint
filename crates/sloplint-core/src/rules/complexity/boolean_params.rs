//! `complexity/boolean-params`: flags public functions taking boolean parameters.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

/// Catalog entry for `complexity/boolean-params`.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/boolean-params",
    family: Family::Complexity,
    summary: "public functions taking boolean parameters",
    why: "Boolean arguments are unreadable at the call site and double the paths per flag.",
    exempt_tests: true,
    preset,
    build: Build::File(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    warn_at: u32,
    deny_at: u32,
}

struct BooleanParams {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Off, 2, 3),
        Strictness::Basic => (Level::Off, 2, 3),
        Strictness::Standard => (Level::Warn, 2, 3),
        Strictness::Strict => (Level::Deny, 1, 2),
    };
    Preset {
        level,
        options: threshold_options(warn_at, deny_at),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    let thresholds = Thresholds::new(parsed.warn_at, parsed.deny_at)?;
    Ok(Box::new(BooleanParams { thresholds }))
}

impl FileRule for BooleanParams {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if !function.public || function.interface_bound || function.in_test {
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
            let message = format!("`{name}` takes {count} boolean parameter(s): {names}");
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
