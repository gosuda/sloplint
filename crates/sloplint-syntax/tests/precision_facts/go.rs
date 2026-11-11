use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, AssertShape, Stmt};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> sloplint_syntax::Analysis<'_> {
    let input = FileInput {
        lang: Lang::Go,
        path: "pkg/sample.go",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn parenthesized_callee_is_not_a_simple_path() {
    let analysis = facts_of(
        "package sample\nfunc Up(s string) string { return (strings.ToUpper)(s) }\nfunc Low(s string) string { return strings.ToLower(s) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let paren = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "strings.ToUpper")
        .expect("has parenthesized call");
    assert!(
        !paren.is_simple_path,
        "call through a parenthesized expression is not a simple path"
    );
    assert!(
        paren.callee_root.is_none(),
        "parenthesized chain has no root, got {:?}",
        paren.callee_root
    );
    let direct = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "strings.ToLower")
        .expect("has direct call");
    assert!(
        direct.is_simple_path,
        "plain qualified call stays a simple path"
    );
    assert_eq!(
        direct.callee_root,
        Some("strings"),
        "plain qualified call keeps its root"
    );
}

#[test]
fn comments_are_not_call_arguments() {
    let analysis = facts_of(
        "package sample\nfunc Greet(n string) string { return render(n) }\nfunc Loud(n string) string { return render(/* locale */ n) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    for call in analysis.facts.calls.iter().filter(|c| c.callee == "render") {
        assert_eq!(
            call.args.len(),
            1,
            "comment inside the argument list is not an argument, got {:?}",
            call.args
        );
        assert_eq!(
            call.args[0].shape,
            ArgShape::Ident("n"),
            "the real argument keeps its shape"
        );
    }
}

#[test]
fn comments_are_not_assertion_values() {
    let analysis = facts_of(
        "package sample\nfunc Check() { assert.True(t, /* live */ ready) }\nfunc Loud() { assert.True(t, ready, \"must be ready\") }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.assertions.len(),
        2,
        "both assertions recorded, got {:?}",
        analysis.facts.assertions
    );
    assert!(
        !analysis.facts.assertions[0].has_message,
        "a comment is not a message argument"
    );
    assert_eq!(
        analysis.facts.assertions[0].shape,
        AssertShape::BareName,
        "the value after the comment keeps its shape"
    );
    assert!(
        analysis.facts.assertions[1].has_message,
        "a trailing message still counts"
    );
}

#[test]
fn leading_comment_keeps_sole_call() {
    let analysis = facts_of(
        "package sample\nfunc Greet(n string) string {\n// renders the greeting\nreturn render(n)\n}\nfunc Loud(n string) string {\n// loud\nm := render(n)\nreturn m\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let greet = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Greet")
        .expect("has Greet");
    let index = greet
        .sole_call
        .expect("comment does not hide the sole call");
    let at = usize::try_from(index).expect("call index fits in usize");
    assert_eq!(
        analysis.facts.calls.get(at).map(|c| c.callee),
        Some("render"),
        "the sole call is the render call"
    );
    let loud = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Loud")
        .expect("has Loud");
    assert!(
        loud.sole_call.is_none(),
        "two statements are not a sole call, got {:?}",
        loud.sole_call
    );
}

#[test]
fn deprecated_doc_marks_function() {
    let analysis = facts_of(
        "package sample\n// Deprecated: use Parse2.\nfunc Parse(s string) string { return Parse2(s) }\n// Parse2 parses.\nfunc Parse2(s string) string { return s }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let parse = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Parse")
        .expect("has Parse");
    assert!(
        parse.is_deprecated,
        "Deprecated: doc marks the function deprecated"
    );
    let parse2 = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Parse2")
        .expect("has Parse2");
    assert!(
        !parse2.is_deprecated,
        "ordinary doc leaves the function unmarked"
    );
}

#[test]
fn value_return_sets_has_value_return() {
    let analysis = facts_of(
        "package sample\nfunc Load() (int, error) { return 1, nil }\nfunc Nope() { return }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let load = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Load")
        .expect("has Load");
    assert!(load.has_value_return, "a value-bearing return is recorded");
    let nope = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Nope")
        .expect("has Nope");
    assert!(!nope.has_value_return, "a bare return records no value");
}

#[test]
fn wrapped_error_still_rethrows() {
    let analysis = facts_of(
        "package sample\nfunc load() error { if err := g(); err != nil { return fmt.Errorf(\"save: %w\", err) } }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the error check is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "a return carrying the bound error is a rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let analysis = facts_of(
        "package sample\nfunc save() error { if err := g(); err != nil { return fmt.Errorf(\"save failed\") } }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the error check without the bound error is still a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Other]),
        "a return without the bound error stays Other, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

#[test]
fn non_error_nil_check_emits_no_handler() {
    let analysis = facts_of("package sample\nfunc f() { cfg, _ := load()\nif cfg != nil { } }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert!(
        analysis.facts.handlers.is_empty(),
        "a nil check on a non-error value is not a handler, got {:?}",
        analysis.facts.handlers
    );
    let analysis = facts_of("package sample\nfunc f() { if err := g(); err != nil { } }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the error check keeps its handler"
    );
}

#[test]
fn chained_call_callee_drops_receiver_call_text() {
    let analysis = facts_of(
        "package sample\nfunc Save(v string) { logrus.WithField(\"k\", v).Error(\"save failed\") }\nfunc Note(m string) { slog.Info(m) }\nfunc Run() { func() { println(\"x\") }() }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let chained = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "Error")
        .expect("has the outer chained call");
    assert_eq!(
        chained.args.len(),
        1,
        "the outer call keeps its own argument, got {:?}",
        chained.args
    );
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee == "slog.Info"),
        "a clean chain keeps its dotted text"
    );
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee.is_empty()),
        "an immediately invoked closure carries no body text"
    );
}

#[test]
fn handler_call_uses_short_callee() {
    let analysis = facts_of(
        "package sample\nfunc f(v string) { if err := g(); err != nil { logrus.With(\"k\", v).Error(\"x\") } }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the error check is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Call { callee, .. }] if *callee == "Error"),
        "the handler call drops the receiver call text, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

#[test]
fn deferred_closure_handler_is_cleanup() {
    let analysis =
        facts_of("package sample\nfunc f() { defer func() { if err := g(); err != nil { } }() }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the deferred error check is a handler"
    );
    assert!(
        analysis.facts.handlers[0].in_cleanup,
        "an error check inside a deferred closure is cleanup"
    );
    let analysis = facts_of("package sample\nfunc f() { if err := g(); err != nil { } }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the plain error check is a handler"
    );
    assert!(
        !analysis.facts.handlers[0].in_cleanup,
        "a plain error check is not cleanup"
    );
}

#[test]
fn closure_sees_outer_error_param() {
    let analysis = facts_of(
        "package sample\nfunc f(err error) { g(func() { if err != nil { log.Print(err) } }) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "the closure check over the outer error is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Call { .. }]),
        "the closure body keeps its call, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let analysis =
        facts_of("package sample\nfunc f(cfg Config) { g(func() { if cfg != nil { } }) }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert!(
        analysis.facts.handlers.is_empty(),
        "the closure check over a non-error value is not a handler, got {:?}",
        analysis.facts.handlers
    );
}

#[test]
fn leading_comment_does_not_defeat_literal_exemption() {
    let analysis =
        facts_of("package sample\nfunc f(v Value) { v.MethodByName(/* name */ \"Run\") }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert!(
        analysis.facts.dynamic_symbols.is_empty(),
        "a literal name after a comment stays exempt, got {:?}",
        analysis.facts.dynamic_symbols
    );
    let analysis =
        facts_of("package sample\nfunc f(v Value, name string) { v.MethodByName(name) }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.dynamic_symbols.len(),
        1,
        "a computed name is still reported"
    );
    assert_eq!(
        analysis.facts.dynamic_symbols[0].what, "reflection by name",
        "the report keeps its kind"
    );
}

#[test]
fn comment_in_literal_keeps_field_source() {
    let analysis = facts_of(
        "package sample\nfunc f() { x := Config{\n// copied from y\nA: y.A,\nB: y.B,\nC: y.C,\nD: y.D,\nE: y.E,\nF: y.F,\n} }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.field_copies.len(),
        1,
        "the literal is recorded"
    );
    assert_eq!(
        analysis.facts.field_copies[0].fields, 6,
        "the comment is not a field"
    );
    assert_eq!(
        analysis.facts.field_copies[0].source,
        Some("y"),
        "the comment does not poison the source"
    );
}

#[test]
fn keyed_literal_reports_common_source() {
    let analysis = facts_of(
        "package sample\nfunc f() { p := Point{X: s.X, Y: s.Y, Z: s.Z, W: s.W, V: s.V, U: s.U} }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.field_copies.len(),
        1,
        "the literal is recorded"
    );
    assert_eq!(
        analysis.facts.field_copies[0].source,
        Some("s"),
        "the shared source is reported"
    );
    let analysis = facts_of(
        "package sample\nfunc f() { p := Point{X: s.X, Y: t.Y, Z: s.Z, W: s.W, V: s.V, U: s.U} }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.field_copies.len(),
        1,
        "the mixed literal is recorded"
    );
    assert!(
        analysis.facts.field_copies[0].source.is_none(),
        "mixed sources stay sourceless, got {:?}",
        analysis.facts.field_copies[0].source
    );
}

#[test]
fn left_nested_concat_carries_longest_run() {
    let analysis = facts_of(
        "package sample\nfunc F(x string, y string) {\nlog.Print(\"a\" + x + y)\nlog.Print(x + y)\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<&ArgShape> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log.Print")
        .map(|c| &c.args[0].shape)
        .collect();
    assert_eq!(shapes.len(), 2, "two print calls, got {:?}", shapes.len());
    assert!(
        shapes
            .iter()
            .any(|shape| **shape == ArgShape::Concat { longest_static: 1 }),
        "a deep-left chain carries its 1-char run"
    );
    assert!(
        shapes.iter().any(|shape| **shape == ArgShape::Other),
        "a string-free addition stays Other"
    );
}

#[test]
fn method_receiver_binding_reaches_function_fact() {
    let analysis = facts_of(
        "package sample\nfunc (s *Svc) Do(x int) int { return x }\nfunc (*Svc) Reset() {}\nfunc Free(x int) int { return x }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let named = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Do")
        .expect("has Do");
    assert_eq!(
        named.receiver,
        Some("s"),
        "the named receiver binding is recorded"
    );
    let unnamed = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Reset")
        .expect("has Reset");
    assert_eq!(
        unnamed.receiver, None,
        "an unnamed receiver records nothing"
    );
    let free = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Free")
        .expect("has Free");
    assert_eq!(free.receiver, None, "a free function has no receiver");
}
