use super::*;
use crate::facts::WorldFacts;
use crate::role::{CodeRole, RoleRegion};
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
        package_from_manifest: false,
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
fn test_region_inside_library_file_stays_below_threshold() {
    let rule = HomonymDefinitions {
        allow: HashSet::new(),
        min_files: 5,
    };
    let mut regioned = facts_in("ledger", vec![site("reconcile_ledger", 7)]);
    regioned.regions = vec![RoleRegion {
        start_line: 5,
        end_line: 9,
        role: CodeRole::Test,
    }];
    let owned: Vec<WorldFacts> = (1u32..=4)
        .map(|line| facts_in("ledger", vec![site("reconcile_ledger", line)]))
        .collect();
    let mut owned = owned;
    owned.push(regioned);
    let paths = ["a.rs", "b.rs", "c.rs", "d.rs", "src/inner.rs"];
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
        "a library-role file whose only site sits in a test region must not count: {out:?}"
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
