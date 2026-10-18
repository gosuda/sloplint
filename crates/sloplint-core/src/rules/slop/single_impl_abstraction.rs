//! `slop/single-impl-abstraction`: abstractions with exactly one implementation.

use std::collections::BTreeMap;

use crate::config::Strictness;
use crate::facts::NamedSite;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

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
        for (name, sites) in &abstractions {
            let Some((path, site)) = sites.first() else {
                continue;
            };
            if sites.len() > 1 || self.allowed(name) {
                continue;
            }
            let Some((impl_path, impl_site)) = sole_impl(files, name) else {
                continue;
            };
            out.push(Finding::at_site(
                path,
                site,
                INFO.id,
                Severity::Warn,
                format!("`{name}` has exactly one implementation ({impl_path}:{})", impl_site.line),
                format!(
                    "use the concrete type directly and delete `{name}`; add the abstraction when a second implementation exists"
                ),
            ));
        }
    }
}

type Sites<'a> = BTreeMap<&'a str, Vec<(&'a str, &'a NamedSite)>>;

fn abstraction_sites<'a>(files: &[WorldFile<'a>]) -> Sites<'a> {
    let mut sites: Sites<'a> = BTreeMap::new();
    for file in files {
        for site in &file.facts.abstractions {
            sites
                .entry(site.name.as_ref())
                .or_default()
                .push((file.path, site));
        }
    }
    sites
}

fn sole_impl<'a>(files: &[WorldFile<'a>], name: &str) -> Option<(&'a str, &'a NamedSite)> {
    let mut found = None;
    for file in files {
        for site in &file.facts.impls {
            if site.name.as_ref() != name {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some((file.path, site));
        }
    }
    found
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
