use std::collections::{HashMap, HashSet};

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
        let incomplete: HashSet<_> = files
            .iter()
            .filter(|file| !file.parsed)
            .map(|file| (file.facts.lang.namespace(), file.facts.package.as_str()))
            .collect();
        let abstractions = abstraction_sites(files);
        let impls = impl_index(files);
        for group in abstractions.chunk_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2) {
            if let [(lang, package, name, path, site)] = *group
                && !incomplete.contains(&(lang, package))
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
#[path = "single_impl_abstraction_tests.rs"]
mod tests;
