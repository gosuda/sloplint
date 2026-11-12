use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{parse_options, string_values};
use crate::config::Strictness;
use crate::facts::{FileFacts, Import, ImportKind};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-rename-import",
    family: Family::Grep,
    summary: "import aliases that hide the original name from text search",
    why: "An alias removes the original name from text search at every use site.",
    confidence: Confidence::Low,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow: Vec<String>,
}

struct NoRenameImport {
    allow: Vec<String>,
}

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
            Strictness::Strict => Level::At(Severity::Info),
        },
        options: fixed_options(),
    }
}

fn fixed_options() -> toml::Table {
    let mut options = toml::Table::new();
    options.insert("allow".to_string(), string_values(&[]));
    options
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    let allow = options.allow;
    Ok(Box::new(NoRenameImport { allow }))
}

impl FileRule for NoRenameImport {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let mut everywhere: HashMap<&str, u32> = HashMap::new();
        let mut production: HashMap<&str, u32> = HashMap::new();
        for import in &facts.imports {
            *everywhere.entry(import.name).or_insert(0) += 1;
            if !import.in_test {
                *production.entry(import.name).or_insert(0) += 1;
            }
        }
        let bound = bound_names(facts);
        for import in &facts.imports {
            let Some(alias) = import.alias else { continue };
            let visible = if import.in_test {
                &everywhere
            } else {
                &production
            };
            if import.kind != ImportKind::Named {
                continue;
            }
            if alias == import.name
                || alias == "_"
                || !valid_ident(import.name)
                || alias.contains(import.name)
                || import.name.contains(alias)
                || bound.contains(import.name)
                || visible.get(import.name).is_some_and(|count| *count > 1)
                || self.allowed(import, alias)
            {
                continue;
            }
            out.push(Finding::at(
                facts,
                import.pos,
                INFO.id,
                Severity::Major,
                format!("import alias `{alias}` hides `{}` from text search", import.name),
                format!(
                    "import `{}` under its own name and qualify it with its module where names collide",
                    import.name
                ),
            ));
        }
    }
}

fn bound_names<'s>(facts: &FileFacts<'s>) -> HashSet<&'s str> {
    let mut bound: HashSet<&'s str> = facts.bindings.iter().map(|binding| binding.name).collect();
    bound.extend(facts.types.iter().map(|ty| ty.name));
    for function in &facts.functions {
        bound.insert(function.name);
        bound.extend(function.receiver);
        bound.extend(function.params.iter().map(|param| param.name));
    }
    bound
}

fn valid_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_alphabetic() {
        return false;
    }
    chars.all(|next| next == '_' || next.is_alphanumeric())
}

impl NoRenameImport {
    fn allowed(&self, import: &Import<'_>, alias: &str) -> bool {
        if self.allow.is_empty() {
            return false;
        }
        let bare = format!("{} as {alias}", import.name);
        if self.in_allow(&bare) {
            return true;
        }
        for separator in ["::", ".", "/"] {
            let qualified = format!("{}{separator}{} as {alias}", import.module, import.name);
            if self.in_allow(&qualified) {
                return true;
            }
        }
        let module = format!("{} as {alias}", import.module);
        self.in_allow(&module)
    }

    fn in_allow(&self, candidate: &str) -> bool {
        self.allow.iter().any(|entry| entry.as_str() == candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{
        Function, ImportScope, MutBinding, Param, Pos, PrimitiveKind, TypeDef, Visibility,
    };
    use crate::lang::Lang;
    use crate::role::CodeRole;
    use std::borrow::Cow;

    fn rule() -> NoRenameImport {
        let options: Options =
            parse_options(preset(Strictness::Strict).options).expect("strict preset parses");
        NoRenameImport {
            allow: options.allow,
        }
    }

    fn facts(imports: Vec<Import<'static>>) -> FileFacts<'static> {
        FileFacts {
            lang: Lang::Python,
            path: "pkg/mod.py",
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

    fn import(
        module: &'static str,
        name: &'static str,
        alias: Option<&'static str>,
    ) -> Import<'static> {
        Import {
            pos: Pos { line: 1, byte: 0 },
            module: Cow::Borrowed(module),
            name,
            alias,
            kind: ImportKind::Named,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        }
    }

    fn check(rule: &NoRenameImport, imports: Vec<Import<'static>>) -> Vec<Finding> {
        let facts = facts(imports);
        let mut out = Vec::new();
        rule.check(&facts, &mut out);
        out
    }

    fn in_test(mut import: Import<'static>) -> Import<'static> {
        import.in_test = true;
        import
    }

    fn run(facts: &FileFacts<'_>) -> Vec<Finding> {
        let mut out = Vec::new();
        rule().check(facts, &mut out);
        out
    }

    fn binding(name: &'static str) -> MutBinding<'static> {
        MutBinding {
            name,
            pos: Pos { line: 4, byte: 0 },
            last_write_line: 4,
            first_write_line: 4,
            write_lines: 0,
            loop_depth: 0,
        }
    }

    fn param(name: &'static str) -> Param<'static> {
        Param {
            name,
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        }
    }

    fn function(name: &'static str, params: Vec<Param<'static>>) -> Function<'static> {
        Function {
            name,
            pos: Pos { line: 3, byte: 0 },
            doc: Vec::new(),
            doc_pos: None,
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            receiver: None,
            params,
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
            end_line: 5,
            has_value_return: false,
            sole_call: None,
        }
    }

    fn renamed_runsc() -> Import<'static> {
        import("example.com/shim/runsc", "runsc", Some("rsc"))
    }

    #[test]
    fn alias_of_a_name_the_file_binds_locally_stays_silent() {
        let mut file = facts(vec![renamed_runsc()]);
        file.bindings.push(binding("runsc"));
        let out = run(&file);
        assert!(out.is_empty(), "a local `runsc` needs the alias: {out:?}");
    }

    #[test]
    fn alias_of_a_name_the_file_takes_as_a_parameter_stays_silent() {
        let mut file = facts(vec![renamed_runsc()]);
        file.functions
            .push(function("Start", vec![param("ctx"), param("runsc")]));
        let out = run(&file);
        assert!(
            out.is_empty(),
            "a `runsc` parameter needs the alias: {out:?}"
        );
    }

    #[test]
    fn alias_of_a_name_the_file_declares_as_a_function_stays_silent() {
        let mut file = facts(vec![renamed_runsc()]);
        file.functions.push(function("runsc", Vec::new()));
        let out = run(&file);
        assert!(
            out.is_empty(),
            "a `runsc` function needs the alias: {out:?}"
        );
    }

    #[test]
    fn alias_of_a_name_the_file_declares_as_a_type_stays_silent() {
        let mut file = facts(vec![renamed_runsc()]);
        file.types.push(TypeDef {
            name: "runsc",
            pos: Pos { line: 3, byte: 0 },
            visibility: Visibility::Private,
            abstraction: false,
        });
        let out = run(&file);
        assert!(out.is_empty(), "a `runsc` type needs the alias: {out:?}");
    }

    #[test]
    fn alias_beside_bindings_of_other_names_still_flags() {
        let mut file = facts(vec![renamed_runsc()]);
        file.bindings.push(binding("service"));
        file.functions.push(function("New", vec![param("ctx")]));
        assert_eq!(run(&file).len(), 1, "no local `runsc` exists");
    }

    #[test]
    fn a_test_import_of_the_same_name_does_not_excuse_a_production_alias() {
        let imports = vec![
            import("crate::parser", "Parser", Some("Analyzer")),
            in_test(import("crate::fixtures", "Parser", None)),
        ];
        assert_eq!(check(&rule(), imports).len(), 1);
    }

    #[test]
    fn a_test_alias_beside_a_production_import_of_the_name_stays_silent() {
        let imports = vec![
            import("crate::parser", "Parser", None),
            in_test(import("crate::fixtures", "Parser", Some("Fake"))),
        ];
        assert_eq!(check(&rule(), imports).len(), 0);
    }

    #[test]
    fn identity_alias_stays_silent() {
        let out = check(&rule(), vec![import("os", "path", Some("path"))]);
        assert!(out.is_empty(), "identity aliases stay silent: {out:?}");
    }

    #[test]
    fn containment_alias_stays_silent() {
        let out = check(
            &rule(),
            vec![import("simplejson", "simplejson", Some("json"))],
        );
        assert!(out.is_empty(), "containment aliases stay silent: {out:?}");
    }

    #[test]
    fn underscore_alias_stays_silent() {
        let out = check(&rule(), vec![import("gettext", "gettext", Some("_"))]);
        assert!(out.is_empty(), "underscore aliases stay silent: {out:?}");
    }

    #[test]
    fn non_identifier_target_stays_silent() {
        let out = check(&rule(), vec![import("pkg", "foo-bar", Some("fb"))]);
        assert!(
            out.is_empty(),
            "non-identifier targets stay silent: {out:?}"
        );
    }

    #[test]
    fn true_rename_still_flags() {
        let out = check(&rule(), vec![import("numpy", "numpy", Some("np"))]);
        assert_eq!(out.len(), 1, "strict flags a true rename once: {out:?}");
        assert!(
            out[0].message.contains("`np` hides `numpy`"),
            "the finding names the alias and the hidden import: {}",
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
        let out = check(&rule(), vec![import("numpy", "numpy", Some("np"))]);
        assert_eq!(out.len(), 1, "strict flags a true rename once: {out:?}");
    }
}
