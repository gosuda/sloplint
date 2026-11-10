use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{parse_options, string_values};
use crate::config::Strictness;
use crate::facts::{FileFacts, Visibility};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::text::identifier_words;

const DEFAULT_NAMES: &[&str] = &[
    "process",
    "handle",
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

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/generic-name",
    family: Family::Grep,
    summary: "public functions whose names are too generic to search for",
    why: "Generic names match unrelated code in every search.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Lower1,
        test: Shift::Off,
        support: Shift::Keep,
        bench: Shift::Off,
        example: Shift::Off,
    },
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
            Strictness::Standard | Strictness::Strict => Level::At(Severity::Minor),
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

impl FileRule for GenericName {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if function.visibility != Visibility::Exported || function.signature_forced {
                continue;
            }
            if !self.names.contains(&normalized(function.name)) {
                continue;
            }
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Major,
                format!(
                    "`{}` is a generic name shared by unrelated code",
                    function.name
                ),
                "name the item after what it holds or does".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Function, Pos};
    use crate::lang::Lang;
    use crate::role::CodeRole;

    fn facts(functions: Vec<Function<'static>>) -> FileFacts<'static> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source: "",
            line_starts: vec![0],
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 1,
            package: String::new(),
            is_generated: false,
            functions,
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn function(name: &'static str, visibility: Visibility) -> Function<'static> {
        Function {
            name,
            pos: Pos { line: 1, byte: 0 },
            doc: Vec::new(),
            doc_pos: None,
            visibility,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            receiver: None,
            end_line: 1,
            has_value_return: false,
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
            sole_call: None,
        }
    }

    fn rule() -> GenericName {
        let mut names = HashSet::new();
        for name in DEFAULT_NAMES {
            names.insert(normalized(name));
        }
        GenericName { names }
    }

    fn check(names: &GenericName, functions: Vec<Function<'static>>) -> Vec<Finding> {
        let facts = facts(functions);
        let mut out = Vec::new();
        names.check(&facts, &mut out);
        out
    }

    #[test]
    fn run_is_no_longer_generic() {
        let out = check(&rule(), vec![function("run", Visibility::Exported)]);
        assert!(out.is_empty(), "run stays silent: {out:?}");
    }

    #[test]
    fn execute_is_no_longer_generic() {
        let out = check(&rule(), vec![function("execute", Visibility::Exported)]);
        assert!(out.is_empty(), "execute stays silent: {out:?}");
    }

    #[test]
    fn restricted_visibility_stays_silent() {
        let out = check(&rule(), vec![function("handle", Visibility::Restricted)]);
        assert!(out.is_empty(), "restricted names stay silent: {out:?}");
    }

    #[test]
    fn exported_generic_still_flags() {
        let out = check(&rule(), vec![function("handle", Visibility::Exported)]);
        assert_eq!(out.len(), 1, "exported generic names flag once: {out:?}");
        assert_eq!(
            out[0].rule.as_ref(),
            INFO.id,
            "the finding carries this rule id"
        );
        assert_eq!(out[0].line, 1, "the finding sits on the definition line");
    }

    #[test]
    fn default_names_exclude_run_and_execute() {
        assert!(
            !DEFAULT_NAMES.contains(&"run"),
            "run stays out of the default list"
        );
        assert!(
            !DEFAULT_NAMES.contains(&"execute"),
            "execute stays out of the default list"
        );
    }
}
