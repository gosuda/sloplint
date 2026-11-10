use std::collections::HashMap;

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
        let mut name_counts: HashMap<&str, u32> = HashMap::new();
        for import in &facts.imports {
            *name_counts.entry(import.name).or_insert(0) += 1;
        }
        for import in &facts.imports {
            let Some(alias) = import.alias else { continue };
            if import.kind != ImportKind::Named {
                continue;
            }
            if alias == import.name
                || alias == "_"
                || !valid_ident(import.name)
                || alias.contains(import.name)
                || import.name.contains(alias)
                || name_counts.get(import.name).is_some_and(|count| *count > 1)
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
    use crate::facts::{ImportScope, Pos};
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
