use std::sync::LazyLock;

use sloplint_core::facts::{AmbientKind, ArgShape, ScrutineeKind};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::ObjC,
        path: "src/main.m",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn function_of<'a>(
    analysis: &'a Analysis<'a>,
    name: &str,
) -> &'a sloplint_core::facts::Function<'a> {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("has function {name}"))
}

#[test]
fn file_scope_c_main_is_entry_point() {
    let analysis = facts_of(
        "int helper(void) { return 1; }\nint main(int argc, char *argv[]) { return 0; }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let main = function_of(&analysis, "main");
    assert!(main.entry_point, "file-scope main is the program entry");
    let helper = function_of(&analysis, "helper");
    assert!(
        !helper.entry_point,
        "ordinary exported function is not an entry"
    );
}

#[test]
fn switch_default_reports_scrutinee_and_variants() {
    let analysis = facts_of(
        "int pick(int state) {\nswitch (state) {\ncase S_IDLE:\nreturn 0;\ncase S_RUN:\nreturn 1;\ndefault:\nreturn -1;\n}\n}\nint single(int mode) {\nswitch (mode) {\ncase MODE_A:\nreturn 1;\ndefault:\nreturn 0;\n}\n}\nint numeric(int code) {\nswitch (code) {\ncase 1:\nreturn 10;\ncase 2:\nreturn 20;\ndefault:\nreturn 0;\n}\n}\nstruct Cfg { int mode; };\nint pick_field(struct Cfg *cfg) {\nswitch (cfg->mode) {\ncase M_A:\nreturn 1;\ncase M_B:\nreturn 2;\ndefault:\nreturn 0;\n}\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        4,
        "one arm per default, got {:?}",
        analysis.facts.wildcard_arms
    );
    let ident = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.scrutinee == ScrutineeKind::Ident && a.variant_count == 2)
        .unwrap_or_else(|| panic!("has two-label ident switch"));
    assert!(ident.enum_like, "two identifier labels are enum-like");
    let single = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 1)
        .unwrap_or_else(|| panic!("has single-label switch"));
    assert!(!single.enum_like, "one label is not an enum-like arm");
    let numeric = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 0)
        .unwrap_or_else(|| panic!("has numeric-label switch"));
    assert!(
        !numeric.enum_like,
        "number labels are not named enum variants"
    );
    assert_eq!(
        numeric.scrutinee,
        ScrutineeKind::Ident,
        "the subject is still classified"
    );
    let field = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.scrutinee == ScrutineeKind::Field && a.variant_count == 2)
        .unwrap_or_else(|| panic!("has member-subject switch"));
    assert!(field.enum_like, "two labels stay enum-like over a member");
}

#[test]
fn callee_chains_root_the_leftmost_identifier() {
    let analysis = facts_of(
        "struct Ops { int (*measure)(int k); };\nstatic struct Ops *ops;\nint read_ops(int k) { return ops->measure(k); }\nint pick(int (**table)(int), int i) { return table[i](i); }\nvoid run(void) {\n[[a init] retain];\n[[NSMutableArray alloc] initWithCapacity:n];\n[obj.prop reload];\n[self.repo save];\n[$target ping];\n[build() step];\n[3 advance];\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let mut calls = analysis
        .facts
        .calls
        .iter()
        .filter(|c| {
            matches!(
                c.callee,
                "advance"
                    | "alloc"
                    | "initWithCapacity"
                    | "ops->measure"
                    | "ping"
                    | "reload"
                    | "retain"
                    | "save"
                    | "step"
                    | "table[i]"
            )
        })
        .map(|c| (c.callee, c.callee_root, c.is_simple_path))
        .collect::<Vec<_>>();
    calls.sort_by_key(|entry| entry.0);
    let expected: Vec<(&str, Option<&str>, bool)> = vec![
        ("advance", None, false),
        ("alloc", Some("NSMutableArray"), true),
        ("initWithCapacity", None, false),
        ("ops->measure", Some("ops"), true),
        ("ping", Some("$target"), true),
        ("reload", Some("obj"), true),
        ("retain", None, false),
        ("save", Some("self"), true),
        ("step", None, false),
        ("table[i]", None, false),
    ];
    assert_eq!(
        calls, expected,
        "roots follow identifier chains, got {calls:?}"
    );
}

#[test]
fn adjacent_literals_join_into_one_run() {
    let analysis = facts_of(
        "void report(void) {\nlog_error(\"abc \" \"defgh\");\nlog_error(\"oops\");\nlog_error(\"a\" + \"b\");\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log_error")
        .map(|c| c.args.first().map(|a| &a.shape))
        .collect::<Vec<_>>();
    assert_eq!(shapes.len(), 3, "three log calls, got {shapes:?}");
    let mut statics: Vec<u32> = Vec::new();
    for shape in &shapes {
        if let Some(ArgShape::Str { longest_static }) = shape {
            statics.push(*longest_static);
        }
    }
    statics.sort_unstable();
    assert_eq!(
        statics,
        vec![4, 9],
        "adjacent pieces join into one run, got {shapes:?}"
    );
    assert_eq!(
        shapes
            .iter()
            .filter(|s| matches!(s, Some(ArgShape::Concat { .. })))
            .count(),
        1,
        "plus concat still detects Concat, got {shapes:?}"
    );
}

#[test]
fn concat_carries_its_static_run() {
    let analysis = facts_of(
        "void report(int detail, int code) {\nlog_error(\"failure: \" + detail);\nlog_error(\"err: \" + detail);\nlog_error(\"\" + code);\nlog_error(\"aaaa\" TAG \"bbbb\");\nlog_error(\"aa\" \"bb\" + \"c\");\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let runs = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log_error")
        .map(|c| {
            c.args.first().and_then(|a| match &a.shape {
                ArgShape::Concat { longest_static } => Some(*longest_static),
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![Some(9), Some(5), Some(0), Some(4), Some(4)],
        "long, short, empty, gapped, and joined-operand runs, got {runs:?}"
    );
}

#[test]
fn trailing_identifier_keeps_the_joined_run() {
    let analysis = facts_of("void report(void) {\nNSLog(@\"Fail: \" @\"x: \" ERR_TAG);\n}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let runs = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "NSLog")
        .map(|c| {
            c.args.first().and_then(|a| match &a.shape {
                ArgShape::Concat { longest_static } => Some(*longest_static),
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![Some(9)],
        "adjacent pieces join before the identifier, got {runs:?}"
    );
}

#[test]
fn keyed_and_block_literals_leave_collection() {
    let analysis = facts_of(
        "struct P { int x; int y; };\nvoid run(void) {\ntake((struct P){ .x = 1, .y = 2 });\ntake((int[]){ 1, ^{ return 2; } });\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "take")
        .map(|c| c.args.first().map(|a| &a.shape))
        .collect::<Vec<_>>();
    assert_eq!(
        shapes,
        vec![Some(&ArgShape::Other), Some(&ArgShape::CodeBody)],
        "keyed is Other, block is CodeBody, got {shapes:?}"
    );
}

#[test]
fn plain_array_literals_stay_collection() {
    let analysis =
        facts_of("void run(void) {\ntake((int[]){ 1, 2, 3 });\ntake(@[ @\"a\", @\"b\" ]);\n}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "take")
        .map(|c| c.args.first().map(|a| &a.shape))
        .collect::<Vec<_>>();
    assert_eq!(
        shapes,
        vec![Some(&ArgShape::Collection), Some(&ArgShape::Collection)],
        "plain literals stay Collection, got {shapes:?}"
    );
}

#[test]
fn nsstring_out_parameter_is_not_a_string() {
    let analysis = facts_of("void take(NSString *name, NSString **out) {}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let kinds: Vec<sloplint_core::facts::PrimitiveKind> = function_of(&analysis, "take")
        .params
        .iter()
        .map(|p| p.primitive_kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            sloplint_core::facts::PrimitiveKind::Str,
            sloplint_core::facts::PrimitiveKind::Other
        ],
        "one level stays Str, two levels do not, got {kinds:?}"
    );
}

#[test]
fn every_top_level_declarator_is_a_global_mutable() {
    let analysis = facts_of("int ga = 1, gb = 2;\nconst int gc = 3;\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let mut mutables: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name)
        .collect();
    mutables.sort_unstable();
    assert_eq!(
        mutables,
        vec!["ga", "gb"],
        "both declarators emit, const stays silent, got {mutables:?}"
    );
}
