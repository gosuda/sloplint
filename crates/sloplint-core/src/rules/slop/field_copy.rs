use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, parse_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/field-copy",
    family: Family::Slop,
    summary: "struct literals copying every field one-to-one from another value",
    why: "One-to-one field copying means two types carry the same data.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Off,
        support: Shift::Lower1,
        bench: Shift::Off,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let (level, min_fields) = match strictness {
        Strictness::Minimal | Strictness::Basic => (Level::Off, 6),
        Strictness::Standard => (Level::At(Severity::Minor), 6),
        Strictness::Strict => (Level::At(Severity::Minor), 4),
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
    let options: Options = parse_options(options)?;
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
                Severity::Minor,
                format!("{} fields copied one-to-one from `{source}`", copy.fields),
                "if both types carry the same data, merge them into one type; if this copy is the deliberate conversion, keep it and suppress with a reason".to_string(),
            ));
        }
    }
}
