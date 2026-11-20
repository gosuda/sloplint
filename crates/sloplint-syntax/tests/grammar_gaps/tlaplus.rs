//! TLA+ (with PlusCal) shapes the adapter reads, observed through a raw-tree
//! walk because a hidden MISSING recovery node sets `has_error` without
//! producing a cursor-visible error position.
//!
//! The vendored grammar in `vendor/tree-sitter-tlaplus` is upstream HEAD
//! `add40814` (2026-02-08), not the published `tree-sitter-tlaplus` 1.5.0
//! crate (2024-10-20). HEAD is pinned because two scanner correctness fixes
//! postdate the release (`8d749f9a fix(scanner): correct UB in nested_scan`
//! and the `add40814` const-parameter fix); the scanner handles nested proofs
//! and conjunction/disjunction lists. The controls below pin the node shapes
//! those fixes protect, so a re-vendor that drops them shows up here.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Tlaplus,
        path,
        source,
        role: CodeRole::Library,
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

fn parse_raw(source: &str) -> tree_sitter::Tree {
    let language: tree_sitter::Language = tree_sitter_tlaplus::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("tlaplus language loads");
    parser.parse(source, None).expect("parser returns a tree")
}

/// True when the tree holds a visible ERROR node or a MISSING node anywhere.
/// When the walk finds no such node but the root still reports an error, the
/// recovery is hidden, so the root position is reported instead of silence.
fn first_problem(tree: &tree_sitter::Tree) -> Option<String> {
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return Some(format!("{} at {}", node.kind(), node.start_position()));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return tree
                    .root_node()
                    .has_error()
                    .then(|| format!("error at {}", tree.root_node().start_position()));
            }
        }
    }
}

const MODULE: &str = r#"---- MODULE Demo ----
EXTENDS Naturals

Op(a, b) == a + b

Fib[n \in Nat] == IF n < 2 THEN n ELSE Fib[n - 1] + Fib[n - 2]

AllPositive(S) == \A x \in S : x > 0

CaseOp(x) == CASE x = 0 -> "zero" [] x = 1 -> "one" [] OTHER -> "other"

===============================================================================
"#;

#[test]
fn core_definitions_parse_clean() {
    let tree = parse_raw(MODULE);
    assert!(
        first_problem(&tree).is_none(),
        "module must parse clean, got {:?}",
        first_problem(&tree)
    );
    let sexp = tree.root_node().to_sexp();
    assert!(
        !sexp.contains("ERROR") && !sexp.contains("MISSING"),
        "raw tree must hold no ERROR or MISSING node, got {sexp}"
    );
    let analysis = run_clean("src/demo.tla", MODULE);
    let names: Vec<_> = analysis.facts.functions.iter().map(|f| f.name).collect();
    for want in ["Op", "Fib", "AllPositive", "CaseOp"] {
        assert!(
            names.contains(&want),
            "{want} must surface as a function, got {names:?}"
        );
    }
}

#[test]
fn pluscal_while_counts_toward_procedure_depth() {
    let source = "---- MODULE PcDepth ----\n(*\n--algorithm Counter\nprocedure Work(n) begin L1: while n > 0 do n := n - 1; end while; return; end procedure;\nbegin\n  call Work(3);\nend algorithm;\n*)\nSpec == TRUE\n============================================================================\n";
    let analysis = run_clean("src/pcdepth.tla", source);
    let tree = parse_raw(source);
    assert!(
        first_problem(&tree).is_none(),
        "pluscal procedure must parse clean, got {:?}",
        first_problem(&tree)
    );
    let work = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Work")
        .expect("Work must be a function fact");
    assert_eq!(
        work.max_depth, 1,
        "one pluscal while must reach depth 1, got {}",
        work.max_depth
    );
    let params: Vec<_> = work.params.iter().map(|p| p.name).collect();
    assert_eq!(params, vec!["n"], "Work params must be [n], got {params:?}");
}

#[test]
fn operator_parameters_surface_in_order() {
    let analysis = run_clean("src/demo.tla", MODULE);
    let op = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Op")
        .expect("Op must be a function fact");
    let params: Vec<_> = op.params.iter().map(|p| p.name).collect();
    assert_eq!(
        params,
        vec!["a", "b"],
        "Op params must be [a, b], got {params:?}"
    );
    let fib = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Fib")
        .expect("Fib must be a function fact");
    let fib_params: Vec<_> = fib.params.iter().map(|p| p.name).collect();
    assert_eq!(
        fib_params,
        vec!["n"],
        "Fib params must be [n], got {fib_params:?}"
    );
}

#[test]
fn quantifiers_count_toward_function_depth() {
    let analysis = run_clean(
        "src/deep.tla",
        "---- MODULE Deep ----\nDeep(S) == \\A x \\in S : \\A y \\in S : x = y\n================================================================================\n",
    );
    let deep = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Deep")
        .expect("Deep must be a function fact");
    assert_eq!(
        deep.max_depth, 2,
        "two nested quantifiers must reach depth 2, got {}",
        deep.max_depth
    );
}

#[test]
fn nested_proof_parse_clean() {
    let source = "---- MODULE Pf ----\nTHEOREM \\A x \\in Nat : x = x\nPROOF\n  <1>1. TRUE\n    BY DEF TRUE\n  <1> QED\n    BY <1>1\n===============================================================================\n";
    let tree = parse_raw(source);
    assert!(
        first_problem(&tree).is_none(),
        "nested proof must parse clean, got {:?}",
        first_problem(&tree)
    );
    let sexp = tree.root_node().to_sexp();
    for shape in [
        "(theorem",
        "(non_terminal_proof",
        "(proof_step",
        "(qed_step",
    ] {
        assert!(
            sexp.contains(shape),
            "proof tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn pluscal_program_parse_clean() {
    let source = "---- MODULE Pc ----\n(*\n--algorithm Counter\nvariables i = 0;\nbegin\n  while i < 10 do\n    i := i + 1;\n  end while;\nend algorithm;\n*)\nSpec == TRUE\n============================================================================\n";
    let tree = parse_raw(source);
    assert!(
        first_problem(&tree).is_none(),
        "pluscal program must parse clean, got {:?}",
        first_problem(&tree)
    );
    let sexp = tree.root_node().to_sexp();
    for shape in ["(pcal_algorithm", "(pcal_while", "(pcal_assign"] {
        assert!(
            sexp.contains(shape),
            "pluscal tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn vendored_example_parses_clean() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/tree-sitter-tlaplus/test/examples/Highlight.tla"
    );
    let source =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path} must be readable: {e}"));
    let tree = parse_raw(&source);
    assert!(
        first_problem(&tree).is_none(),
        "Highlight.tla must parse clean, got {:?}",
        first_problem(&tree)
    );
}

#[test]
fn invalid_tlaplus_still_reports_a_parse_error() {
    let analysis = run("src/bad.tla", "---- MODULE Bad ----\nOp ==\n====\n");
    assert!(
        analysis.parse_error.is_some(),
        "a truncated operator body must not parse cleanly"
    );
}

#[test]
fn pluscal_macro_body_stays_inside_the_function_scope() {
    let source = "---- MODULE Pm ----\n(*\n--algorithm A\nvariables x = 1;\nmacro m(a) begin\n  x := a;\nend macro;\nbegin\n  m(2);\nend algorithm;\n*)\n====\n";
    let analysis = run_clean("src/pm.tla", source);
    let m = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "m")
        .expect("macro head must surface");
    // The macro (not just its signature) owns the body scope: its span
    // covers `end macro;` at line 7, not just the header on line 5.
    assert_eq!(m.end_line, 7, "macro function must own its body scope");
}

#[test]
fn pluscal_call_args_and_parallel_assign_writes() {
    let source = "---- MODULE Pw ----\n(*\n--algorithm A\nprocedure P(a) variables v = 0, w = 0;\nbegin\n  v := a || w := a;\n  PrintT(\"lit\");\n  return;\nend procedure;\nbegin\n  call P(1);\nend algorithm;\n*)\n====\n";
    let analysis = run_clean("src/pw.tla", source);
    let bound: Vec<&str> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    for want in ["v", "w"] {
        assert!(
            bound.contains(&want),
            "procedure variable {want} declared: {bound:?}"
        );
    }
    // Decl line is 4 (`variables v = 0, w = 0`); the parallel assignment
    // `v := a || w := a` is line 6. insert_decls stamps last_write_line
    // with the decl line, so a real write is proven by it moving to 6.
    for want in ["v", "w"] {
        assert!(
            analysis
                .facts
                .bindings
                .iter()
                .any(|b| b.name == want && b.last_write_line == 6),
            "parallel assign writes {want}"
        );
    }
    let print = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "PrintT")
        .expect("PrintT call recorded");
    assert_eq!(print.args.len(), 1, "PrintT arg must be kept");
}

#[test]
fn let_local_definitions_stay_private() {
    let source = "---- MODULE L ----\nF == LET x == 1\n         y == 2\n     IN x + y\n====\n";
    let analysis = run_clean("src/l.tla", source);
    for want in ["x", "y"] {
        let f = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == want)
            .unwrap_or_else(|| panic!("LET-local {want} is a function fact"));
        assert_eq!(
            f.visibility,
            sloplint_core::facts::Visibility::Private,
            "LET-local {want} must not be public"
        );
    }
}

#[test]
fn bound_op_calls_keep_args_and_skip_comments() {
    // PrintT("x", 1) parses as bound_op(name, parameter...), not
    // function_evaluation; args must still reach the call facts, and an
    // inline comment between them is not an argument.
    let source = "---- MODULE B ----\nF == PrintT(\"x\" (* note *), 1)\n====\n";
    let analysis = run_clean("src/b.tla", source);
    let print = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "PrintT")
        .expect("bound_op PrintT call recorded");
    assert_eq!(print.args.len(), 2, "bound_op args, comment excluded");
}

#[test]
fn prefixed_op_keeps_its_args_and_prefix_is_not_one() {
    // `M!Op(x)` parses as prefixed_op(prefix: M!, op: bound_op(name,
    // parameter: x)): the bound_op holds the real arguments.
    let source = "---- MODULE Q ----\nG == M!Op(1)\n====\n";
    let analysis = run_clean("src/q.tla", source);
    let op = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "Op")
        .expect("prefixed M!Op call recorded");
    assert_eq!(op.args.len(), 1, "bound_op parameter, prefix excluded");
}

#[test]
fn deep_nesting_survives_serialize_capacity() {
    // More nested PlusCal `with` blocks than the serialization buffer can
    // carry: 251 size slots alone exceed the 1024-byte buffer, so the
    // outermost contexts drop; they must restore as empty contexts at the
    // right depth, not corrupt the parse.
    let mut body = String::new();
    for i in 0..250 {
        body.push_str(&format!("    with v{i} = 0 do\n"));
    }
    body.push_str("    skip;\n");
    for _ in 0..250 {
        body.push_str("    end with;\n");
    }
    let source = format!(
        "---- MODULE Deep ----\n(*\n--algorithm A\nprocedure P(a) variables x = 0;\nbegin\n{body}end procedure;\nbegin\n  call P(1);\nend algorithm;\n*)\n====\n"
    );
    let analysis = run_clean("src/deep.tla", &source);
    let m = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "P")
        .expect("procedure P parsed");
    assert!(m.end_line > 0);
}
