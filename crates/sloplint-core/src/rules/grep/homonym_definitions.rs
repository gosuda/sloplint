use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{parse_options, string_values};
use crate::config::Strictness;
use crate::facts::{NamedSite, WorldFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift, role_at};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

const ALLOWED_NAMES: &[&str] = &[
    "setup", "teardown", "test", "String", "Error", "Result", "Config", "Options",
];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/homonym-definitions",
    family: Family::Grep,
    summary: "one name defined in many files",
    why: "Many definitions with one name make search results ambiguous.",
    confidence: Confidence::Low,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow: Vec<String>,
    min_files: u32,
}

struct SiteRef<'a> {
    name: &'a str,
    path: &'a str,
    site: &'a NamedSite,
}

struct HomonymDefinitions {
    allow: HashSet<String>,
    min_files: usize,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, min_files) = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => (Level::Off, 8),
        Strictness::Strict => (Level::At(Severity::Info), 5),
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
        let reported = self.reported_names(files);
        if reported.is_empty() {
            return;
        }
        let mut flat: Vec<(Lang, &str, SiteRef<'_>)> = Vec::new();
        for file in files {
            add_definitions(file, &reported, &mut flat);
        }
        flat.sort_by(|a, b| {
            (a.0, a.1, a.2.name, a.2.path, a.2.site.line).cmp(&(
                b.0,
                b.1,
                b.2.name,
                b.2.path,
                b.2.site.line,
            ))
        });
        for group in flat.chunk_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2.name == b.2.name) {
            Self::report(group, out);
        }
    }
}

type Bucket<'a> = Vec<(Lang, &'a str, &'a str, usize)>;

fn eligible(facts: &WorldFacts, line: u32) -> bool {
    INFO.roles.shift(role_at(facts.role, &facts.regions, line)) != Shift::Off
}

fn add_definitions<'a>(
    file: &WorldFile<'a>,
    reported: &HashSet<(Lang, &'a str, &'a str)>,
    flat: &mut Vec<(Lang, &'a str, SiteRef<'a>)>,
) {
    let facts = file.facts;
    let package = facts.package.as_str();
    for site in &facts.definitions {
        if eligible(facts, site.line)
            && reported.contains(&(facts.lang, package, site.name.as_ref()))
        {
            flat.push((
                facts.lang,
                package,
                SiteRef {
                    name: &site.name,
                    path: file.path,
                    site,
                },
            ));
        }
    }
}

impl HomonymDefinitions {
    fn reported_names<'a>(&self, files: &[WorldFile<'a>]) -> HashSet<(Lang, &'a str, &'a str)> {
        const NAME_SHARDS: usize = 64;
        let state = std::hash::RandomState::new();
        let chunk = files
            .len()
            .div_ceil(rayon::current_num_threads() * 4)
            .max(1);
        let buckets: Vec<[Bucket<'a>; NAME_SHARDS]> = files
            .par_chunks(chunk)
            .enumerate()
            .map(|(chunk_index, chunk_files)| {
                let mut buckets: [Bucket<'a>; NAME_SHARDS] = std::array::from_fn(|_| Vec::new());
                for (offset, file) in chunk_files.iter().enumerate() {
                    let facts = file.facts;
                    let package = facts.package.as_str();
                    for site in &facts.definitions {
                        if self.allow.contains(site.name.as_ref()) || !eligible(facts, site.line) {
                            continue;
                        }
                        let shard =
                            usize::from(state.hash_one(&*site.name).to_le_bytes()[0]) % NAME_SHARDS;
                        buckets[shard].push((
                            facts.lang,
                            package,
                            &site.name,
                            chunk_index * chunk + offset,
                        ));
                    }
                }
                buckets
            })
            .collect();
        (0..NAME_SHARDS)
            .into_par_iter()
            .flat_map(|shard| {
                let mut counts: HashMap<(Lang, &'a str, &'a str), (usize, usize)> = HashMap::new();
                for bucket in &buckets {
                    for &(lang, package, name, index) in &bucket[shard] {
                        let (count, last) = counts
                            .entry((lang, package, name))
                            .or_insert((0, usize::MAX));
                        if *last != index {
                            *count += 1;
                            *last = index;
                        }
                    }
                }
                counts
                    .into_iter()
                    .filter(|(_, (count, _))| *count >= self.min_files)
                    .map(|(key, _)| key)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn report(group: &[(Lang, &str, SiteRef<'_>)], out: &mut Vec<Finding>) {
        let Some((_, _, first)) = group.first() else {
            return;
        };
        let name = first.name;
        let file_count = group.chunk_by(|a, b| a.2.path == b.2.path).count();
        let mut others = Vec::new();
        for entry in group.iter().skip(1).take(5) {
            others.push(format!("{}:{}", entry.2.path, entry.2.site.line));
        }
        let joined = others.join(", ");
        out.push(Finding::at_site(
            first.path,
            first.site,
            INFO.id,
            Severity::Major,
            format!("`{name}` is defined in {file_count} files"),
            format!("give each definition a domain-specific name; other definitions: {joined}"),
        ));
    }
}

#[cfg(test)]
#[path = "homonym_definitions_tests.rs"]
mod tests;
