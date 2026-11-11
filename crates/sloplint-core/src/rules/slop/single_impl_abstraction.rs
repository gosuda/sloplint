use std::collections::HashMap;

use crate::config::Strictness;
use crate::facts::NamedSite;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule, parse_options};

const APPLICABLE: &[Lang] = &[
    Lang::Rust,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::JavaScript,
    Lang::Python,
    Lang::Kotlin,
    Lang::Scala,
    Lang::Swift,
    Lang::Zig,
    Lang::Cpp,
    Lang::Java,
    Lang::CSharp,
    Lang::Dart,
];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/single-impl-abstraction",
    family: Family::Slop,
    summary: "abstractions with exactly one implementation",
    why: "An interface with one implementation is indirection without variation.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Off,
        support: Shift::Keep,
        bench: Shift::Off,
        example: Shift::Off,
    },
    preset,
    build: Build::World(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic => Level::Off,
        Strictness::Standard | Strictness::Strict => Level::At(Severity::Minor),
    };
    Preset {
        level,
        options: fixed_options(),
    }
}

fn fixed_options() -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("allow".to_string(), toml::Value::Array(Vec::new()));
    table
}

fn build(options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    let options: Options = parse_options(options)?;
    Ok(Box::new(SingleImplAbstraction {
        allow: options.allow,
    }))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow: Vec<String>,
}

struct SingleImplAbstraction {
    allow: Vec<String>,
}

impl WorldRule for SingleImplAbstraction {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        let abstractions = abstraction_sites(files);
        let impls = impl_index(files);
        for group in abstractions.chunk_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2) {
            if let [(lang, package, name, path, site)] = *group
                && !self.allowed(name)
                && let Some((impl_path, impl_site)) = sole_impl(&impls, (lang, package, name))
            {
                out.push(Finding::at_site(
                        path,
                        site,
                        INFO.id,
                        Severity::Minor,
                        format!(
                            "`{name}` has exactly one implementation ({impl_path}:{})",
                            impl_site.line
                        ),
                        format!(
                            "use the concrete type directly and delete `{name}`; add the abstraction when a second implementation exists"
                        ),
                ));
            }
        }
    }
}

type Sites<'a> = Vec<(Lang, &'a str, &'a str, &'a str, &'a NamedSite)>;

fn applicable_files<'f, 'a>(files: &'f [WorldFile<'a>]) -> impl Iterator<Item = &'f WorldFile<'a>> {
    files
        .iter()
        .filter(|file| APPLICABLE.contains(&file.facts.lang))
}

fn abstraction_sites<'a>(files: &[WorldFile<'a>]) -> Sites<'a> {
    let mut sites: Sites<'a> = Vec::new();
    for file in applicable_files(files) {
        for site in &file.facts.abstractions {
            if !INFO
                .roles
                .eligible(file.facts.role, &file.facts.regions, site.line)
            {
                continue;
            }
            sites.push((
                file.facts.lang.namespace(),
                file.facts.package.as_str(),
                site.name.as_ref(),
                file.path,
                site,
            ));
        }
    }
    sites.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)).then(a.2.cmp(b.2)));
    sites
}

struct ImplSites<'a> {
    first: (&'a str, &'a NamedSite),
    repeated: bool,
}

type Impls<'a> = HashMap<(Lang, &'a str, &'a str), ImplSites<'a>>;

fn impl_index<'a>(files: &[WorldFile<'a>]) -> Impls<'a> {
    let mut index: Impls<'a> = HashMap::new();
    for file in applicable_files(files) {
        for site in &file.facts.impls {
            index
                .entry((
                    file.facts.lang.namespace(),
                    file.facts.package.as_str(),
                    site.name.as_ref(),
                ))
                .and_modify(|sites| sites.repeated = true)
                .or_insert(ImplSites {
                    first: (file.path, site),
                    repeated: false,
                });
        }
    }
    index
}

fn sole_impl<'a>(
    impls: &Impls<'a>,
    key: (Lang, &'a str, &'a str),
) -> Option<(&'a str, &'a NamedSite)> {
    impls
        .get(&key)
        .filter(|sites| !sites.repeated)
        .map(|sites| sites.first)
}

impl SingleImplAbstraction {
    fn allowed(&self, name: &str) -> bool {
        self.allow.iter().any(|entry| entry == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{FileFacts, ImplRef, Pos, TypeDef, Visibility, WorldFacts};
    use crate::finding::line_starts;

    fn named(name: &str) -> NamedSite {
        NamedSite {
            name: name.into(),
            line: 1,
            column: 1,
        }
    }

    fn facts(package: &str, abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
        WorldFacts {
            lang: Lang::Rust,
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            package: package.to_string(),
            package_from_manifest: false,
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions,
            impls,
            barrel: None,
            imports: Vec::new(),
        }
    }

    fn same_package(abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
        facts("pkg", abstractions, impls)
    }

    fn test_double(abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
        let mut double = facts("pkg", abstractions, impls);
        double.role = crate::role::CodeRole::Test;
        double
    }

    fn reported(files: &[WorldFile<'_>]) -> Vec<Finding> {
        let rule = SingleImplAbstraction { allow: Vec::new() };
        let mut out = Vec::new();
        rule.check(files, &mut out);
        out
    }

    #[test]
    fn singletons_emit_in_name_order_and_duplicates_stay_silent() {
        let rule = SingleImplAbstraction { allow: Vec::new() };
        let zeta = same_package(vec![named("Zeta")], vec![named("Zeta")]);
        let alpha = same_package(vec![named("Alpha")], vec![named("Alpha")]);
        let dup_first = same_package(vec![named("Dup")], vec![named("Dup")]);
        let dup_second = same_package(vec![named("Dup")], Vec::new());
        let pair = same_package(vec![named("Pair")], Vec::new());
        let pair_impl_one = same_package(Vec::new(), vec![named("Pair")]);
        let pair_impl_two = same_package(Vec::new(), vec![named("Pair")]);
        let files = [
            WorldFile {
                path: "z.rs",
                facts: &zeta,
            },
            WorldFile {
                path: "a.rs",
                facts: &alpha,
            },
            WorldFile {
                path: "d1.rs",
                facts: &dup_first,
            },
            WorldFile {
                path: "d2.rs",
                facts: &dup_second,
            },
            WorldFile {
                path: "p.rs",
                facts: &pair,
            },
            WorldFile {
                path: "p1.rs",
                facts: &pair_impl_one,
            },
            WorldFile {
                path: "p2.rs",
                facts: &pair_impl_two,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        let messages: Vec<String> = out.iter().map(|finding| finding.message.clone()).collect();
        assert_eq!(messages.len(), 2);
        assert!(
            messages[0].contains("`Alpha`"),
            "the first finding must be `Alpha`: {}",
            messages[0]
        );
        assert!(
            messages[1].contains("`Zeta`"),
            "the second finding must be `Zeta`: {}",
            messages[1]
        );
    }

    #[test]
    fn same_name_in_two_packages_flags_twice() {
        let rule = SingleImplAbstraction { allow: Vec::new() };
        let left = facts("left", vec![named("Draw")], vec![named("Draw")]);
        let right = facts("right", vec![named("Draw")], vec![named("Draw")]);
        let files = [
            WorldFile {
                path: "left.rs",
                facts: &left,
            },
            WorldFile {
                path: "right.rs",
                facts: &right,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert_eq!(
            out.len(),
            2,
            "each package has its own sole implementation: {out:?}"
        );
    }

    fn in_lang(lang: Lang, abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
        let mut file = same_package(abstractions, impls);
        file.lang = lang;
        file
    }

    #[test]
    fn a_second_implementation_in_tsx_counts_for_a_typescript_interface() {
        let script = in_lang(Lang::TypeScript, vec![named("Shape")], vec![named("Shape")]);
        let view = in_lang(Lang::Tsx, Vec::new(), vec![named("Shape")]);
        let files = [
            WorldFile {
                path: "shape.ts",
                facts: &script,
            },
            WorldFile {
                path: "square.tsx",
                facts: &view,
            },
        ];
        let out = reported(&files);
        assert!(
            out.is_empty(),
            "TypeScript and TSX share one namespace: {out:?}"
        );
    }

    #[test]
    fn an_implementation_in_an_unrelated_language_is_not_a_second_one() {
        let script = in_lang(Lang::TypeScript, vec![named("Shape")], vec![named("Shape")]);
        let other = in_lang(Lang::Python, Vec::new(), vec![named("Shape")]);
        let files = [
            WorldFile {
                path: "shape.ts",
                facts: &script,
            },
            WorldFile {
                path: "shape.py",
                facts: &other,
            },
        ];
        let out = reported(&files);
        assert_eq!(
            out.len(),
            1,
            "a Python class cannot implement a TypeScript interface: {out:?}"
        );
    }

    #[test]
    fn impl_in_another_package_stays_silent() {
        let rule = SingleImplAbstraction { allow: Vec::new() };
        let declaration = facts("left", vec![named("Draw")], Vec::new());
        let elsewhere = facts("right", Vec::new(), vec![named("Draw")]);
        let files = [
            WorldFile {
                path: "left.rs",
                facts: &declaration,
            },
            WorldFile {
                path: "right.rs",
                facts: &elsewhere,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert!(
            out.is_empty(),
            "an impl from another package is not a local implementation: {out:?}"
        );
    }

    #[test]
    fn ext_suffixed_name_with_one_impl_flags() {
        let rule = SingleImplAbstraction { allow: Vec::new() };
        let ext = same_package(vec![named("ParseExt")], vec![named("ParseExt")]);
        let files = [WorldFile {
            path: "input.rs",
            facts: &ext,
        }];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert_eq!(
            out.len(),
            1,
            "a name suffix must not exempt a sole implementation: {out:?}"
        );
    }

    fn abstraction<'s>(name: &'s str, visibility: Visibility) -> TypeDef<'s> {
        typedef(name, visibility, true)
    }

    fn typedef<'s>(name: &'s str, visibility: Visibility, abstraction: bool) -> TypeDef<'s> {
        TypeDef {
            name,
            pos: Pos { line: 1, byte: 0 },
            visibility,
            abstraction,
        }
    }

    fn concrete_impl<'s>(name: &'s str) -> ImplRef<'s> {
        implref(name, false)
    }

    fn generic_impl<'s>(name: &'s str) -> ImplRef<'s> {
        implref(name, true)
    }

    fn implref<'s>(name: &'s str, generic: bool) -> ImplRef<'s> {
        ImplRef {
            name,
            pos: Pos { line: 1, byte: 0 },
            is_generic: generic,
            child_is_abstract: false,
        }
    }

    fn file_facts<'s>(
        source: &'s str,
        types: Vec<TypeDef<'s>>,
        impls: Vec<ImplRef<'s>>,
    ) -> FileFacts<'s> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            package: "pkg".to_string(),
            is_generated: false,
            line_count: 1,
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types,
            impls,
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

    fn projected(source: &str, types: Vec<TypeDef<'_>>, impls: Vec<ImplRef<'_>>) -> usize {
        let rule = SingleImplAbstraction { allow: Vec::new() };
        let file = file_facts(source, types, impls);
        let world = WorldFacts::from_facts(&file);
        let files = [WorldFile {
            path: "src/lib.rs",
            facts: &world,
        }];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        out.len()
    }

    #[test]
    fn projected_exported_trait_stays_silent() {
        let count = projected(
            "pub trait Draw {}\n",
            vec![abstraction("Draw", Visibility::Exported)],
            vec![concrete_impl("Draw")],
        );
        assert_eq!(
            count, 0,
            "an exported trait never reaches the rule: {count}"
        );
    }

    #[test]
    fn projected_generic_impl_stays_silent() {
        let count = projected(
            "trait Captures {}\n",
            vec![abstraction("Captures", Visibility::Restricted)],
            vec![generic_impl("Captures")],
        );
        assert_eq!(count, 0, "a generic impl never reaches the rule: {count}");
    }

    #[test]
    fn projected_private_trait_with_concrete_impl_flags() {
        let count = projected(
            "trait Draw {}\n",
            vec![abstraction("Draw", Visibility::Restricted)],
            vec![concrete_impl("Draw")],
        );
        assert_eq!(
            count, 1,
            "a crate-visible trait with one concrete impl flags: {count}"
        );
    }

    #[test]
    fn test_double_impl_silences_the_library_singleton() {
        let library = same_package(vec![named("Repo")], vec![named("Repo")]);
        let double = test_double(Vec::new(), vec![named("Repo")]);
        let files = [
            WorldFile {
                path: "a.rs",
                facts: &library,
            },
            WorldFile {
                path: "t.rs",
                facts: &double,
            },
        ];
        assert!(
            reported(&files).is_empty(),
            "a test double is a second implementation and keeps the seam: {:?}",
            reported(&files)
        );
    }

    #[test]
    fn abstraction_and_impl_only_in_a_test_file_stay_silent() {
        let double = test_double(vec![named("Repo")], vec![named("Repo")]);
        let files = [WorldFile {
            path: "t.rs",
            facts: &double,
        }];
        assert!(
            reported(&files).is_empty(),
            "a test-only impl must stay silent"
        );
    }

    #[test]
    fn test_region_abstraction_does_not_ambiguate_the_library_singleton() {
        let library = same_package(vec![named("Repo")], vec![named("Repo")]);
        let double = test_double(vec![named("Repo")], Vec::new());
        let files = [
            WorldFile {
                path: "a.rs",
                facts: &library,
            },
            WorldFile {
                path: "t.rs",
                facts: &double,
            },
        ];
        let out = reported(&files);
        assert_eq!(
            out.len(),
            1,
            "the exempt abstraction declaration must not block the library report: {out:?}"
        );
        assert_eq!(&*out[0].path, "a.rs");
    }
}
