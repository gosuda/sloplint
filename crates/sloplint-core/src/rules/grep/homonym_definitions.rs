//! `grep/homonym-definitions`: one name defined in many files.

use std::collections::{HashMap, HashSet};

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

/// Collects the sites of every name in `reported`, in file then definition order.
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
    /// Names outside `allow` that are defined in at least `min_files` distinct files.
    ///
    /// Counting comes first so that only reported names are sorted: on a large tree almost
    /// every name falls below the threshold.
    fn reported_names<'a>(&self, files: &[WorldFile<'a>]) -> HashSet<&'a str> {
        let mut counts: HashMap<&'a str, (usize, usize)> = HashMap::new();
        for (index, file) in files.iter().enumerate() {
            for site in &file.facts.definitions {
                if self.allow.contains(site.name.as_ref()) {
                    continue;
                }
                let (count, last) = counts.entry(&site.name).or_insert((0, usize::MAX));
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

    fn site(name: &str, line: u32) -> NamedSite {
        NamedSite {
            name: name.into(),
            line,
            column: 1,
            context: "".into(),
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
}
