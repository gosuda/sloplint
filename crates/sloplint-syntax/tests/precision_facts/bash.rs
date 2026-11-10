use sloplint_core::facts::{ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct FnFacts {
    name: String,
    visibility: Visibility,
    signature_forced: bool,
    entry_point: bool,
    in_test: bool,
    is_overload: bool,
    is_deprecated: bool,
    is_decorated: bool,
    is_trait_item: bool,
}

fn functions_of(source: &str) -> Vec<FnFacts> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .functions
        .iter()
        .map(|f| FnFacts {
            name: f.name.to_owned(),
            visibility: f.visibility,
            signature_forced: f.signature_forced,
            entry_point: f.entry_point,
            in_test: f.in_test,
            is_overload: f.is_overload,
            is_deprecated: f.is_deprecated,
            is_decorated: f.is_decorated,
            is_trait_item: f.is_trait_item,
        })
        .collect()
}

fn calls_of(source: &str) -> Vec<(String, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .calls
        .iter()
        .map(|c| (c.callee.to_owned(), c.is_simple_path, c.initializes_binding))
        .collect()
}

fn arms_of(source: &str) -> Vec<(ScrutineeKind, u32, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .wildcard_arms
        .iter()
        .map(|a| {
            (
                a.scrutinee,
                a.variant_count,
                a.has_nullable_ctor,
                a.enum_like,
            )
        })
        .collect()
}

fn handler_assertion_counts(source: &str) -> (usize, usize) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    (
        analysis.facts.handlers.len(),
        analysis.facts.assertions.len(),
    )
}

#[test]
fn top_level_function_is_exported() {
    let rows = functions_of("deploy() {\n    echo ok\n}\n");
    assert_eq!(rows.len(), 1, "one function recorded");
    assert_eq!(rows[0].name, "deploy", "function name kept");
    assert_eq!(
        rows[0].visibility,
        Visibility::Exported,
        "bash function is exported"
    );
    let none = functions_of("echo hi\n");
    assert!(none.is_empty(), "command-only file has no functions");
}

#[test]
fn function_head_carries_no_forced_signature() {
    let rows = functions_of("main() {\n    echo hi\n}\n");
    assert_eq!(rows.len(), 1, "main function recorded");
    assert!(!rows[0].signature_forced, "bash never forces a signature");
    assert!(!rows[0].entry_point, "bash has no entry-point concept");
    assert!(!rows[0].in_test, "library file is not a test region");
    assert!(!rows[0].is_overload, "bash has no overloads");
    assert!(!rows[0].is_deprecated, "bash has no deprecation marker");
    assert!(!rows[0].is_decorated, "bash has no decorators");
    assert!(!rows[0].is_trait_item, "bash has no traits");
    let none = functions_of("FOO=bar\n");
    assert!(none.is_empty(), "assignment-only file has no functions");
}

#[test]
fn calls_carry_no_path_or_binding_facts() {
    let rows = calls_of("grep -r \"needle\" .\n");
    assert!(!rows.is_empty(), "command call recorded");
    for (callee, is_simple_path, initializes_binding) in &rows {
        assert_eq!(callee, "grep", "callee is the command name");
        assert!(!is_simple_path, "bash leaves is_simple_path at default");
        assert!(!initializes_binding, "plain command initializes no binding");
    }
    let nested = calls_of("result=$(grep -r \"needle\" .)\n");
    assert!(!nested.is_empty(), "substitution call recorded");
    for (_, _, initializes_binding) in &nested {
        assert!(
            !initializes_binding,
            "command substitution still leaves default"
        );
    }
}

#[test]
fn star_arm_reports_other_scrutinee() {
    let arms = arms_of("case \"$x\" in\nfoo) echo a ;;\n*) echo b ;;\nesac\n");
    assert_eq!(arms.len(), 1, "star arm recorded once");
    assert_eq!(
        arms[0].0,
        ScrutineeKind::Other,
        "bash scrutinee stays Other"
    );
    assert_eq!(arms[0].1, 0, "bash counts no variants");
    assert!(!arms[0].2, "bash has no nullable constructors");
    assert!(!arms[0].3, "star arm is not enum-like");
    let plain = arms_of("case \"$x\" in\nfoo) echo a ;;\nesac\n");
    assert!(plain.is_empty(), "literal-only case has no wildcard arm");
}

#[test]
fn trap_file_has_no_handler_or_assertion_facts() {
    let (handlers, assertions) = handler_assertion_counts("trap 'cleanup' EXIT\n");
    assert_eq!(handlers, 0, "bash emits no handler facts");
    assert_eq!(assertions, 0, "bash emits no assertion facts");
    let (plain_handlers, plain_assertions) = handler_assertion_counts("[[ -n $x ]] && echo yes\n");
    assert_eq!(plain_handlers, 0, "conditional file still has no handlers");
    assert_eq!(
        plain_assertions, 0,
        "conditional file still has no assertions"
    );
}
