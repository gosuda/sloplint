use super::{EmptyOptions, parse_options};
use crate::config::Strictness;
use crate::facts::{FileFacts, Import, ImportKind, ImportScope};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-wildcard-import",
    family: Family::Grep,
    summary: "wildcard imports that hide where each name comes from",
    why: "A wildcard import hides where each name comes from.",
    confidence: Confidence::Low,
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

struct NoWildcardImport;

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
            Strictness::Strict => Level::At(Severity::Info),
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
            if import.kind != ImportKind::Wildcard
                || import.scope != ImportScope::Module
                || prelude_segment(import)
                || self_segment(import)
            {
                continue;
            }
            out.push(Finding::at(
                facts,
                import.pos,
                INFO.id,
                Severity::Major,
                format!("wildcard import from `{}`", import.module),
                "list the imported names explicitly".to_string(),
            ));
        }
    }
}

fn prelude_segment(import: &Import<'_>) -> bool {
    import
        .module
        .split([':', '.'])
        .any(|segment| segment == "prelude" || segment.ends_with("_prelude"))
}

fn self_segment(import: &Import<'_>) -> bool {
    let module = import.module.as_ref();
    module == "self" || module.starts_with("self::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Pos;
    use crate::lang::Lang;
    use crate::role::CodeRole;
    use std::borrow::Cow;

    fn rule() -> NoWildcardImport {
        NoWildcardImport
    }

    fn facts(imports: Vec<Import<'static>>) -> FileFacts<'static> {
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
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports,
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

    fn import(module: &'static str, scope: ImportScope) -> Import<'static> {
        Import {
            pos: Pos { line: 1, byte: 0 },
            module: Cow::Borrowed(module),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            scope,
            in_test: false,
            is_reexport: false,
            external_root: None,
        }
    }

    fn check(rule: &NoWildcardImport, imports: Vec<Import<'static>>) -> Vec<Finding> {
        let facts = facts(imports);
        let mut out = Vec::new();
        rule.check(&facts, &mut out);
        out
    }

    #[test]
    fn prelude_glob_stays_silent() {
        let out = check(
            &rule(),
            vec![import("crate::alloc_prelude", ImportScope::Module)],
        );
        assert!(out.is_empty(), "prelude globs stay silent: {out:?}");
    }

    #[test]
    fn function_local_glob_stays_silent() {
        let out = check(
            &rule(),
            vec![import("other_crate", ImportScope::FunctionLocal)],
        );
        assert!(out.is_empty(), "function-local globs stay silent: {out:?}");
    }

    #[test]
    fn self_glob_stays_silent() {
        let out = check(&rule(), vec![import("self::util", ImportScope::Module)]);
        assert!(out.is_empty(), "self globs stay silent: {out:?}");
    }

    #[test]
    fn foreign_glob_still_flags() {
        let out = check(&rule(), vec![import("other_crate", ImportScope::Module)]);
        assert_eq!(out.len(), 1, "foreign globs flag once: {out:?}");
        assert!(
            out[0].message.contains("other_crate"),
            "the finding names the module: {}",
            out[0].message
        );
    }

    #[test]
    fn off_at_standard_info_at_strict() {
        assert_eq!(
            preset(Strictness::Minimal).level,
            Level::Off,
            "minimal stays off"
        );
        assert_eq!(
            preset(Strictness::Basic).level,
            Level::Off,
            "basic stays off"
        );
        assert_eq!(
            preset(Strictness::Standard).level,
            Level::Off,
            "standard stays off"
        );
        assert_eq!(
            preset(Strictness::Strict).level,
            Level::At(Severity::Info),
            "strict stays informational"
        );
        assert_eq!(INFO.confidence, Confidence::Low, "confidence stays low");
        let strict = build(preset(Strictness::Strict).options, &Settings::default())
            .expect("strict preset builds");
        let facts = facts(vec![import("other_crate", ImportScope::Module)]);
        let mut out = Vec::new();
        strict.check(&facts, &mut out);
        assert_eq!(out.len(), 1, "strict flags a foreign glob once: {out:?}");
    }
}
