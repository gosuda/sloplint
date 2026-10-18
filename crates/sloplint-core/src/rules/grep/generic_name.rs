//! `grep/generic-name`: public functions whose names are too generic to search for.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{parse_options, string_values};
use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::text::identifier_words;

const DEFAULT_NAMES: &[&str] = &[
    "process",
    "handle",
    "execute",
    "run",
    "manage",
    "perform",
    "do_work",
    "do_it",
    "get_data",
    "parse_data",
    "run_task",
    "process_data",
    "handle_data",
    "helper",
    "util",
];

/// `grep/generic-name`: flags public functions whose normalized name lands in the generic list.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/generic-name",
    family: Family::Grep,
    summary: "public functions whose names are too generic to search for",
    why: "Generic names match unrelated code in every search.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    names: Vec<String>,
}

struct GenericName {
    names: HashSet<String>,
}

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal | Strictness::Basic => Level::Off,
            Strictness::Standard | Strictness::Strict => Level::Warn,
        },
        options: fixed_options(),
    }
}

fn fixed_options() -> toml::Table {
    let mut options = toml::Table::new();
    options.insert("names".to_string(), string_values(DEFAULT_NAMES));
    options
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    let mut names = HashSet::new();
    for name in &options.names {
        names.insert(normalized(name));
    }
    Ok(Box::new(GenericName { names }))
}

fn normalized(name: &str) -> String {
    identifier_words(name).collect::<Vec<_>>().join("_")
}

fn fix_for(lang: Lang, name: &str) -> String {
    if snake_case(lang) {
        format!("rename `{name}` to `{name}_<noun>`, naming the domain object it acts on")
    } else {
        format!("rename `{name}` to `{name}<Noun>`, naming the domain object it acts on")
    }
}

fn snake_case(lang: Lang) -> bool {
    matches!(
        lang,
        Lang::Rust
            | Lang::Python
            | Lang::C
            | Lang::Cpp
            | Lang::Ruby
            | Lang::Bash
            | Lang::R
            | Lang::Zig
            | Lang::Ocaml
            | Lang::OcamlInterface
            | Lang::Odin
            | Lang::Starlark
    )
}

impl FileRule for GenericName {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if !function.public || function.interface_bound {
                continue;
            }
            if !self.names.contains(&normalized(function.name)) {
                continue;
            }
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Deny,
                format!(
                    "`{}` is a generic name shared by unrelated code",
                    function.name
                ),
                fix_for(facts.lang, function.name),
            ));
        }
    }
}
