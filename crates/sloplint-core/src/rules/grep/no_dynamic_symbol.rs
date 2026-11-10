use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-dynamic-symbol",
    family: Family::Grep,
    summary: "symbols resolved by runtime-built names",
    why: "A name built at runtime appears nowhere in the source, so search and static analysis miss it.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Keep,
    },
    preset,
    build: Build::File(build),
};

struct NoDynamicSymbol;

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal => Level::Off,
            Strictness::Basic | Strictness::Standard | Strictness::Strict => {
                Level::At(Severity::Minor)
            }
        },
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    parse_options::<EmptyOptions>(options)?;
    Ok(Box::new(NoDynamicSymbol))
}

impl FileRule for NoDynamicSymbol {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for symbol in &facts.dynamic_symbols {
            out.push(Finding::at(
                facts,
                symbol.pos,
                INFO.id,
                Severity::Major,
                format!("{} builds a name at runtime", symbol.what),
                "dispatch through a static table that maps literal keys to functions, so every target name appears in the source"
                    .to_string(),
            ));
        }
    }
}
