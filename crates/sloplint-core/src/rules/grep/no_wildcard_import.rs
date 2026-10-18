//! `grep/no-wildcard-import`: wildcard imports that hide where each name comes from.

use super::{EmptyOptions, parse_options};
use crate::config::Strictness;
use crate::facts::{FileFacts, Import, ImportKind};
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

/// `grep/no-wildcard-import`: flags imports that bring every name of a module into scope.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-wildcard-import",
    family: Family::Grep,
    summary: "wildcard imports that hide where each name comes from",
    why: "A wildcard import hides where each name comes from.",
    exempt_tests: true,
    preset,
    build: Build::File(build),
};

struct NoWildcardImport;

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
    Ok(Box::new(NoWildcardImport))
}

impl FileRule for NoWildcardImport {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for import in &facts.imports {
            if import.kind != ImportKind::Wildcard || self.exempted(facts.lang, import) {
                continue;
            }
            out.push(Finding::at(
                facts,
                import.pos,
                INFO.id,
                Severity::Deny,
                format!("wildcard import from `{}`", import.module),
                "list the imported names explicitly".to_string(),
            ));
        }
    }
}

impl NoWildcardImport {
    fn exempted(&self, lang: Lang, import: &Import<'_>) -> bool {
        if lang != Lang::Rust {
            return false;
        }
        let module = import.module.as_ref();
        let prelude = module == "prelude" || module.ends_with("::prelude");
        let test_super = (module == "super" || module.ends_with("::super")) && import.in_test;
        prelude || test_super
    }
}
