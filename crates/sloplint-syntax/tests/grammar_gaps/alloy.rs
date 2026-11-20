//! Valid Alloy 6 the vendored grammar must keep parsing, observed through
//! `analyze` plus a raw-tree check in `run_clean`, because a hidden MISSING
//! recovery node sets `has_error` without producing a cursor-visible error
//! position.
//!
//! `vendor/tree-sitter-alloy` carries no grammar patch (the upstream corpus
//! passes unmodified at the pinned revision), so these are control tests:
//! they pin the facts the Alloy adapter reads, and a shape regression in a
//! re-vendor shows up as a changed fact, not only as a new error.

use std::sync::LazyLock;

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Alloy,
        path: "model.als",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_alloy::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("alloy language loads");
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
        "valid Alloy must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

#[test]
fn paragraphs_parse_clean() {
    let analysis = run_clean(
        "module tour/addressBook\n\
         \n\
         open util/ordering[State] as ord\n\
         \n\
         abstract sig Shape {}\n\
         sig Book, Library {\n\
         addr: Name -> lone Addr\n\
         }\n\
         sig Name, Addr {}\n\
         \n\
         fact noSelf { no n: Name | n in n.addr }\n\
         fact { all b: Book | some b.addr }\n\
         \n\
         pred add[b, b2: Book, n: Name, a: Addr] {\n\
         b2.addr = b.addr + n -> a\n\
         }\n\
         \n\
         fun lookup[b: Book, n: Name]: set Addr {\n\
         n.(b.addr)\n\
         }\n\
         \n\
         assert addIsFunctional {\n\
         all b, b2: Book, n: Name, a: Addr |\n\
         add[b, b2, n, a] => some b2.addr\n\
         }\n\
         \n\
         enum Color { Red, Green, Blue }\n\
         \n\
         run add for 3 but 2 Book expect 0\n\
         check addIsFunctional for 3\n",
    );
    let names: Vec<_> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        ["noSelf", "", "add", "lookup", "addIsFunctional"],
        "paragraph order and names, got {names:?}"
    );
}

#[test]
fn params_types_and_return_type_match_decls() {
    let analysis = run_clean(
        "pred add[b, b2: Book, n: Name] {\nall x: Book | b in x\n}\n\
         fun lookup[b: Book]: set Addr {\nb.addr\n}\n",
    );
    let add = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "add")
        .expect("pred add");
    let param_names: Vec<_> = add.params.iter().map(|p| p.name).collect();
    assert_eq!(param_names, ["b", "b2", "n"], "got {param_names:?}");
    assert!(
        add.params.iter().all(|p| p.ty.is_some()),
        "every Alloy decl carries a type, got {:?}",
        add.params
    );
    assert_eq!(add.return_type, None, "preds have no return type");
    let lookup = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "lookup")
        .expect("fun lookup");
    assert_eq!(lookup.return_type, Some("set Addr"));
}

#[test]
fn conditional_and_quantifier_nesting_counts() {
    let analysis = run_clean(
        "pred holds[b: Book] {\n\
         all x: Book | b in x => some x.addr else no x.addr\n\
         }\n",
    );
    let holds = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "holds")
        .expect("pred holds");
    assert!(
        holds.max_depth >= 2,
        "quantifier over implication-else nests, depth {}",
        holds.max_depth
    );
}

#[test]
fn calls_record_box_join_heads() {
    let analysis = run_clean("pred step[b: Book] {\nadd[b, b]\nsome b.addr\n}\n");
    let callees: Vec<_> = analysis.facts.calls.iter().map(|c| c.callee).collect();
    assert_eq!(
        callees,
        ["add"],
        "predicate application only, got {callees:?}"
    );
}

#[test]
fn imports_record_module_and_alias() {
    let analysis = run_clean("open util/ordering[State] as ord\npred holds {}\n");
    assert_eq!(analysis.facts.imports.len(), 1);
    let import = &analysis.facts.imports[0];
    assert_eq!(import.module, "util/ordering");
    assert_eq!(import.name, "ordering");
    assert_eq!(import.alias, Some("ord"));
}

#[test]
fn types_record_first_name_and_abstract() {
    let analysis =
        run_clean("abstract sig Shape {}\nsig Book, Library {}\nprivate sig Hidden {}\n");
    let shape = analysis
        .facts
        .types
        .iter()
        .find(|t| t.name == "Shape")
        .expect("sig Shape");
    assert!(shape.abstraction, "abstract qualifier marks the type");
    assert_eq!(shape.visibility, Visibility::Exported);
    let book = analysis
        .facts
        .types
        .iter()
        .find(|t| t.name == "Book")
        .expect("first name of a multi-name sig");
    assert_eq!(book.visibility, Visibility::Exported);
    let hidden = analysis
        .facts
        .types
        .iter()
        .find(|t| t.name == "Hidden")
        .expect("private sig");
    assert_eq!(hidden.visibility, Visibility::Private);
}

// Round 2: real-model shapes the vendored grammar rejected — primed
// declaration names, `Sig::name` scoping, keyword-led qualified paths used
// as call heads and infix operators, and string-named asserts.

#[test]
fn primed_decl_names_parse_clean() {
    run_clean(
        "assert add_1 { all x: Int | some x }\n\
         pred step'[s: State] { some s }\n\
         fun lookup'[b: Book]: set Addr { b.addr }\n",
    );
}

#[test]
fn sig_scoped_names_parse_clean() {
    let analysis = run_clean(
        "sig GrandParent {}\n\
         pred root { no GrandParent::parents }\n\
         fun kidsOf[]: set GrandParent::children { GrandParent::children }\n",
    );
    let names: Vec<_> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(names.contains(&"root"), "pred name kept, got {names:?}");
}

#[test]
fn qualified_paths_as_calls_and_infix_parse_clean() {
    let analysis = run_clean(
        "open util/ordering[State]\n\
         pred ordered { first[] = pred/totalOrder[] or this/first = this/last }\n\
         fun total[a, b: Int]: Int { a fun/add b }\n",
    );
    let total = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "total")
        .expect("fun total");
    assert_eq!(
        total.return_type,
        Some("Int"),
        "got {:?}",
        total.return_type
    );
}

#[test]
fn string_named_assert_parses() {
    run_clean("assert \"every book has an address\" { all b: Book | some b.addr }\n");
}

#[test]
fn private_and_anonymous_paragraphs_are_not_exported() {
    let analysis =
        run_clean("private pred helper[a: Int] {\nsome a\n}\nfact { all b: Book | some b }\n");
    let helper = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "helper")
        .expect("private pred");
    assert_eq!(helper.visibility, Visibility::Private);
    let anonymous = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name.is_empty())
        .expect("anonymous fact");
    assert_eq!(
        anonymous.visibility,
        Visibility::Private,
        "an unnamed paragraph cannot be searched by name"
    );
}

#[test]
fn primed_command_targets_parse_clean() {
    run_clean("pred step' { 1 = 1 }\ncheck step'\nrun step'\n");
}

#[test]
fn path_call_after_constraint_parses_clean() {
    run_clean("sig A {}\nfact { a = a\npred/totalOrder[a] }\n");
}

#[test]
fn primed_paragraph_name_keeps_primes() {
    let analysis = run_clean("pred step' { 1 = 1 }\nassert A' { 1 = 1 }\n");
    for want in ["step'", "A'"] {
        assert!(
            analysis.facts.functions.iter().any(|f| f.name == want),
            "missing paragraph name {want}: {:?}",
            analysis
                .facts
                .functions
                .iter()
                .map(|f| &f.name)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn infix_qual_call_records_call_fact() {
    let analysis =
        run_clean("fun total[a, b: Int]: Int { a fun/add b }\nfun add[x, y: Int]: Int { x + y }\n");
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee == "fun/add"),
        "missing infix call fact: {:?}",
        analysis
            .facts
            .calls
            .iter()
            .map(|c| &c.callee)
            .collect::<Vec<_>>()
    );
}
