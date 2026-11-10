use crate::config::Strictness;

use crate::facts::{FileFacts, Function, Import, ImportKind};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

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

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/typed-error-boundary",
    family: Family::Agent,
    summary: "public functions whose return type hides the failure cases",
    why: "An untyped error hides which failures a caller must handle.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Off,
        test: Shift::Off,
        support: Shift::Off,
        bench: Shift::Off,
        example: Shift::Off,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = parse_options(options)?;
    Ok(Box::new(TypedErrorBoundary))
}

struct TypedErrorBoundary;

impl FileRule for TypedErrorBoundary {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if !matches!(facts.lang, Lang::Rust | Lang::Zig | Lang::Swift) {
            return;
        }
        for function in &facts.functions {
            if !function.exported() || function.entry_point {
                continue;
            }
            let Some((message, fix)) = untyped_boundary(facts, function) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Minor,
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
        Lang::Rust => rust_untyped(&facts.imports, return_type).then_some(RUST_FIX),
        Lang::Zig => zig_untyped(return_type).then_some(ZIG_FIX),
        _ => None,
    }
}

fn rust_untyped(imports: &[Import<'_>], return_type: &str) -> bool {
    let compact: String = return_type.chars().filter(|c| !c.is_whitespace()).collect();
    if RUST_MARKERS.iter().any(|marker| compact.contains(marker)) {
        return true;
    }
    let Some((head, inner)) = split_head(&compact) else {
        return false;
    };
    if top_level_commas(inner) != 0 {
        return false;
    }
    resolves_anyhow_result(imports, head)
}

fn split_head(compact: &str) -> Option<(&str, &str)> {
    let (head, rest) = compact.split_once('<')?;
    let inner = rest.strip_suffix('>')?;
    let head = head.rsplit("::").next()?;
    if head.is_empty() || !head.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return None;
    }
    Some((head, inner))
}

fn resolves_anyhow_result(imports: &[Import<'_>], head: &str) -> bool {
    imports.iter().any(|import| {
        let root = import.external_root.or_else(|| {
            let module: &str = &import.module;
            (!module.contains("::")).then_some(module)
        });
        let Some(root) = root else {
            return false;
        };
        if root != "anyhow" && root != "eyre" {
            return false;
        }
        if import.kind == ImportKind::Wildcard {
            return head == "Result";
        }
        if import.name != "Result" {
            return false;
        }
        match import.alias {
            None => head == "Result",
            Some(alias) => alias == head,
        }
    })
}

fn top_level_commas(inner: &str) -> usize {
    let mut angles = 0u32;
    let mut parens = 0u32;
    let mut squares = 0u32;
    let mut commas = 0usize;
    for token in inner.chars() {
        match token {
            '<' => angles += 1,
            '>' => angles = angles.saturating_sub(1),
            '(' => parens += 1,
            ')' => parens = parens.saturating_sub(1),
            '[' => squares += 1,
            ']' => squares = squares.saturating_sub(1),
            ',' if angles == 0 && parens == 0 && squares == 0 => commas += 1,
            _ => {}
        }
    }
    commas
}

fn zig_untyped(return_type: &str) -> bool {
    let trimmed = return_type.trim_start();
    trimmed.starts_with("anyerror!") || trimmed == "anyerror" || trimmed.starts_with('!')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Pos, Visibility};
    use crate::finding::line_starts;
    use std::borrow::Cow;

    fn facts<'s>(
        lang: Lang,
        source: &'s str,
        functions: Vec<Function<'s>>,
        imports: Vec<Import<'s>>,
    ) -> FileFacts<'s> {
        FileFacts {
            lang,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count: 0,
            package: String::new(),
            is_generated: false,
            functions,
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

    fn function<'s>(
        name: &'s str,
        visibility: Visibility,
        return_type: Option<&'s str>,
    ) -> Function<'s> {
        Function {
            name,
            pos: Pos { line: 5, byte: 0 },
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
            params: Vec::new(),
            return_type,
            untyped_throws: false,
            has_value_return: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 8,
            end_line: 8,
            sole_call: None,
        }
    }

    fn import<'s>(module: &'s str, name: &'s str, alias: Option<&'s str>) -> Import<'s> {
        Import {
            pos: Pos { line: 1, byte: 0 },
            module: Cow::Borrowed(module),
            name,
            alias,
            kind: crate::facts::ImportKind::Named,
            scope: crate::facts::ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        }
    }

    fn lines(
        rule: &TypedErrorBoundary,
        lang: Lang,
        functions: Vec<Function<'_>>,
        imports: Vec<Import<'_>>,
    ) -> Vec<String> {
        let source = "fn f() {}\n";
        let mut out = Vec::new();
        rule.check(&facts(lang, source, functions, imports), &mut out);
        out.into_iter().map(|finding| finding.message).collect()
    }

    #[test]
    fn alias_and_arity_narrow_the_rust_boundary() {
        let rule = TypedErrorBoundary;
        let anyhow = import("anyhow", "Result", None);
        let source = "fn f() {}\n";
        let mut out = Vec::new();
        rule.check(
            &facts(
                Lang::Rust,
                source,
                vec![
                    function(
                        "load",
                        Visibility::Exported,
                        Some("Result<Config,ApplyError>"),
                    ),
                    function("quiet", Visibility::Restricted, Some("Result<()>")),
                ],
                vec![anyhow],
            ),
            &mut out,
        );
        assert!(out.is_empty());
        let found = lines(
            &rule,
            Lang::Rust,
            vec![
                function(
                    "load",
                    Visibility::Exported,
                    Some("Result<Config,Box<dyn std::error::Error>>"),
                ),
                function("fetch", Visibility::Exported, Some("Result<Config>")),
            ],
            vec![import("anyhow", "Result", None)],
        );
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn aliased_result_and_typed_zig_stay_silent_while_inferred_set_flags() {
        let rule = TypedErrorBoundary;
        let found = lines(
            &rule,
            Lang::Rust,
            vec![function(
                "load",
                Visibility::Exported,
                Some("Result<Config,ApplyError>"),
            )],
            vec![import("anyhow", "Result", Some("AnyhowResult"))],
        );
        assert!(found.is_empty());
        let found = lines(
            &rule,
            Lang::Zig,
            vec![
                function("ok", Visibility::Exported, Some("LoadError!Config")),
                function("load", Visibility::Exported, Some("!Config")),
            ],
            Vec::new(),
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn tuple_argument_and_aliased_head_flag() {
        let rule = TypedErrorBoundary;
        let found = lines(
            &rule,
            Lang::Rust,
            vec![function(
                "parse",
                Visibility::Exported,
                Some("Result<(String, usize)>"),
            )],
            vec![import("anyhow", "Result", None)],
        );
        assert_eq!(found.len(), 1);
        let found = lines(
            &rule,
            Lang::Rust,
            vec![function(
                "load",
                Visibility::Exported,
                Some("AnyhowResult<Config>"),
            )],
            vec![import("anyhow", "Result", Some("AnyhowResult"))],
        );
        assert_eq!(found.len(), 1);
        let found = lines(
            &rule,
            Lang::Rust,
            vec![function(
                "load",
                Visibility::Exported,
                Some("Result<Config>"),
            )],
            vec![Import {
                pos: Pos { line: 1, byte: 0 },
                module: Cow::Borrowed("anyhow"),
                name: "*",
                alias: None,
                kind: crate::facts::ImportKind::Wildcard,
                scope: crate::facts::ImportScope::Module,
                in_test: false,
                is_reexport: false,
                external_root: Some("anyhow"),
            }],
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn support_region_functions_flag_for_role_lowering() {
        let rule = TypedErrorBoundary;
        let mut load = function(
            "load",
            Visibility::Exported,
            Some("Result<Config,Box<dyn std::error::Error>>"),
        );
        load.in_test = true;
        let found = lines(&rule, Lang::Rust, vec![load], Vec::new());
        assert_eq!(found.len(), 1);
    }
    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "typed-error-boundary ships at low confidence"
        );
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
        let strict_preset = preset(Strictness::Strict);
        assert_eq!(
            strict_preset.level,
            Level::At(Severity::Info),
            "strict reports at info"
        );
        let rule =
            build(strict_preset.options, &Settings::default()).expect("strict preset builds");
        let load = function(
            "load",
            Visibility::Exported,
            Some("Result<Config,Box<dyn std::error::Error>>"),
        );
        let mut out = Vec::new();
        rule.check(
            &facts(Lang::Rust, "fn f() {}\n", vec![load], Vec::new()),
            &mut out,
        );
        assert_eq!(
            out.len(),
            1,
            "strict flags the untyped boundary on a positive input"
        );
    }
}
