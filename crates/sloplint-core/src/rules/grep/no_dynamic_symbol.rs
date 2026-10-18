//! `grep/no-dynamic-symbol`: symbols resolved by a name built at runtime.

use super::{EmptyOptions, parse_options};
use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

/// `grep/no-dynamic-symbol`: flags every runtime-built symbol lookup site.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-dynamic-symbol",
    family: Family::Grep,
    summary: "symbols resolved by runtime-built names",
    why: "A name built at runtime appears nowhere in the source, so search and static analysis miss it.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

struct NoDynamicSymbol;

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal => Level::Off,
            Strictness::Basic | Strictness::Standard | Strictness::Strict => Level::Warn,
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
                Severity::Deny,
                format!("{} builds a name at runtime", symbol.what),
                "dispatch through a static table that maps literal keys to functions, so every target name appears in the source"
                    .to_string(),
            ));
        }
    }
}
