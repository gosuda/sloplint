//! `slop/single-impl-abstraction`: abstractions with exactly one implementation.

use std::collections::HashMap;

use crate::config::Strictness;
use crate::facts::NamedSite;
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

/// Languages whose abstractions this rule judges. Adapters for other languages (Ruby mixins,
/// PHP interfaces, Go interfaces) also emit abstraction or implementation facts, which this
/// rule must not read.
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

/// `slop/single-impl-abstraction`: an interface whose only implementation is itself.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/single-impl-abstraction",
    family: Family::Slop,
    summary: "abstractions with exactly one implementation",
    why: "An interface with one implementation is indirection without variation.",
    exempt_tests: false,
    preset,
    build: Build::World(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic => Level::Off,
        Strictness::Standard | Strictness::Strict => Level::Warn,
    };
    Preset {
        level,
        options: fixed_options(),
    }
}

fn fixed_options() -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("allow".to_string(), toml::Value::Array(Vec::new()));
    table.insert(
        "allow_suffixes".to_string(),
        toml::Value::Array(vec![toml::Value::String("Ext".into())]),
    );
    table
}

fn build(options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    let options: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    Ok(Box::new(SingleImplAbstraction {
        allow: options.allow,
        allow_suffixes: options.allow_suffixes,
    }))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow: Vec<String>,
    allow_suffixes: Vec<String>,
}

struct SingleImplAbstraction {
    allow: Vec<String>,
    allow_suffixes: Vec<String>,
}

impl WorldRule for SingleImplAbstraction {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        let abstractions = abstraction_sites(files);
        let impls = impl_index(files);
        for group in abstractions.chunk_by(|a, b| a.0 == b.0) {
            if let [(name, path, site)] = *group
                && !self.allowed(name)
                && let Some((impl_path, impl_site)) = sole_impl(&impls, name)
            {
                out.push(Finding::at_site(
                        path,
                        site,
                        INFO.id,
                        Severity::Warn,
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

/// Abstraction sites in name order; runs of equal names keep file order.
type Sites<'a> = Vec<(&'a str, &'a str, &'a NamedSite)>;

fn applicable_files<'f, 'a>(files: &'f [WorldFile<'a>]) -> impl Iterator<Item = &'f WorldFile<'a>> {
    files
        .iter()
        .filter(|file| APPLICABLE.contains(&file.facts.lang))
}

fn abstraction_sites<'a>(files: &[WorldFile<'a>]) -> Sites<'a> {
    let mut sites: Sites<'a> = Vec::new();
    for file in applicable_files(files) {
        for site in &file.facts.abstractions {
            sites.push((site.name.as_ref(), file.path, site));
        }
    }
    sites.sort_by(|a, b| a.0.cmp(b.0));
    sites
}

/// First implementation site of a name, and whether any later site repeats it.
struct ImplSites<'a> {
    first: (&'a str, &'a NamedSite),
    repeated: bool,
}

type Impls<'a> = HashMap<&'a str, ImplSites<'a>>;

fn impl_index<'a>(files: &[WorldFile<'a>]) -> Impls<'a> {
    let mut index: Impls<'a> = HashMap::new();
    for file in applicable_files(files) {
        for site in &file.facts.impls {
            index
                .entry(site.name.as_ref())
                .and_modify(|sites| sites.repeated = true)
                .or_insert(ImplSites {
                    first: (file.path, site),
                    repeated: false,
                });
        }
    }
    index
}

fn sole_impl<'a>(impls: &Impls<'a>, name: &str) -> Option<(&'a str, &'a NamedSite)> {
    impls
        .get(name)
        .filter(|sites| !sites.repeated)
        .map(|sites| sites.first)
}

impl SingleImplAbstraction {
    fn allowed(&self, name: &str) -> bool {
        self.allow.iter().any(|entry| entry == name)
            || self
                .allow_suffixes
                .iter()
                .any(|suffix| name.ends_with(suffix.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::WorldFacts;

    fn named(name: &str) -> NamedSite {
        NamedSite {
            name: name.into(),
            line: 1,
            column: 1,
            context: "".into(),
        }
    }

    fn facts(abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
        WorldFacts {
            lang: Lang::Rust,
            is_test_file: false,
            is_entry_file: false,
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions,
            impls,
            barrel: None,
        }
    }

    #[test]
    fn singletons_emit_in_name_order_and_duplicates_stay_silent() {
        let rule = SingleImplAbstraction {
            allow: Vec::new(),
            allow_suffixes: Vec::new(),
        };
        let zeta = facts(vec![named("Zeta")], vec![named("Zeta")]);
        let alpha = facts(vec![named("Alpha")], vec![named("Alpha")]);
        let dup_first = facts(vec![named("Dup")], vec![named("Dup")]);
        let dup_second = facts(vec![named("Dup")], Vec::new());
        let pair = facts(vec![named("Pair")], Vec::new());
        let pair_impl_one = facts(Vec::new(), vec![named("Pair")]);
        let pair_impl_two = facts(Vec::new(), vec![named("Pair")]);
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
}
