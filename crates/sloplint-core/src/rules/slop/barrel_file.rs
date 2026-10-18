//! `slop/barrel-file`: barrel files re-exporting from other barrel files.

use std::collections::BTreeSet;

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
        let paths: BTreeSet<&str> = files.iter().map(|file| file.path).collect();
        let barrels: BTreeSet<&str> = files
            .iter()
            .filter(|file| file.facts.barrel.is_some())
            .map(|file| file.path)
            .collect();
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
    paths: &BTreeSet<&str>,
    barrels: &BTreeSet<&str>,
    targets: &'a [Vec<String>],
) -> Option<&'a str> {
    for candidates in targets {
        for candidate in candidates {
            let path = candidate.as_str();
            if !paths.contains(path) {
                continue;
            }
            if barrels.contains(path) {
                return Some(path);
            }
            break;
        }
    }
    None
}
