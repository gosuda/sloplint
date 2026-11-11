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
mod tests {
    use super::*;
    use crate::role::CodeRole;
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
        facts_in("crate", defs)
    }

    fn facts_in(package: &str, defs: Vec<NamedSite>) -> WorldFacts {
        facts_with_role(CodeRole::Library, package, defs)
    }

    fn facts_with_role(role: CodeRole, package: &str, defs: Vec<NamedSite>) -> WorldFacts {
        WorldFacts {
            lang: Lang::Rust,
            role,
            regions: Vec::new(),
            package: package.to_string(),
            has_inline_tests: false,
            public_functions: 0,
            definitions: defs,
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
            imports: Vec::new(),
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
        assert_eq!(
            order,
            [("a.rs".to_string(), 3), ("a.rs".to_string(), 1)],
            "groups emit in name order"
        );
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

    #[test]
    fn same_name_across_packages_stays_silent() {
        let rule = HomonymDefinitions {
            allow: HashSet::new(),
            min_files: 5,
        };
        let left: Vec<WorldFacts> = (1u32..=4)
            .map(|line| facts_in("left", vec![site("reconcile_ledger", line)]))
            .collect();
        let right: Vec<WorldFacts> = (1u32..=4)
            .map(|line| facts_in("right", vec![site("reconcile_ledger", line)]))
            .collect();
        let owned: Vec<WorldFacts> = left.into_iter().chain(right).collect();
        let files: Vec<WorldFile> = owned
            .iter()
            .enumerate()
            .map(|(index, facts)| WorldFile {
                path: if index < 4 { "left.rs" } else { "right.rs" },
                facts,
            })
            .collect();
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert!(
            out.is_empty(),
            "four files in each of two packages must stay below five files: {out:?}"
        );
    }

    #[test]
    fn same_package_reaching_threshold_flags() {
        let rule = HomonymDefinitions {
            allow: HashSet::new(),
            min_files: 5,
        };
        let owned: Vec<WorldFacts> = (1u32..=5)
            .map(|line| facts_in("ledger", vec![site("reconcile_ledger", line)]))
            .collect();
        let files: Vec<WorldFile> = owned
            .iter()
            .enumerate()
            .map(|(index, facts)| WorldFile {
                path: ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"][index],
                facts,
            })
            .collect();
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert_eq!(
            out.len(),
            1,
            "five files in one package must report: {out:?}"
        );
        assert!(
            out[0].message.contains("`reconcile_ledger`"),
            "the finding names the repeated definition: {}",
            out[0].message
        );
    }
    #[test]
    fn bench_first_site_does_not_suppress_libraries_meeting_threshold() {
        let rule = HomonymDefinitions {
            allow: HashSet::new(),
            min_files: 5,
        };
        let bench = facts_with_role(CodeRole::Bench, "ledger", vec![site("reconcile_ledger", 1)]);
        let owned: Vec<WorldFacts> = (1u32..=5)
            .map(|line| facts_in("ledger", vec![site("reconcile_ledger", line)]))
            .collect();
        let mut owned = owned;
        owned.insert(0, bench);
        let paths = [
            "benches/a.rs",
            "lib_a.rs",
            "lib_b.rs",
            "lib_c.rs",
            "lib_d.rs",
            "lib_e.rs",
        ];
        let files: Vec<WorldFile> = owned
            .iter()
            .enumerate()
            .map(|(index, facts)| WorldFile {
                path: paths[index],
                facts,
            })
            .collect();
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert_eq!(
            out.len(),
            1,
            "five library files must still report once: {out:?}"
        );
        assert_eq!(
            &*out[0].path, "lib_a.rs",
            "the finding must sit on the first library site: {}",
            out[0].path
        );
        assert!(
            out[0].message.contains("defined in 5 files"),
            "the bench file must not inflate the count: {}",
            out[0].message
        );
    }

    #[test]
    fn exempt_files_do_not_push_libraries_over_threshold() {
        let rule = HomonymDefinitions {
            allow: HashSet::new(),
            min_files: 5,
        };
        let bench = facts_with_role(CodeRole::Bench, "ledger", vec![site("reconcile_ledger", 1)]);
        let tests = facts_with_role(CodeRole::Test, "ledger", vec![site("reconcile_ledger", 2)]);
        let owned: Vec<WorldFacts> = (1u32..=4)
            .map(|line| facts_in("ledger", vec![site("reconcile_ledger", line)]))
            .collect();
        let mut owned = owned;
        owned.push(bench);
        owned.push(tests);
        let paths = ["a.rs", "b.rs", "c.rs", "d.rs", "benches/a.rs", "tests/t.rs"];
        let files: Vec<WorldFile> = owned
            .iter()
            .enumerate()
            .map(|(index, facts)| WorldFile {
                path: paths[index],
                facts,
            })
            .collect();
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert!(
            out.is_empty(),
            "four library files stay below five files: {out:?}"
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
        assert_eq!(
            preset(Strictness::Strict).options.get("min_files"),
            Some(&toml::Value::Integer(5)),
            "strict keeps the five-file trigger"
        );
        let strict = build(preset(Strictness::Strict).options).expect("strict preset builds");
        let owned: Vec<WorldFacts> = (1u32..=5)
            .map(|line| facts_in("ledger", vec![site("reconcile_ledger", line)]))
            .collect();
        let files: Vec<WorldFile> = owned
            .iter()
            .enumerate()
            .map(|(index, facts)| WorldFile {
                path: ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"][index],
                facts,
            })
            .collect();
        let mut out = Vec::new();
        strict.check(&files, &mut out);
        assert_eq!(
            out.len(),
            1,
            "strict flags five files sharing a name: {out:?}"
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
            let reported = rule.reported_names(&world);
            let mut reference: BTreeMap<(&str, &str), BTreeSet<usize>> = BTreeMap::new();
            for (index, defs) in files.iter().enumerate() {
                for &name in defs {
                    if name != "h" {
                        reference.entry(("crate", name)).or_default().insert(index);
                    }
                }
            }
            let expected: HashSet<(Lang, &str, &str)> = reference
                .into_iter()
                .filter(|(_, indexes)| indexes.len() >= min_files)
                .map(|((package, name), _)| (Lang::Rust, package, name))
                .collect();
            assert_eq!(
                reported,
                expected,
                "sharded counts must match the distinct-file reference"
            );
        }
    }
}
