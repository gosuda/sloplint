//! `slop/barrel-file`: barrel files re-exporting from other barrel files.

use crate::config::Strictness;
use crate::facts::NamedSite;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

/// `slop/barrel-file`: a pass-through re-export chain between a name and its definition.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/barrel-file",
    family: Family::Slop,
    summary: "barrel files re-exporting from other barrel files",
    why: "Chained re-exports add hops between a name and its definition.",
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
        options: toml::Table::new(),
    }
}

fn build(_options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    Ok(Box::new(BarrelFile))
}

struct BarrelFile;

impl WorldRule for BarrelFile {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        if !files.iter().any(|file| file.facts.barrel.is_some()) {
            return;
        }
        let mut paths: Vec<&str> = files.iter().map(|file| file.path).collect();
        paths.sort_unstable();
        let mut barrels: Vec<&str> = files
            .iter()
            .filter(|file| file.facts.barrel.is_some())
            .map(|file| file.path)
            .collect();
        barrels.sort_unstable();
        for file in files {
            let Some(targets) = &file.facts.barrel else {
                continue;
            };
            let Some(target) = chain_target(&paths, &barrels, targets) else {
                continue;
            };
            let site = NamedSite {
                name: "".into(),
                line: 1,
                column: 1,
                context: "".into(),
            };
            out.push(Finding::at_site(
                file.path,
                &site,
                INFO.id,
                Severity::Warn,
                format!("barrel file re-exports from another barrel `{target}`"),
                "import from the defining module directly and delete the pass-through re-exports"
                    .to_string(),
            ));
        }
    }
}

fn chain_target<'a>(
    paths: &[&str],
    barrels: &[&str],
    targets: &'a [Vec<String>],
) -> Option<&'a str> {
    for candidates in targets {
        for candidate in candidates {
            let path = candidate.as_str();
            if paths.binary_search(&path).is_err() {
                continue;
            }
            if barrels.binary_search(&path).is_ok() {
                return Some(path);
            }
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::WorldFacts;
    use crate::lang::Lang;

    fn facts(barrel: Option<Vec<Vec<String>>>) -> WorldFacts {
        WorldFacts {
            lang: Lang::TypeScript,
            is_test_file: false,
            is_entry_file: false,
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel,
        }
    }

    #[test]
    fn findings_follow_file_order_and_name_the_first_barrel_target() {
        let rule = BarrelFile;
        let zed = facts(Some(vec![vec![
            "missing.ts".to_string(),
            "b.ts".to_string(),
        ]]));
        let ay = facts(Some(vec![
            vec!["leaf.ts".to_string()],
            vec!["b.ts".to_string()],
        ]));
        let bee = facts(Some(Vec::new()));
        let leaf = facts(None);
        let files = [
            WorldFile {
                path: "z.ts",
                facts: &zed,
            },
            WorldFile {
                path: "a.ts",
                facts: &ay,
            },
            WorldFile {
                path: "b.ts",
                facts: &bee,
            },
            WorldFile {
                path: "leaf.ts",
                facts: &leaf,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        let order: Vec<String> = out.iter().map(|finding| finding.path.to_string()).collect();
        assert_eq!(order, ["z.ts".to_string(), "a.ts".to_string()]);
        assert!(
            out[0].message.contains("`b.ts`"),
            "z.ts must name its barrel target: {}",
            out[0].message
        );
        assert!(
            out[1].message.contains("`b.ts`"),
            "a.ts must name its barrel target: {}",
            out[1].message
        );
    }

    #[test]
    fn no_barrel_file_reports_nothing() {
        let rule = BarrelFile;
        let plain = facts(None);
        let files = [WorldFile {
            path: "a.ts",
            facts: &plain,
        }];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        assert!(
            out.is_empty(),
            "a tree without barrels must report nothing: {out:?}"
        );
    }
}
