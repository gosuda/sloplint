//! Dafny source exercising the shapes the sloplint adapter reads.
//!
//! The vendored grammar is the published tree-sitter-dafny 0.1.0 crate at
//! `8085572` with no local patch, so every valid case below parses against
//! the published grammar too. The tests pin the adapter-visible shapes (a
//! grammar change that renames or restructures them shows up here) and the
//! documented upstream gaps that must still error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Dafny,
        path: "gap.dfy",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_dafny::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("dafny language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean(source: &str) -> Analysis<'_> {
    assert_raw_clean(source);
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Dafny must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_error(source: &str) {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "invalid Dafny must still report a parse error:\n{source}"
    );
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

// --- Method with specification clauses -------------------------------------

#[test]
fn method_with_specs_parses_and_names_params() {
    let analysis = run_clean(concat!(
        "method Bump(delta: int := 1, verbose: bool) returns (r: int)\n",
        "  requires delta >= 0\n",
        "  ensures r >= 0\n",
        "  modifies this\n",
        "  decreases delta\n",
        "{\n",
        "  return delta;\n",
        "}\n",
    ));
    assert_eq!(function_names(&analysis), ["Bump"]);
    let params = &analysis.facts.functions[0].params;
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "delta");
    assert!(params[0].has_default);
    assert_eq!(params[1].name, "verbose");
}

// --- Function, predicate, lemma, constructor, iterator ----------------------

#[test]
fn function_predicate_lemma_constructor_iterator_parse() {
    let analysis = run_clean(concat!(
        "function Fib(n: nat): nat\n",
        "  decreases n\n",
        "{\n",
        "  if n < 2 then n else Fib(n - 1) + Fib(n - 2)\n",
        "}\n",
        "predicate Even(n: int)\n",
        "{\n",
        "  n % 2 == 0\n",
        "}\n",
        "lemma Helper(x: int)\n",
        "  requires x > 0\n",
        "{\n",
        "  assert x > 0;\n",
        "  var m := map k | 0 <= k < 3 :: k := k * k;\n",
        "}\n",
        "class Counter {\n",
        "  var n: int\n",
        "  constructor Init(start: int)\n",
        "  {\n",
        "    n := start;\n",
        "  }\n",
        "}\n",
        "iterator Gen(start: int) yields (y: int)\n",
        "{\n",
        "  yield start;\n",
        "}\n",
    ));
    assert_eq!(
        function_names(&analysis),
        ["Fib", "Even", "Helper", "Init", "Gen"]
    );
}

// --- Class, trait, datatype, newtype, synonym --------------------------------

#[test]
fn type_declarations_parse_and_name_types() {
    let analysis = run_clean(concat!(
        "trait HasId {\n",
        "  var id: int\n",
        "}\n",
        "class Counter extends HasId {\n",
        "  var n: int\n",
        "}\n",
        "datatype Tree = Leaf | Node(left: Tree, right: Tree)\n",
        "newtype Age = n: int | 0 <= n\n",
        "type MySet = set<int>\n",
    ));
    let mut types: Vec<&str> = analysis.facts.types.iter().map(|t| t.name).collect();
    types.sort_unstable();
    assert_eq!(types, ["Age", "Counter", "HasId", "MySet", "Tree"]);
}

// --- Control flow: if chains, loops, match, forall ---------------------------

#[test]
fn control_flow_parses() {
    let analysis = run_clean(concat!(
        "method Flow(t: int) returns (r: int)\n",
        "{\n",
        "  if t > 10 {\n",
        "    r := 2;\n",
        "  } else if t > 5 {\n",
        "    r := 1;\n",
        "  } else {\n",
        "    r := 0;\n",
        "  }\n",
        "  while r > 0\n",
        "    invariant r >= 0\n",
        "    decreases r\n",
        "  {\n",
        "    r := r - 1;\n",
        "  }\n",
        "  for i := 0 to 10 {\n",
        "    r := r + i;\n",
        "  }\n",
        "  forall x: nat | x < 5 {\n",
        "    print x, \"\\n\";\n",
        "  }\n",
        "  match r\n",
        "  case 0 => print \"zero\";\n",
        "  case _ => print \"other\";\n",
        "  return r;\n",
        "}\n",
    ));
    assert_eq!(function_names(&analysis), ["Flow"]);
}

// --- Expressions: calls, lambdas, quantifiers, displays ----------------------

#[test]
fn expression_forms_parse_and_record_calls() {
    let analysis = run_clean(concat!(
        "function Adder<T>(a: T, b: T): T\n",
        "{\n",
        "  a\n",
        "}\n",
        "method Use() {\n",
        "  var plain := Adder<int>(1, 2);\n",
        "  var f := (x: int) => x * 2;\n",
        "  var applied := f(21);\n",
        "  var q := forall x: int :: x + 0 == x;\n",
        "  var s := set x: nat | x < 10;\n",
        "  var m := map k | 0 <= k < 3 :: k := k * k;\n",
        "  var e := [1, 2, 3];\n",
        "  var cond := if plain > 0 then \"pos\" else \"neg\";\n",
        "  print cond, \"\\n\";\n",
        "}\n",
    ));
    let callees: Vec<&str> = analysis.facts.calls.iter().map(|c| c.callee).collect();
    assert!(callees.contains(&"Adder"), "calls: {callees:?}");
    assert!(callees.contains(&"f"), "calls: {callees:?}");
    assert!(callees.contains(&"print"), "calls: {callees:?}");
}

// --- Ghost, calc, and proof statements ---------------------------------------

#[test]
fn ghost_calc_and_proof_statements_parse() {
    run_clean(concat!(
        "lemma CalcLemma(x: int)\n",
        "  requires x > 0\n",
        "{\n",
        "  ghost var y := x + 1;\n",
        "  assert y > 1;\n",
        "  assume x != 0;\n",
        "  expect x > 0, \"positive\";\n",
        "  calc {\n",
        "    x;\n",
        "    == x + 0;\n",
        "  }\n",
        "  reveal Foo;\n",
        "}\n",
    ));
}

// --- Modules, imports, includes ----------------------------------------------

#[test]
fn module_forms_parse_and_record_imports() {
    let analysis = run_clean(concat!(
        "include \"lib.dfy\"\n",
        "module Shop {\n",
        "  import opened Lib\n",
        "  export E provides Foo reveals *\n",
        "  method Run() {\n",
        "    print \"run\\n\";\n",
        "  }\n",
        "}\n",
    ));
    let modules: Vec<&str> = analysis
        .facts
        .imports
        .iter()
        .map(|i| i.module.as_ref())
        .collect();
    assert!(modules.contains(&"lib.dfy"), "imports: {modules:?}");
    assert!(modules.contains(&"Lib"), "imports: {modules:?}");
}

// --- Known upstream gaps: still errors ----------------------------------------
//
// The grammar README lists float literals `.5` / `42.` among its remaining
// known issues. Pin them as errors so a fix shows up. (Empty `match` was
// listed there too, but the grammar accepts an armless `match_statement`.)

#[test]
fn leading_dot_float_still_errors() {
    run_error("method M() {\n  var x := .5;\n}\n");
}

#[test]
fn trailing_dot_float_still_errors() {
    run_error("method M() {\n  var x := 42.;\n}\n");
}

#[test]
fn function_body_match_wildcard_records_arm() {
    // A `case _` arm inside `fn_match_expression` (function bodies chain
    // continuations instead of statements) must record a wildcard arm, or
    // `agent/catch-all-match` stays silent for function-body matches.
    let analysis = run_clean(concat!(
        "function Describe(t: int): string\n",
        "{\n",
        "  match t case 0 => \"zero\" case _ => \"other\"\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert!(analysis.facts.wildcard_arms[0].enum_like);
}

#[test]
fn empty_match_parses_with_no_arms() {
    let analysis = run_clean("method M(x: int) {\n  match x {}\n}\n");
    assert!(analysis.facts.wildcard_arms.is_empty());
}

#[test]
fn var_decl_writes_are_tracked_on_the_binding() {
    // `var x := 0` is the declaration; a later `x := ...` must record a
    // write on the same binding — scoping `local_var_decl` to itself
    // would miss it.
    let analysis = run_clean(concat!(
        "method M() {\n",
        "  var x := 0;\n",
        "  x := x + 1;\n",
        "  x := x + 2;\n",
        "}\n",
    ));
    let binding = analysis
        .facts
        .bindings
        .iter()
        .find(|b| b.name == "x")
        .expect("x binding recorded");
    assert_eq!(binding.write_lines, 2);
    assert_eq!(binding.first_write_line, 3);
    assert!(binding.last_write_line > binding.first_write_line);
}

#[test]
fn chained_else_ifs_do_not_inflate_nesting() {
    let nested = run_clean(concat!(
        "method M(a: bool, b: bool) returns (r: int) {\n",
        "  r := if a then if b then 1 else 2 else 3;\n",
        "}\n",
    ));
    let m = &nested.facts.functions[0];
    assert!(
        m.max_depth >= 2,
        "an if-expression in the then position nests: {}",
        m.max_depth
    );

    let flat = run_clean(concat!(
        "method M(c: bool) {\n",
        "  if c {\n",
        "  } else if c {\n",
        "  }\n",
        "}\n",
    ));
    let m = &flat.facts.functions[0];
    assert_eq!(m.max_depth, 1, "an else-if chain counts once");
}

#[test]
fn spec_clause_calls_are_not_linted() {
    // Calls inside requires/ensures/invariant are proof constraints, not
    // runtime calls — they must not appear as callees.
    let analysis = run_clean(concat!(
        "method Helper() returns (r: int) {\n",
        "  r := 0;\n",
        "}\n",
        "method M() requires Helper() > 0 ensures Helper() > 0 {\n",
        "  while true invariant Helper() > 0 {\n",
        "    break;\n",
        "  }\n",
        "}\n",
    ));
    let callees: Vec<&str> = analysis.facts.calls.iter().map(|c| c.callee).collect();
    assert!(
        !callees.contains(&"Helper"),
        "spec clauses are skipped: {callees:?}"
    );
}

#[test]
fn returns_clause_and_type_colon_report_output_types() {
    let analysis = run_clean(concat!(
        "method M(x: int) returns (r: int) {\n",
        "  r := x;\n",
        "}\n",
        "function F(x: int): int {\n",
        "  x\n",
        "}\n",
    ));
    let m = &analysis.facts.functions[0];
    assert_eq!(m.return_type, Some("(r: int)"));
    let f = &analysis.facts.functions[1];
    assert_eq!(f.return_type, Some("int"));
}

// --- Controls: still invalid ---------------------------------------------------

#[test]
fn unclosed_block_still_errors() {
    run_error("method M() {\n");
}

#[test]
fn unterminated_paren_still_errors() {
    run_error("method M( {\n}\n");
}

#[test]
fn directive_comments_inside_contracts_still_register() {
    // `requires` subtrees are skipped for lint facts, but suppression
    // markers buried in them must still reach the comment stream.
    let analysis = run_clean(concat!(
        "method Check(n: int)\n",
        "  requires n > 0 // sloplint-allow-file: spec\n",
        "{\n",
        "  var x := 1;\n",
        "}\n",
    ));
    assert!(
        analysis
            .comments
            .iter()
            .any(|c| c.text.contains("sloplint-allow-file")),
        "directive comment inside a skipped clause must register: {:?}",
        analysis.comments
    );
}

#[test]
fn loop_index_binds_to_the_loop_scope() {
    // `for i := 0` shadows an outer `i`: its initialization and the body's
    // writes belong to the loop's own binding, not the outer local.
    let analysis = run_clean(concat!(
        "method M()\n",
        "{\n",
        "  var i := 0;\n",
        "  i := 1;\n",
        "  for i := 0 to 10\n",
        "  {\n",
        "    i := i + 1;\n",
        "  }\n",
        "  i := 5;\n",
        "}\n",
    ));
    let bindings: Vec<_> = analysis
        .facts
        .bindings
        .iter()
        .filter(|b| b.name == "i")
        .collect();
    assert_eq!(bindings.len(), 2, "expected outer and loop `i` bindings");
    assert_eq!(bindings[0].last_write_line, 9);
    assert_eq!(bindings[1].last_write_line, 7);
}

#[test]
fn tuple_var_decls_bind_their_names() {
    // `var (a2, b2) := pair` registers both names; a later write hits the
    // destructured binding, not an outer local of the same name.
    let analysis = run_clean(concat!(
        "method M()\n",
        "{\n",
        "  var (a2, b2) := (1, 2);\n",
        "  a2 := 5;\n",
        "}\n",
    ));
    let names: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    assert!(
        names.contains(&"a2") && names.contains(&"b2"),
        "destructured names must bind: {names:?}"
    );
    let a2 = analysis
        .facts
        .bindings
        .iter()
        .find(|b| b.name == "a2")
        .unwrap();
    assert_eq!(a2.last_write_line, 4);
}

#[test]
fn plain_witness_expressions_are_lintable() {
    // `witness e` is compiled — its calls must reach lint facts; only
    // `ghost witness e` is proof-only and skipped.
    let analysis = run_clean(concat!(
        "function ComputeDefault(): int\n",
        "{\n",
        "  1\n",
        "}\n",
        "type X = x: int | x > 0 witness ComputeDefault()\n",
        "type G = y: int | y > 0 ghost witness ComputeDefault()\n",
    ));
    let calls: Vec<_> = analysis.facts.calls.iter().map(|c| c.callee).collect();
    assert_eq!(
        calls.iter().filter(|c| **c == "ComputeDefault").count(),
        1,
        "only the compiled witness's call is a lint fact: {calls:?}"
    );
}

#[test]
fn datatype_updates_are_not_calls() {
    // `p.(x := 1)` is a datatype update — a `.` then `(` suffix — not a call.
    let analysis = run_clean(concat!(
        "datatype P = P(x: int)\n",
        "method M()\n",
        "{\n",
        "  var p := P(0);\n",
        "  p := p.(x := 1);\n",
        "  Helper();\n",
        "}\n",
        "method Helper() {}\n",
    ));
    let callees: Vec<_> = analysis.facts.calls.iter().map(|c| c.callee).collect();
    assert!(
        callees.contains(&"Helper"),
        "real calls still land: {callees:?}"
    );
    assert!(
        !callees.contains(&"p"),
        "datatype update is not a call: {callees:?}"
    );
}


#[test]
fn if_inside_an_else_operand_still_counts_as_nesting() {
    // `else 1 + if b then 2 else 3`: the inner if is an operand of the else
    // expression, not an else-if alternative — it must count as a level.
    let analysis = run_clean(concat!(
        "function Pick(x: int): int\n",
        "{\n",
        "  if x == 0 then 0 else 1 + if x == 1 then 2 else 3\n",
        "}\n",
    ));
    let f = &analysis.facts.functions[0];
    assert!(
        f.max_depth >= 2,
        "nested conditional inside the else operand must count: {}",
        f.max_depth
    );
}
