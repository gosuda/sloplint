use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use rayon::prelude::*;
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
        let reported = self.reported_names(files);
        if reported.is_empty() {
            return;
        }
        let mut flat: Vec<SiteRef<'_>> = Vec::new();
        for file in files {
            add_definitions(file, &reported, &mut flat);
        }
        flat.sort_by(|a, b| (a.name, a.path, a.site.line).cmp(&(b.name, b.path, b.site.line)));
        for sites in flat.chunk_by(|a, b| a.name == b.name) {
            self.report(sites, out);
        }
    }
}

fn add_definitions<'a>(
    file: &WorldFile<'a>,
    reported: &HashSet<&'a str>,
    flat: &mut Vec<SiteRef<'a>>,
) {
    for site in &file.facts.definitions {
        if reported.contains(site.name.as_ref()) {
            flat.push(SiteRef {
                name: &site.name,
                path: file.path,
                site,
            });
        }
    }
}

impl HomonymDefinitions {
    fn reported_names<'a>(&self, files: &[WorldFile<'a>]) -> HashSet<&'a str> {
        const NAME_SHARDS: usize = 64;
        let state = std::hash::RandomState::new();
        let chunk = files
            .len()
            .div_ceil(rayon::current_num_threads() * 4)
            .max(1);
        let buckets: Vec<[Vec<(&'a str, usize)>; NAME_SHARDS]> = files
            .par_chunks(chunk)
            .enumerate()
            .map(|(chunk_index, chunk_files)| {
                let mut buckets: [Vec<(&'a str, usize)>; NAME_SHARDS] =
                    std::array::from_fn(|_| Vec::new());
                for (offset, file) in chunk_files.iter().enumerate() {
                    for site in &file.facts.definitions {
                        if self.allow.contains(site.name.as_ref()) {
                            continue;
                        }
                        let shard =
                            usize::from(state.hash_one(&*site.name).to_le_bytes()[0]) % NAME_SHARDS;
                        buckets[shard].push((&site.name, chunk_index * chunk + offset));
                    }
                }
                buckets
            })
            .collect();
        (0..NAME_SHARDS)
            .into_par_iter()
            .flat_map(|shard| {
                let mut counts: HashMap<&'a str, (usize, usize)> = HashMap::new();
                for bucket in &buckets {
                    for &(name, index) in &bucket[shard] {
                        let (count, last) = counts.entry(name).or_insert((0, usize::MAX));
                        if *last != index {
                            *count += 1;
                            *last = index;
                        }
                    }
                }
                counts
                    .into_iter()
                    .filter(|(_, (count, _))| *count >= self.min_files)
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn report(&self, sites: &[SiteRef<'_>], out: &mut Vec<Finding>) {
        let Some(first) = sites.first() else {
            return;
        };
        let name = first.name;
        let file_count = sites.chunk_by(|a, b| a.path == b.path).count();
        let mut others = Vec::new();
        for entry in sites.iter().skip(1).take(5) {
            others.push(format!("{}:{}", entry.path, entry.site.line));
        }
        let joined = others.join(", ");
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::WorldFacts;
    use crate::lang::Lang;
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;

    fn site(name: &str, line: u32) -> NamedSite {
        NamedSite {
            name: name.into(),
            line,
            column: 1,
        }
    }

    fn facts(defs: Vec<NamedSite>) -> WorldFacts {
        WorldFacts {
            lang: Lang::Rust,
            is_test_file: false,
            is_entry_file: false,
            has_inline_tests: false,
            public_functions: 0,
            definitions: defs,
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
        }
    }

    #[test]
    fn groups_emit_in_name_order_with_first_site_at_smallest_path() {
        let rule = HomonymDefinitions {
            allow: HashSet::new(),
            min_files: 2,
        };
        let later = facts(vec![site("zeta", 2), site("alpha", 9)]);
        let earlier = facts(vec![site("zeta", 1), site("alpha", 3)]);
        let files = [
            WorldFile {
                path: "b.rs",
                facts: &later,
            },
            WorldFile {
                path: "a.rs",
                facts: &earlier,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        let order: Vec<(String, u32)> = out
            .iter()
            .map(|finding| (finding.path.to_string(), finding.line))
            .collect();
        assert_eq!(order, [("a.rs".to_string(), 3), ("a.rs".to_string(), 1)]);
        assert!(
            out[0].message.contains("`alpha`"),
            "the first group must be `alpha`: {}",
            out[0].message
        );
        assert!(
            out[1].message.contains("`zeta`"),
            "the second group must be `zeta`: {}",
            out[1].message
        );
    }

    #[test]
    fn repeats_inside_one_file_count_as_one_file() {
        let rule = HomonymDefinitions {
            allow: HashSet::new(),
            min_files: 3,
        };
        let crowded = facts(vec![site("run", 1), site("run", 5), site("run", 9)]);
        let other = facts(vec![site("run", 2)]);
        let third = facts(vec![site("run", 4)]);
        let files = |extra: bool| {
            let mut files = vec![
                WorldFile {
                    path: "a.rs",
                    facts: &crowded,
                },
                WorldFile {
                    path: "b.rs",
                    facts: &other,
                },
            ];
            if extra {
                files.push(WorldFile {
                    path: "c.rs",
                    facts: &third,
                });
            }
            files
        };
        let mut out = Vec::new();
        rule.check(&files(false), &mut out);
        assert!(
            out.is_empty(),
            "four definitions in two files must stay below three files: {out:?}"
        );
        rule.check(&files(true), &mut out);
        assert_eq!(out.len(), 1, "three files must report once: {out:?}");
        assert!(
            out[0].message.contains("defined in 3 files"),
            "the message must count files, not definitions: {}",
            out[0].message
        );
    }

    proptest! {
        #[test]
        fn sharded_counts_match_a_sequential_reference(
            files in proptest::collection::vec(
                proptest::collection::vec(
                    prop_oneof![
                        Just("a"),
                        Just("b"),
                        Just("c"),
                        Just("d"),
                        Just("e"),
                        Just("f"),
                        Just("g"),
                        Just("h"),
                    ],
                    0..=6
                ),
                0..=60
            ),
            min_files in 1usize..=5,
        ) {
            let owned: Vec<WorldFacts> = files
                .iter()
                .map(|defs| facts(defs.iter().map(|&name| site(name, 1)).collect()))
                .collect();
            let world: Vec<WorldFile> = owned
                .iter()
                .map(|facts| WorldFile { path: "", facts })
                .collect();
            let rule = HomonymDefinitions {
                allow: HashSet::from(["h".to_string()]),
                min_files,
            };
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .build()
                .expect("the proptest pool must build");
            let reported = pool.install(|| rule.reported_names(&world));
            let mut reference: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
            for (index, defs) in files.iter().enumerate() {
                for &name in defs {
                    if name != "h" {
                        reference.entry(name).or_default().insert(index);
                    }
                }
            }
            let expected: HashSet<&str> = reference
                .into_iter()
                .filter(|(_, indexes)| indexes.len() >= min_files)
                .map(|(name, _)| name)
                .collect();
            assert_eq!(
                reported,
                expected,
                "sharded counts must match the distinct-file reference"
            );
        }
    }
}
