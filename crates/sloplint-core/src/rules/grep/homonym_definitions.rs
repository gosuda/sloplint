//! `grep/homonym-definitions`: one name defined in many files.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{parse_options, string_values};
use crate::config::Strictness;
use crate::facts::NamedSite;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

const ALLOWED_NAMES: &[&str] = &[
    "new", "default", "from", "into", "fmt", "drop", "clone", "eq", "hash", "main", "init",
    "__init__", "setup", "teardown", "test", "String", "Error", "Result", "Config", "Options",
];

/// `grep/homonym-definitions`: flags names defined in at least `min_files` distinct files.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/homonym-definitions",
    family: Family::Grep,
    summary: "one name defined in many files",
    why: "Many definitions with one name make search results ambiguous.",
    exempt_tests: false,
    preset,
    build: Build::World(build),
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow: Vec<String>,
    min_files: u32,
}

struct SiteRef<'a> {
    path: &'a str,
    site: &'a NamedSite,
}

struct HomonymDefinitions {
    allow: HashSet<String>,
    min_files: usize,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, min_files) = match strictness {
        Strictness::Minimal | Strictness::Basic => (Level::Off, 8),
        Strictness::Standard => (Level::Warn, 8),
        Strictness::Strict => (Level::Warn, 5),
    };
    Preset {
        level,
        options: fixed_options(min_files),
    }
}

fn fixed_options(min_files: u32) -> toml::Table {
    let mut options = toml::Table::new();
    options.insert("allow".to_string(), string_values(ALLOWED_NAMES));
    options.insert(
        "min_files".to_string(),
        toml::Value::Integer(i64::from(min_files)),
    );
    options
}

fn build(options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    let options: Options = parse_options(options)?;
    if options.min_files < 2 {
        return Err(format!(
            "min_files {} must be at least 2",
            options.min_files
        ));
    }
    Ok(Box::new(HomonymDefinitions {
        allow: options.allow.into_iter().collect(),
        min_files: usize::try_from(options.min_files).unwrap_or(usize::MAX),
    }))
}

impl WorldRule for HomonymDefinitions {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        let mut groups: BTreeMap<&str, Vec<SiteRef<'_>>> = BTreeMap::new();
        for file in files {
            self.add_definitions(file, &mut groups);
        }
        for (name, mut sites) in groups {
            sites.sort_by_key(|entry| (entry.path, entry.site.line));
            self.report(name, &sites, out);
        }
    }
}

impl HomonymDefinitions {
    fn add_definitions<'a>(
        &self,
        file: &WorldFile<'a>,
        groups: &mut BTreeMap<&'a str, Vec<SiteRef<'a>>>,
    ) {
        for site in &file.facts.definitions {
            if self.allow.contains(site.name.as_ref()) {
                continue;
            }
            let path = file.path;
            let sites = groups.entry(&site.name).or_default();
            sites.push(SiteRef { path, site });
        }
    }

    fn report(&self, name: &str, sites: &[SiteRef<'_>], out: &mut Vec<Finding>) {
        let mut files = HashSet::new();
        for entry in sites {
            files.insert(entry.path);
        }
        if files.len() < self.min_files {
            return;
        }
        let first = &sites[0];
        let mut others = Vec::new();
        for entry in sites.iter().skip(1).take(5) {
            others.push(format!("{}:{}", entry.path, entry.site.line));
        }
        let joined = others.join(", ");
        let file_count = files.len();
        out.push(Finding::at_site(
            first.path,
            first.site,
            INFO.id,
            Severity::Deny,
            format!("`{name}` is defined in {file_count} files"),
            format!("give each definition a domain-specific name; other definitions: {joined}"),
        ));
    }
}
