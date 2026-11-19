//! OCaml source that the published tree-sitter-ocaml 0.26.0 grammar rejected.
//!
//! The vendored grammar under `vendor/tree-sitter-ocaml` accepts each repro
//! below. The published crate reports a parse error for each repro. The
//! controls and the invalid literals are the same against both grammars, so
//! a change in them shows up as a regression.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Ocaml,
        path: "gap.ml",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
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
        "valid OCaml must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_error(source: &str) {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "invalid OCaml must still report a parse error:\n{source}"
    );
}

fn assert_interface_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml interface language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw interface tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

// --- camlp5 tagged quotations (hol-light) --------------------------------------

#[test]
fn tagged_quotations_parse() {
    run_clean(concat!(
        "let f x = <:expr< $x$ + 1 >>\n",
        "let g = <:patt< `A x >>\n",
        "let h = <:ctyp< int -> string >>\n",
        "let s = <:str_item< let x = 1 >>\n",
        "let m = <:module_expr< struct let a = 2 end >>\n",
        "let t = <:sig_item< val y : int >>\n",
        "let c = <:class_expr< object method m = 1 end >>\n",
    ));
}

#[test]
fn nested_quotations_parse() {
    run_clean("let nested = <:expr< f <:expr< 1 >> x >>\n");
    run_clean("let deep = <:str_item< let q = <:ctyp< 'a >> >>\n");
}

// --- backtick term quotations (hol-light `prove`) ------------------------------

#[test]
fn term_quotation_parses() {
    run_clean(concat!(
        "let tac = fun x -> x\n",
        "let p = prove (`?n. n + 0 = n`, tac)\n",
        "let q = ` { x | P x } `\n",
    ));
}

// --- polymorphic-variant tags must not be swallowed ----------------------------

#[test]
fn backtick_variants_stay_tags() {
    run_clean(concat!(
        "let f = function `A -> 1 | `B -> 2 | `C _ -> 3\n",
        "let g x = match x with `Data payload -> payload | `Empty -> 0\n",
        "type t = [`A | `B of int | `c ]\n",
    ));
}

// --- quotations in signature / type / module positions -------------------------

#[test]
fn quotations_in_all_positions_parse() {
    run_clean("type t = <:ctyp< int >>\n");
    run_clean("module M : <:module_type< sig end >> = struct end\n");
    assert_interface_clean("val f : <:ctyp< int >>\nval g : int\n");
}

// --- quotation in a let binding pattern (review-driven) ------------------------

#[test]
fn quotation_binding_pattern_parses() {
    run_clean("let <:patt< `A >> = f 1\n");
}

// --- colon and dotted tag forms (camlp5 location/module tags) ------------------

#[test]
fn colon_and_dotted_tags_parse() {
    run_clean(concat!(
        "let w = <:expr:< 1 >>\n",
        "let q = <:m.n< body >>\n",
        "let v = `1 + 1`\n",
    ));
}

// --- raw-identifier variant tags must not be swallowed -------------------------

#[test]
fn raw_ident_tags_stay_tags() {
    run_clean("let (`\\#let \\#rec) = x\n");
}

// Two raw variant arms give the lexer a real competing term_quotation span:
// if `\# ever opened a quote, `\#one -> ... `\#two would collapse into one
// quotation and the match arms would disappear.
#[test]
fn raw_ident_variant_arms_stay_tags() {
    run_clean(concat!(
        "let f v = match v with\n",
        "  | `\\#one -> 1\n",
        "  | `\\#two -> 2\n",
    ));
}

// --- Controls: still invalid ---------------------------------------------------

#[test]
fn unclosed_quotation_still_errors() {
    run_error("let f = <:expr< let x = 1\n");
}

#[test]
fn unclosed_term_quotation_still_errors() {
    run_error("let f = `?n. n + 0\n");
}

#[test]
fn unclosed_let_still_errors() {
    run_error("let f = function `A -> \n");
}

// --- cppo preprocessor directives ----------------------------------------------

#[test]
fn cppo_directives_in_structure_and_match_arms_parse() {
    run_clean(concat!(
        "#ifdef STRINGLIT\n",
        "let has_stringlit = true\n",
        "#endif\n",
        "let typeof js = match js with\n",
        "  | `Floatlit _ -> \"floatlit\"\n",
        "#ifdef STRINGLIT\n",
        "  | `Stringlit _ -> \"stringlit\"\n",
        "#endif\n",
        "let f x = x\n",
    ));
}

// --- camlp4 conditional sections ------------------------------------------------

#[test]
fn camlp4_conditional_in_let_body_parses() {
    run_clean(concat!(
        "let to_string =\n",
        "  #<If:TESTING>\n",
        "    original_name\n",
        "  #<Else>\n",
        "    function\n",
        "    | FakeSource n -> n\n",
        "    | Source n -> n\n",
        "  #<End>\n",
        "let y = 2\n",
    ));
}

// `#<If$minlevel 10>` — directive arguments without a colon.
#[test]
fn camlp4_bare_arg_directive_parses() {
    run_clean(concat!(
        "let _ =\n",
        "  #<If$minlevel 10>\n",
        "    Printf.printf \"add\\n\"\n",
        "  #<End>\n",
        "let r = 1\n",
    ));
}

// --- opa/qml bypass forms --------------------------------------------------------

#[test]
fn opa_directive_and_quotation_parse() {
    run_clean(concat!(
        "##register myfunction : string -> string\n",
        "let myfunction s = s\n",
        "#<< type data = string >>#;\n",
        "let x = 1\n",
    ));
}
