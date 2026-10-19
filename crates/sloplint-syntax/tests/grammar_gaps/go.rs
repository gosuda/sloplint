//! Valid Go that tree-sitter-go 0.25.0 rejected, observed through `analyze` only.
//!
//! Each repro below is accepted by `gofmt` and the Go 1.27.2 compiler; the vendored
//! grammar in `vendor/tree-sitter-go` must parse it. The controls pin the facts the
//! Go adapter reads, so a shape regression in the grammar patch shows up here.

use std::sync::LazyLock;

use sloplint_core::facts::Stmt;
use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Go,
        path,
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let analysis = run(path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{path} must parse cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn callees<'a>(analysis: &'a Analysis<'_>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

fn function_names<'a>(analysis: &'a Analysis<'_>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

// G1: Go 1.26 `new` takes a value, and `new` may be shadowed by a user function.

#[test]
fn new_accepts_a_value_operand() {
    let source = "package p\n\
        var a = new(42)\n\
        var b = new(int64(-64))\n\
        var c = new(x + 1)\n\
        var d = new(T{1})\n\
        var e = new(f())\n";
    let analysis = run_clean("new_value.go", source);
    assert_eq!(
        callees(&analysis).iter().filter(|c| **c == "new").count(),
        5,
        "five `new` calls: {:?}",
        callees(&analysis)
    );
}

#[test]
fn shadowed_new_takes_ordinary_arguments() {
    let source = "package p\n\
        func new(name string, rest ...int) int { return len(rest) }\n\
        var a = new(\"self\", 1, 2)\n\
        var b = new(x.y)\n\
        var c = new(true)\n";
    let analysis = run_clean("shadowed_new.go", source);
    assert_eq!(function_names(&analysis), ["new"]);
    let three = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "new" && c.args.len() == 3)
        .expect("the three-argument call to the shadowed `new`");
    assert_eq!(three.callee, "new");
}

#[test]
fn make_and_new_keep_their_type_operands() {
    let source = "package p\n\
        func f(n int) {\n\
        \t_ = new(int)\n\
        \t_ = make([]int, n)\n\
        \t_ = make(map[string]int)\n\
        \t_ = new(struct{ x int })\n\
        }\n";
    let analysis = run_clean("make_new.go", source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(
        callees(&analysis),
        ["new", "make", "make", "new"],
        "type operands still parse as call arguments"
    );
    let two = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "make" && c.args.len() == 2)
        .expect("`make([]int, n)` keeps its length argument");
    assert_eq!(two.callee, "make");
}

// G2: generic method declarations are valid in Go 1.27 (`go build` accepts them with
// `go 1.27` in go.mod and reports "generic method requires go1.27 or later" for 1.26).

#[test]
fn generic_method_declaration_parses() {
    let source = "package p\n\
        type O[T any] struct{}\n\
        func (O[T]) G[U any](u U) U { return u }\n\
        func (o *O[T]) H[A, B comparable](a A, b B) {}\n\
        func (o O[T]) Plain(x int) int { return x }\n";
    let analysis = run_clean("generic_method.go", source);
    assert_eq!(function_names(&analysis), ["G", "H", "Plain"]);
    let g = &analysis.facts.functions[0];
    assert!(g.interface_bound, "a method declaration is interface bound");
    assert_eq!(g.params.len(), 1, "G takes one parameter");
    assert_eq!(g.params[0].name, "u");
    assert_eq!(g.params[0].ty, Some("U"));
    assert_eq!(g.return_type, Some("U"));
    let h = &analysis.facts.functions[1];
    assert_eq!(
        h.params.iter().map(|p| p.name).collect::<Vec<_>>(),
        ["a", "b"],
        "type parameters do not become parameters"
    );
    let plain = &analysis.facts.functions[2];
    assert_eq!(plain.params.len(), 1);
    assert_eq!(plain.params[0].name, "x");
    assert_eq!(plain.return_type, Some("int"));
}

// G3: a struct or interface type literal is a valid method-expression base.

#[test]
fn type_literal_method_expression_bases_parse() {
    let source = "package p\n\
        var a = (*struct{ J }).M\n\
        var b = interface{ m1(string) }.m1\n\
        var c = struct{ J }.M\n\
        var d = (**struct{ J }).M\n\
        func f() { _ = (*struct{ J }).M(nil, 1) }\n";
    let analysis = run_clean("type_literal_selector.go", source);
    assert_eq!(
        callees(&analysis),
        ["(*struct{ J }).M"],
        "the call keeps the method expression as its callee text"
    );
}

#[test]
fn named_method_expressions_keep_their_shape() {
    let source = "package p\n\
        func f() {\n\
        \t_ = (*T).M(nil)\n\
        \t_ = (T).M(t)\n\
        \t_ = (*pkg.T[int]).M(nil)\n\
        \t_ = (*interface{ M() })(nil)\n\
        }\n";
    let analysis = run_clean("named_method_expr.go", source);
    assert_eq!(
        callees(&analysis),
        ["(*T).M", "(T).M", "(*pkg.T[int]).M"],
        "parenthesized named types stay expression callees; the last line is a conversion, not a call"
    );
}

// G4: empty statements, empty initializers, and labels at the end of a block.

#[test]
fn empty_initializers_parse_and_keep_conditions() {
    let source = "package p\n\
        func f(x bool, v interface{}) {\n\
        \tif ; x {\n\
        \t}\n\
        \tswitch ; {\n\
        \t}\n\
        \tswitch ; x {\n\
        \t}\n\
        \tswitch ; v.(type) {\n\
        \t}\n\
        }\n\
        func g(err error) error {\n\
        \tif ; err != nil {\n\
        \t\treturn err\n\
        \t}\n\
        \treturn nil\n\
        }\n";
    let analysis = run_clean("empty_init.go", source);
    assert_eq!(function_names(&analysis), ["f", "g"]);
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "`if ; err != nil` is still a handler: {:?}",
        analysis.facts.handlers
    );
    assert_eq!(
        analysis.facts.handlers[0].stmts,
        [Stmt::Rethrow],
        "the handler returns the bound error"
    );
}

#[test]
fn empty_statements_need_no_terminator() {
    let source = "package p\n\
        func f() { ; println(\"x\") }\n\
        func g() { a(); ; b() }\n\
        func h() { a();; b() }\n\
        func i() {\n\
        \t;;\n\
        }\n";
    let analysis = run_clean("empty_statements.go", source);
    assert_eq!(function_names(&analysis), ["f", "g", "h", "i"]);
    assert_eq!(callees(&analysis), ["println", "a", "b", "a", "b"]);
}

#[test]
fn labels_at_the_end_of_a_block_nest() {
    let source = "package p\n\
        func f() {\n\
        L:\n\
        L1:\n\
        }\n\
        func g(x int) {\n\
        \ta()\n\
        L:\n\
        L1:\n\
        }\n\
        func h(x int) {\n\
        \tswitch x {\n\
        \tcase 1:\n\
        \tL:\n\
        \tL1:\n\
        \tcase 2:\n\
        \t}\n\
        }\n\
        func i() {\n\
        L:\n\
        L1:\n\
        \tb()\n\
        }\n";
    let analysis = run_clean("nested_labels.go", source);
    assert_eq!(function_names(&analysis), ["f", "g", "h", "i"]);
    assert_eq!(callees(&analysis), ["a", "b"]);
}

// G5: a const group may close on the line it opens.

#[test]
fn single_line_const_groups_parse() {
    let source = "package p\n\
        const ( F = 1 )\n\
        const ( G = 1; H = 2 )\n\
        const ( I int = 1; J; )\n\
        const ( )\n\
        const (\n\
        \tA = iota\n\
        \tB\n\
        )\n\
        func after() { const ( K = 3 ) }\n";
    let analysis = run_clean("const_group.go", source);
    assert_eq!(
        function_names(&analysis),
        ["after"],
        "declarations after a one-line group still parse"
    );
}

// The patched grammar must not turn invalid Go into valid Go.

#[test]
fn invalid_go_still_reports_a_parse_error() {
    let cases = [
        ("missing_condition.go", "package p\nfunc f() { if ; { } }\n"),
        ("open_call.go", "package p\nvar _ = new(\n"),
        ("double_equals.go", "package p\nconst ( F = = 1 )\n"),
        ("open_params.go", "package p\nfunc f( {}\n"),
        ("stray_colon.go", "package p\nfunc f() { L: : }\n"),
        ("method_no_params.go", "package p\nfunc (T) m[U any] {}\n"),
    ];
    for (path, source) in cases {
        let analysis = run(path, source);
        assert!(
            analysis.parse_error.is_some(),
            "{path} is invalid Go and must report a parse error"
        );
        assert!(
            analysis.facts.functions.is_empty(),
            "{path} yields no facts once the parse fails"
        );
    }
}
