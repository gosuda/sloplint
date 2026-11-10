//! Ladder invariants through the public rule API: levels pin single-severity
//! findings, ceilings never raise graded findings, and grade() names the
//! highest rung at or below the value.

use proptest::prelude::*;
use sloplint_core::facts::{DynamicSymbol, FileFacts, Function, Visibility, WorldFacts};
use sloplint_core::finding::{Confidence, Severity, SourceText};
use sloplint_core::lang::Lang;
use sloplint_core::role::{CodeRole, RoleRegion};
use sloplint_core::rules::Thresholds;
use sloplint_core::{CATALOG, Config, Pos, RuleFilter, RuleSet, WorldFile};

fn config_with_level(rule: &str, level: &str) -> Config {
    let mut table = toml::Table::new();
    table.insert("level".to_string(), toml::Value::String(level.to_string()));
    let mut config = Config::default();
    config.rules.insert(rule.to_string(), table);
    config
}

fn world_facts(barrel: Option<Vec<Vec<String>>>) -> WorldFacts {
    WorldFacts {
        lang: Lang::TypeScript,
        role: CodeRole::Library,
        regions: Vec::new(),
        package: "app".to_string(),
        has_inline_tests: false,
        public_functions: 0,
        definitions: Vec::new(),
        abstractions: Vec::new(),
        impls: Vec::new(),
        barrel,
    }
}

fn file_facts(line_count: u32) -> FileFacts<'static> {
    FileFacts {
        lang: Lang::Rust,
        path: "src/big.rs",
        source: "",
        line_starts: vec![0],
        role: CodeRole::Library,
        regions: Vec::new(),
        line_count,
        package: "app".to_string(),
        is_generated: false,
        functions: Vec::new(),
        calls: Vec::new(),
        bindings: Vec::new(),
        imports: Vec::new(),
        types: Vec::new(),
        impls: Vec::new(),
        wildcard_arms: Vec::new(),
        handlers: Vec::new(),
        assertions: Vec::new(),
        field_copies: Vec::new(),
        dynamic_symbols: Vec::new(),
        ambient: Vec::new(),
        barrel: None,
        has_inline_tests: false,
    }
}

fn regioned_facts(role: CodeRole, regions: Vec<RoleRegion>) -> FileFacts<'static> {
    let mut facts = file_facts(10);
    facts.role = role;
    facts.regions = regions;
    facts
}

fn dynamic_facts() -> FileFacts<'static> {
    let mut facts = file_facts(10);
    facts.dynamic_symbols = vec![DynamicSymbol {
        pos: Pos { line: 1, byte: 0 },
        what: "runtime name",
    }];
    facts
}

fn barrel_chains(target: &str) -> Option<Vec<Vec<String>>> {
    Some(vec![vec![target.to_string()]])
}

#[test]
fn world_rule_with_level_major_reports_its_major_findings() {
    let config = config_with_level("slop/barrel-file", "major");
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
    let index_facts = world_facts(barrel_chains("src/other.ts"));
    let other_facts = world_facts(barrel_chains("src/index.ts"));
    let files = [
        WorldFile {
            path: "src/index.ts",
            facts: &index_facts,
        },
        WorldFile {
            path: "src/other.ts",
            facts: &other_facts,
        },
    ];
    let found = rules.check_world(&files);
    assert!(!found.is_empty(), "the barrel chain must be reported");
    assert!(
        found
            .iter()
            .all(|finding| finding.severity == Severity::Major),
        "level major pins every barrel-file finding: {:?}",
        found
            .iter()
            .map(|finding| finding.severity)
            .collect::<Vec<_>>()
    );
}

#[test]
fn meta_parse_error_with_level_major_reports_major() {
    let config = config_with_level("meta/parse-error", "major");
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
    let text = SourceText {
        path: "src/broken.rs",
        source: "fn",
        line_starts: &[0],
    };
    let finding = rules
        .parse_error(&text, Pos { line: 1, byte: 0 })
        .expect("the parse error is reported");
    assert_eq!(finding.severity, Severity::Major);
    assert_eq!(finding.confidence, Confidence::High);
}

#[test]
fn threshold_rule_with_ceiling_major_keeps_grade_below_ceiling() {
    let config = config_with_level("agent/file-token-budget", "major");
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
    let minor_tier = rules.check_file(&file_facts(1600));
    assert_eq!(minor_tier.len(), 1);
    assert_eq!(
        minor_tier[0].severity,
        Severity::Minor,
        "the minor tier keeps its own grade"
    );
    let major_tier = rules.check_file(&file_facts(3200));
    assert_eq!(major_tier.len(), 1);
    assert_eq!(major_tier[0].severity, Severity::Major);
}

#[test]
fn bench_role_finding_drops_when_policy_is_off() {
    let config = config_with_level("slop/barrel-file", "major");
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
    let mut bench_facts = world_facts(barrel_chains("src/lib.ts"));
    let lib_facts = world_facts(barrel_chains("src/deep.ts"));
    bench_facts.role = CodeRole::Bench;
    let files = [
        WorldFile {
            path: "src/bench.ts",
            facts: &bench_facts,
        },
        WorldFile {
            path: "src/lib.ts",
            facts: &lib_facts,
        },
    ];
    assert!(
        rules.check_world(&files).is_empty(),
        "the bench barrel chain drops when the role policy is off"
    );
    let library_facts = world_facts(barrel_chains("src/lib.ts"));
    let control_lib = world_facts(barrel_chains("src/deep.ts"));
    let control = [
        WorldFile {
            path: "src/bench.ts",
            facts: &library_facts,
        },
        WorldFile {
            path: "src/lib.ts",
            facts: &control_lib,
        },
    ];
    let found = rules.check_world(&control);
    assert_eq!(found.len(), 1, "the same chain reports as library");
    assert_eq!(found[0].path.as_ref(), "src/bench.ts");
}

#[test]
fn test_role_finding_lowers_one_rung() {
    let config = config_with_level("slop/echo-comment", "critical");
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
    let mut facts = regioned_facts(
        CodeRole::Library,
        vec![RoleRegion {
            start_line: 1,
            end_line: 10,
            role: CodeRole::Test,
        }],
    );
    facts.functions = vec![Function {
        name: "process_order",
        pos: Pos { line: 1, byte: 0 },
        doc: vec!["process order"],
        doc_pos: None,
        visibility: Visibility::Exported,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        is_method: false,
        end_line: 1,
        has_value_return: false,
        params: Vec::new(),
        return_type: None,
        untyped_throws: false,
        missing_annotations: Vec::new(),
        cognitive: 0,
        max_depth: 0,
        deepest_line: 0,
        sole_call: None,
    }];
    let found = rules.check_file(&facts);
    assert_eq!(found.len(), 1, "the restated doc comment must be reported");
    assert_eq!(found[0].severity, Severity::Major);
    assert_eq!(found[0].confidence, Confidence::Medium);
}

#[test]
fn ceiling_never_raises_a_graded_finding() {
    for ceiling in Severity::LADDER {
        let config = config_with_level("agent/file-token-budget", ceiling.as_str());
        let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
        let found = rules.check_file(&file_facts(10_000));
        let stamped = CATALOG
            .iter()
            .find(|info| info.id == "agent/file-token-budget")
            .unwrap_or_else(|| panic!("file-token-budget stays in the catalog"))
            .confidence;
        assert_eq!(found.len(), 1, "ten thousand lines always grades blocker");
        assert_eq!(
            found[0].severity, ceiling,
            "a blocker grade under ceiling {ceiling:?} settles at the ceiling"
        );
        assert_eq!(found[0].confidence, stamped);
    }
}

#[test]
fn single_severity_rule_takes_the_level_rung() {
    for rung in Severity::LADDER {
        let config = config_with_level("grep/no-dynamic-symbol", rung.as_str());
        let rules = RuleSet::new(&config, &RuleFilter::default()).expect("rule resolves");
        let found = rules.check_file(&dynamic_facts());
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].severity, rung,
            "a single-severity finding under level {rung:?} takes the rung"
        );
        assert_eq!(
            found[0].confidence,
            Confidence::Low,
            "a pinned single-severity finding keeps the rule low confidence"
        );
    }
}

proptest! {
    #[test]
    fn grade_names_the_highest_rung_at_or_below_the_value(
        cuts in prop::collection::vec(1u32..1000, 4),
        value in any::<u32>(),
    ) {
        let mut sorted = cuts.clone();
        sorted.sort_unstable();
        let thresholds =
            Thresholds::new(sorted[0], sorted[1], sorted[2], sorted[3]).expect("sorted cuts validate");
        let expected = if value >= sorted[3] {
            Some(Severity::Blocker)
        } else if value >= sorted[2] {
            Some(Severity::Critical)
        } else if value >= sorted[1] {
            Some(Severity::Major)
        } else if value >= sorted[0] {
            Some(Severity::Minor)
        } else {
            None
        };
        prop_assert_eq!(thresholds.grade(value), expected);
        if value > 0 && value < sorted[0] {
            prop_assert_eq!(thresholds.grade(value), None);
        }
        prop_assert_eq!(thresholds.grade(u32::MAX), Some(Severity::Blocker));
        if value < u32::MAX {
            let next = thresholds.grade(value);
            let after = thresholds.grade(value + 1);
            prop_assert!(next <= after, "grade never falls as the value grows");
        }
    }
}
