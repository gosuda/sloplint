use std::sync::LazyLock;

use sloplint_core::facts::{AmbientKind, ArgShape, PrimitiveKind, ScrutineeKind};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::C,
        path: "src/main.c",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn file_scope_main_is_entry_point() {
    let analysis = facts_of(
        "int helper(void) { return 1; }\nint main(int argc, char **argv) { return argc; }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let main = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap_or_else(|| panic!("has main"));
    assert!(main.entry_point, "file-scope main is the program entry");
    let helper = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "helper")
        .unwrap_or_else(|| panic!("has helper"));
    assert!(
        !helper.entry_point,
        "ordinary exported function is not an entry"
    );
}

#[test]
fn member_call_is_simple_path() {
    let analysis = facts_of(
        "struct S { int k; };\nint area(struct S *s, int k) { return s->measure(k); }\nint pick(int (**table)(int), int i, int k) { return table[i](k); }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let member = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "s->measure")
        .unwrap_or_else(|| panic!("has member call"));
    assert!(
        member.is_simple_path,
        "identifiers joined by member access are a simple path"
    );
    assert_eq!(
        member.callee_root,
        Some("s"),
        "member chain roots at the receiver"
    );
    let indexed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "table[i]")
        .unwrap_or_else(|| panic!("has subscript call"));
    assert!(
        !indexed.is_simple_path,
        "an index link is not a simple path"
    );
    assert_eq!(
        indexed.callee_root, None,
        "a chain bottoming at an index has no root"
    );
}

#[test]
fn adjacent_strings_join_before_measuring() {
    let analysis = facts_of(
        "void log_error(const char *msg);\nvoid f(void) {\nlog_error(\"abc \" \"defgh\");\nlog_error(\"abc\");\nlog_error(\"abc \" /* why */ \"defgh\");\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let mut lengths: Vec<u32> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log_error")
        .filter_map(|c| match c.args.first().map(|a| &a.shape) {
            Some(ArgShape::Str { longest_static }) => Some(*longest_static),
            _ => None,
        })
        .collect();
    lengths.sort_unstable();
    assert_eq!(
        lengths,
        vec![3, 9, 9],
        "adjacent pieces join into one run, got {lengths:?}"
    );
}

#[test]
fn switch_default_reports_scrutinee_and_variants() {
    let analysis = facts_of(
        "struct S { int state; };\nint f(int state) {\nswitch (state) {\ncase S_IDLE:\nreturn 0;\ncase S_RUN:\nreturn 1;\ndefault:\nreturn -1;\n}\n}\nint g(int mode) {\nswitch (mode) {\ncase MODE_A:\nreturn 1;\ndefault:\nreturn 0;\n}\n}\nint h(struct S *s) {\nswitch (s->state) {\ncase S_IDLE:\nreturn 0;\ncase S_RUN:\nreturn 1;\ndefault:\nreturn -1;\n}\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        3,
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
    assert_eq!(
        single.scrutinee,
        ScrutineeKind::Ident,
        "single label still classifies the subject"
    );
    assert!(!single.enum_like, "one label is not an enum-like arm");
    let field = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.scrutinee == ScrutineeKind::Field)
        .unwrap_or_else(|| panic!("has member-access switch"));
    assert_eq!(
        field.variant_count, 2,
        "member subject counts its labels, got {}",
        field.variant_count
    );
}

#[test]
fn c_params_keep_primitive_kinds() {
    let analysis =
        facts_of("void configure(_Bool verbose, int count, char *name, float ratio) {}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let kinds: Vec<PrimitiveKind> = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "configure")
        .unwrap_or_else(|| panic!("has configure"))
        .params
        .iter()
        .map(|p| p.primitive_kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            PrimitiveKind::Bool,
            PrimitiveKind::Int,
            PrimitiveKind::Str,
            PrimitiveKind::Float
        ],
        "plain C declarators keep their kinds, got {kinds:?}"
    );
}

#[test]
fn getenv_call_stays_env_read() {
    let analysis = facts_of(
        "const char *a(void) { return getenv(\"HOME\"); }\nconst char *b(void) { return getenvx(\"HOME\"); }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let reads: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::EnvRead)
        .map(|a| a.name)
        .collect();
    assert_eq!(
        reads,
        vec!["getenv"],
        "exact env name still reports, got {reads:?}"
    );
}

#[test]
fn plus_operands_report_static_runs() {
    let analysis = facts_of(
        "void log_error(const char *msg);\nvoid f(const char *tag) {\nlog_error(\"abcdefgh\" + tag);\nlog_error(\"ab\" + tag);\nlog_error(\"\" + tag);\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let mut lengths: Vec<u32> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log_error")
        .filter_map(|c| match c.args.first().map(|a| &a.shape) {
            Some(ArgShape::Concat { longest_static }) => Some(*longest_static),
            _ => None,
        })
        .collect();
    lengths.sort_unstable();
    assert_eq!(
        lengths,
        vec![0, 2, 8],
        "operands measure 8, 2 and 0 static chars, got {lengths:?}"
    );
}

#[test]
fn keyed_literal_is_not_collection() {
    let analysis = facts_of(
        "struct S { int x; int y; };\nvoid draw(int *pixels);\nvoid f(void) {\ndraw((int[]){ 1, 2, 3 });\ndraw((struct S){ .x = 1, .y = 2 });\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let mut shapes = Vec::new();
    for c in &analysis.facts.calls {
        if c.callee == "draw" {
            let arg = c
                .args
                .first()
                .unwrap_or_else(|| panic!("draw has an argument"));
            shapes.push(arg.shape.clone());
        }
    }
    assert_eq!(
        shapes,
        vec![ArgShape::Collection, ArgShape::Other],
        "array literal stays Collection, keyed literal is Other, got {shapes:?}"
    );
}

#[test]
fn string_pointer_depth_and_signedness() {
    let analysis =
        facts_of("void parse(const char *input, char **out, const unsigned char *key) {}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let kinds: Vec<PrimitiveKind> = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "parse")
        .unwrap_or_else(|| panic!("has parse"))
        .params
        .iter()
        .map(|p| p.primitive_kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            PrimitiveKind::Str,
            PrimitiveKind::Other,
            PrimitiveKind::Other
        ],
        "plain char pointer stays Str, got {kinds:?}"
    );
}

#[test]
fn escape_and_macro_break_static_runs() {
    let analysis = facts_of(
        "void log_error(const char *msg);\nvoid f(void) {\nlog_error(\"oops\\n\" \"fail\");\nlog_error(\"abcd\" TAG \"efgh\");\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<ArgShape> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log_error")
        .map(|c| {
            c.args
                .first()
                .unwrap_or_else(|| panic!("log call has a message"))
                .shape
                .clone()
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            ArgShape::Str { longest_static: 4 },
            ArgShape::Concat { longest_static: 4 }
        ],
        "escapes and macros end the run, got {shapes:?}"
    );
}
