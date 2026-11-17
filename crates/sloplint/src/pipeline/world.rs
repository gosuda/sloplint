use sloplint_core::Finding;
use sloplint_core::facts::WorldFacts;
use sloplint_core::rules::WorldFile;
use sloplint_syntax::Analysis;

use super::{Analyzed, Pipeline};

pub(super) fn facts(analysis: &Analysis<'_>) -> WorldFacts {
    let mut world = WorldFacts::from_facts(&analysis.facts);
    if analysis.partial {
        world.barrel = None;
        for sites in [
            &mut world.definitions,
            &mut world.abstractions,
            &mut world.impls,
        ] {
            sites.retain(|site| !analysis.tainted.contains(site.line));
        }
    }
    world
}

pub(super) fn findings(pipe: &Pipeline<'_>, analyzed: &[Analyzed]) -> Vec<Finding> {
    let files: Vec<WorldFile<'_>> = analyzed
        .iter()
        .map(|file| WorldFile {
            path: file.rel.as_str(),
            facts: &file.output.world,
            parsed: file.output.parsed,
        })
        .collect();
    pipe.rules.check_world(&files)
}
