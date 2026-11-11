use super::*;
use crate::facts::{Function, Visibility};
use crate::finding::line_starts;
use crate::lang::Lang;
use crate::role::{RoleRegion, Shift};

fn world_facts(role: CodeRole) -> WorldFacts {
    WorldFacts {
        lang: Lang::Rust,
        role,
        regions: Vec::new(),
        package: "ledger".to_string(),
        package_from_manifest: false,
        has_inline_tests: false,
        public_functions: 0,
        definitions: Vec::new(),
        abstractions: Vec::new(),
        impls: Vec::new(),
        barrel: None,
        imports: Vec::new(),
    }
}

fn test_preset(_: Strictness) -> Preset {
    Preset {
        level: Level::At(Severity::Major),
        options: toml::Table::new(),
    }
}

fn test_build(_: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    Err("unused".to_string())
}

#[test]
fn world_findings_shift_by_file_role() {
    let info = RuleInfo {
        id: "test/world-shift",
        family: Family::Agent,
        summary: "test",
        why: "test",
        confidence: Confidence::High,
        roles: RolePolicy {
            entry: Shift::Keep,
            test: Shift::Off,
            support: Shift::Keep,
            bench: Shift::Lower1,
            example: Shift::Keep,
        },
        preset: test_preset,
        build: Build::World(test_build),
    };
    let set = RuleSet {
        file_rules: Vec::new(),
        world_rules: Vec::new(),
        resolved: Vec::new(),
        levels: BTreeMap::from([("test/world-shift".to_string(), Level::At(Severity::Major))]),
        graded: BTreeSet::new(),
        active: BTreeSet::new(),
        filter: RuleFilter::default(),
        custom: Vec::new(),
    };
    let library = world_facts(CodeRole::Library);
    let tested = world_facts(CodeRole::Test);
    let bench = world_facts(CodeRole::Bench);
    let files = [
        WorldFile {
            path: "a.rs",
            facts: &library,
        },
        WorldFile {
            path: "m.rs",
            facts: &tested,
        },
        WorldFile {
            path: "z.rs",
            facts: &bench,
        },
    ];
    let mut found = vec![
        Finding::at_file(
            "z.rs",
            info.id,
            Severity::Major,
            "z".to_string(),
            "f".to_string(),
        ),
        Finding::at_file(
            "a.rs",
            info.id,
            Severity::Major,
            "a".to_string(),
            "f".to_string(),
        ),
        Finding::at_file(
            "m.rs",
            info.id,
            Severity::Major,
            "m".to_string(),
            "f".to_string(),
        ),
    ];
    set.finish_world_findings(&info, &files, &mut found);
    let mut got: Vec<(&str, Severity)> = found
        .iter()
        .map(|finding| (&*finding.path, finding.severity))
        .collect();
    got.sort_unstable();
    assert_eq!(got, [("a.rs", Severity::Major), ("z.rs", Severity::Minor)]);
    assert!(
        found
            .iter()
            .all(|finding| finding.confidence == Confidence::High),
        "world findings carry the rule's confidence"
    );
}

#[test]
fn nesting_drops_only_for_the_same_function_after_suppression() {
    let source = "fn reconcile_ledger() {}\n";
    let facts = FileFacts {
        lang: Lang::Rust,
        path: "src/ledger.rs",
        source,
        line_starts: line_starts(source),
        role: CodeRole::Library,
        regions: Vec::new(),
        line_count: 1,
        package: "ledger".to_string(),
        is_generated: false,
        functions: vec![Function {
            name: "reconcile_ledger",
            pos: Pos { line: 1, byte: 3 },
            doc: vec![],
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
            receiver: None,
            end_line: 1,
            has_value_return: false,
            params: vec![],
            return_type: None,
            untyped_throws: false,
            missing_annotations: vec![],
            cognitive: 100,
            max_depth: 20,
            deepest_line: 1,
            sole_call: None,
        }],
        calls: vec![],
        bindings: vec![],
        imports: vec![],
        types: vec![],
        impls: vec![],
        wildcard_arms: vec![],
        handlers: vec![],
        assertions: vec![],
        field_copies: vec![],
        dynamic_symbols: vec![],
        ambient: vec![],
        barrel: None,
        has_inline_tests: false,
    };
    let mut config = Config::default();
    config.project.strictness = Strictness::Strict;
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("the catalog rules resolve");
    let mut found = rules.check_file(&facts);
    assert_eq!(
        found
            .iter()
            .filter(|f| f.rule.as_ref() == "complexity/cognitive-depth")
            .count(),
        1,
        "the cached output keeps the cognitive finding"
    );
    assert_eq!(
        found
            .iter()
            .filter(|f| f.rule.as_ref() == "complexity/nesting-depth")
            .count(),
        1,
        "the cached output keeps the nesting finding beside it"
    );
    rules.drop_shadowed_nesting(&mut found);
    assert!(
        found
            .iter()
            .any(|f| f.rule.as_ref() == "complexity/cognitive-depth"),
        "the deep function still reports cognitive depth"
    );
    assert!(
        found
            .iter()
            .all(|f| f.rule.as_ref() != "complexity/nesting-depth"),
        "nesting stays silent on the function cognitive flags"
    );
    let mut cousins = nesting_pair(1, 3, 20);
    rules.drop_shadowed_nesting(&mut cousins);
    assert_eq!(
        cousins.len(),
        2,
        "a nesting finding on the same line but another column is another function"
    );
    let filtered = RuleSet::new(
        &config,
        &RuleFilter(vec![crate::suppress::RulePattern::Exact(
            "complexity/nesting-depth".into(),
        )]),
    )
    .expect("the catalog rules resolve");
    let mut kept = rules.check_file(&facts);
    filtered.drop_shadowed_nesting(&mut kept);
    assert_eq!(
        kept.iter()
            .filter(|f| f.rule.as_ref() == "complexity/nesting-depth")
            .count(),
        1,
        "a run that does not select cognitive keeps nesting"
    );
}

fn nesting_pair(line: u32, cog_col: u32, nest_col: u32) -> Vec<Finding> {
    let mut cognitive = Finding::at_file(
        "src/ledger.rs",
        "complexity/cognitive-depth",
        Severity::Minor,
        "c".to_string(),
        "f".to_string(),
    );
    cognitive.line = line;
    cognitive.column = cog_col;
    let mut nesting = Finding::at_file(
        "src/ledger.rs",
        "complexity/nesting-depth",
        Severity::Minor,
        "n".to_string(),
        "f".to_string(),
    );
    nesting.line = line;
    nesting.column = nest_col;
    vec![cognitive, nesting]
}

fn oversized_facts(source: &str, role: CodeRole, regions: Vec<RoleRegion>) -> FileFacts<'_> {
    FileFacts {
        lang: Lang::Rust,
        path: "src/ledger.rs",
        source,
        line_starts: line_starts(source),
        role,
        regions,
        line_count: u32::try_from(source.lines().count()).expect("lines fit u32"),
        package: "ledger".to_string(),
        is_generated: false,
        functions: vec![],
        calls: vec![],
        bindings: vec![],
        imports: vec![],
        types: vec![],
        impls: vec![],
        wildcard_arms: vec![],
        handlers: vec![],
        assertions: vec![],
        field_copies: vec![],
        dynamic_symbols: vec![],
        ambient: vec![],
        barrel: None,
        has_inline_tests: false,
    }
}

fn strict_rules() -> RuleSet {
    let mut config = Config::default();
    config.project.strictness = Strictness::Strict;
    RuleSet::new(&config, &RuleFilter::default()).expect("the catalog rules resolve")
}

fn budget_findings(rules: &RuleSet, facts: &FileFacts<'_>) -> Vec<Finding> {
    rules
        .check_file(facts)
        .into_iter()
        .filter(|finding| finding.rule.as_ref() == "agent/file-token-budget")
        .collect()
}

#[test]
fn file_budget_survives_a_leading_test_region() {
    let mut source = String::from("#[test]\nfn smoke() {}\n}\n");
    source.push_str(&"fn filler_line() {}\n".repeat(608));
    let leading_test = oversized_facts(
        &source,
        CodeRole::Library,
        vec![RoleRegion {
            start_line: 1,
            end_line: 3,
            role: CodeRole::Test,
        }],
    );
    let plain = oversized_facts(&source, CodeRole::Library, Vec::new());
    let rules = strict_rules();
    let reported = budget_findings(&rules, &leading_test);
    let normal = budget_findings(&rules, &plain);
    assert_eq!(
        reported.len(),
        1,
        "a test region at line 1 cannot hide the library budget"
    );
    assert_eq!(
        normal.len(),
        1,
        "the same source without the region keeps reporting"
    );
    assert_eq!(
        reported[0].severity, normal[0].severity,
        "the file-wide budget keeps the severity a library file earns"
    );
}

#[test]
fn file_budget_still_drops_for_whole_test_files() {
    let mut source = String::from("#[test]\nfn smoke() {}\n}\n");
    source.push_str(&"fn filler_line() {}\n".repeat(608));
    let facts = oversized_facts(&source, CodeRole::Test, Vec::new());
    let rules = strict_rules();
    assert!(
        budget_findings(&rules, &facts).is_empty(),
        "a file whose role is test keeps its budget silence"
    );
}
