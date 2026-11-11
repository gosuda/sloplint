use super::*;
use crate::facts::Visibility;
use crate::finding::line_starts;
use crate::lang::Lang;

const BLOCK: [&str; 8] = [
    "let alpha_value = compute_first(input);",
    "let beta_value = compute_second(alpha_value);",
    "let gamma_value = compute_third(beta_value);",
    "let delta_value = compute_fourth(gamma_value);",
    "let epsilon_value = compute_fifth(delta_value);",
    "let zeta_value = compute_sixth(epsilon_value);",
    "let eta_value = compute_seventh(zeta_value);",
    "let theta_value = compute_eighth(eta_value);",
];

const TAIL: [&str; 8] = [
    "name: alpha,",
    "value: compute_alpha(input),",
    "mode: Mode::Fast,",
    "limit: 4096,",
    "retry: policy.retry,",
    "label: make_label(name),",
    "flags: flags | Flags::EXTRA,",
    "owner: current_owner(),",
];

fn function(name: &str, first: u32, last: u32) -> Function<'_> {
    Function {
        name,
        pos: Pos {
            line: first,
            byte: 0,
        },
        doc: Vec::new(),
        doc_pos: None,
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        is_method: false,
        receiver: None,
        params: Vec::new(),
        return_type: None,
        untyped_throws: false,
        missing_annotations: Vec::new(),
        has_value_return: true,
        cognitive: 0,
        max_depth: 0,
        deepest_line: last,
        end_line: last,
        sole_call: None,
    }
}

fn facts<'s>(source: &'s str, functions: Vec<Function<'s>>) -> FileFacts<'s> {
    FileFacts {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        line_starts: line_starts(source),
        role: crate::role::CodeRole::Library,
        regions: Vec::new(),
        line_count: 0,
        package: String::new(),
        is_generated: false,
        functions,
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

fn check(source: &str, functions: Vec<Function<'_>>) -> Vec<String> {
    let rule = UniquePatchAnchors {
        window: 8,
        min_repeats: 3,
    };
    let mut out = Vec::new();
    rule.check(&facts(source, functions), &mut out);
    out.into_iter().map(|finding| finding.message).collect()
}

fn bannered_block_source() -> (String, Vec<Function<'static>>) {
    let mut source = String::new();
    for name in ["doc_a", "doc_b", "doc_c"] {
        for index in 0..8 {
            source.push_str(&format!("// banner {name} line {index} filler text here\n"));
        }
        source.push('\n');
    }
    let mut functions = Vec::new();
    for name in ["alpha", "beta", "gamma"] {
        let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
        source.push_str(&format!("fn {name}() {{\n"));
        for line in BLOCK {
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("}\n\n");
        let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
        functions.push(function(name, first, last));
    }
    (source, functions)
}

#[test]
fn rustdoc_banners_stay_silent_while_shared_block_flags() {
    let (source, functions) = bannered_block_source();
    let found = check(&source, functions);
    assert_eq!(found.len(), 1);
    assert!(found[0].starts_with("3 identical 9-line blocks"));
}

#[test]
fn blank_split_copies_stay_silent() {
    let mut source = String::new();
    let mut functions = Vec::new();
    for name in ["alpha", "beta", "gamma"] {
        let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
        source.push_str(&format!("fn {name}() {{\n"));
        for line in &BLOCK[..3] {
            source.push_str(line);
            source.push('\n');
        }
        source.push('\n');
        for line in &BLOCK[3..6] {
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("}\n\n");
        let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
        functions.push(function(name, first, last));
    }
    assert!(check(&source, functions).is_empty());
}

#[test]
fn struct_tails_without_executable_lines_stay_silent() {
    let mut source = String::new();
    let mut functions = Vec::new();
    for name in ["alpha", "beta", "gamma"] {
        let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
        source.push_str(&format!("fn {name}() {{\n"));
        source.push_str(&format!("let rendered_{name} = Widget {{\n"));
        for line in TAIL {
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("};\n}\n\n");
        let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
        functions.push(function(name, first, last));
    }
    assert!(check(&source, functions).is_empty());
}

#[test]
fn shared_attribute_line_keeps_copies_distinguishable() {
    const STEPS: [&str; 7] = [
        "let step_zero = begin_phase(zero_input);",
        "let step_one = advance_phase(step_zero);",
        "let step_two = advance_phase(step_one);",
        "let step_three = advance_phase(step_two);",
        "let step_four = advance_phase(step_three);",
        "let step_five = advance_phase(step_four);",
        "let step_six = finish_phase(step_five);",
    ];
    let mut source = String::new();
    let mut functions = Vec::new();
    for name in ["alpha", "beta", "gamma"] {
        let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
        source.push_str(&format!("fn {name}() {{\n"));
        source.push_str("#[must_use]\n");
        for line in STEPS {
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("}\n\n");
        let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
        functions.push(function(name, first, last));
    }
    let found = check(&source, functions);
    assert_eq!(found.len(), 1);
    assert!(found[0].starts_with("3 identical 9-line blocks"));
}

#[test]
fn top_level_runs_without_functions_stay_silent() {
    let mut source = String::new();
    for _ in 0..3 {
        for line in BLOCK {
            source.push_str(line);
            source.push('\n');
        }
        source.push('\n');
    }
    assert!(check(&source, Vec::new()).is_empty());
}

fn finding_lines_with_test_lines(test_lines: std::ops::RangeInclusive<u32>) -> Vec<u32> {
    let mut source = String::new();
    let mut functions = Vec::new();
    for name in ["probe", "alpha", "beta", "gamma"] {
        let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
        source.push_str(&format!("fn {name}() {{\n"));
        for line in BLOCK {
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("}\n\n");
        let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
        functions.push(function(name, first, last));
    }
    let mut file = facts(&source, functions);
    file.regions = vec![crate::role::RoleRegion {
        start_line: *test_lines.start(),
        end_line: *test_lines.end(),
        role: crate::role::CodeRole::Test,
    }];
    let rule = UniquePatchAnchors {
        window: 8,
        min_repeats: 3,
    };
    let mut out = Vec::new();
    rule.check(&file, &mut out);
    out.iter().map(|finding| finding.line).collect()
}

#[test]
fn finding_sits_on_the_first_copy_of_the_least_lowered_role() {
    assert_eq!(
        finding_lines_with_test_lines(1..=10),
        [13],
        "a test copy before three production copies leaves the finding in production"
    );
    assert_eq!(
        finding_lines_with_test_lines(34..=43),
        [2],
        "a later test copy does not move the finding"
    );
    assert_eq!(
        finding_lines_with_test_lines(1..=43),
        [2],
        "copies of one role keep the first copy"
    );
}

#[test]
fn preset_pins_off_at_standard_and_info_at_strict() {
    assert_eq!(
        INFO.confidence,
        Confidence::Low,
        "unique-patch-anchors ships at low confidence"
    );
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
    let strict_preset = preset(Strictness::Strict);
    assert_eq!(
        strict_preset.level,
        Level::At(Severity::Info),
        "strict reports at info"
    );
    let rule = build(strict_preset.options, &Settings::default()).expect("strict preset builds");
    let (source, functions) = bannered_block_source();
    let mut out = Vec::new();
    rule.check(&facts(&source, functions), &mut out);
    assert_eq!(
        out.len(),
        1,
        "strict flags the shared block on a positive input"
    );
}
