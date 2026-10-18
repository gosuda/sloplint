//! `slop/field-copy`: struct literals copying every field one-to-one from another value.

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

/// `slop/field-copy`: a literal that mirrors another value field for field.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/field-copy",
    family: Family::Slop,
    summary: "struct literals copying every field one-to-one from another value",
    why: "One-to-one field copying means two types carry the same data.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let (level, min_fields) = match strictness {
        Strictness::Minimal | Strictness::Basic => (Level::Off, 6),
        Strictness::Standard => (Level::Warn, 6),
        Strictness::Strict => (Level::Warn, 4),
    };
    Preset {
        level,
        options: options(min_fields),
    }
}

fn options(min_fields: u32) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert(
        "min_fields".to_string(),
        toml::Value::Integer(i64::from(min_fields)),
    );
    table
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    if options.min_fields < 2 {
        return Err(format!(
            "min_fields {} must be at least 2",
            options.min_fields
        ));
    }
    Ok(Box::new(FieldCopy {
        min_fields: options.min_fields,
    }))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    min_fields: u32,
}

struct FieldCopy {
    min_fields: u32,
}

impl FileRule for FieldCopy {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for copy in &facts.field_copies {
            let Some(source) = copy.source else {
                continue;
            };
            if copy.fields < self.min_fields {
                continue;
            }
            out.push(Finding::at(
                facts,
                copy.pos,
                INFO.id,
                Severity::Warn,
                format!("{} fields copied one-to-one from `{source}`", copy.fields),
                "if both types carry the same data, merge them into one type; if this copy is the deliberate conversion, keep it and suppress with a reason".to_string(),
            ));
        }
    }
}
