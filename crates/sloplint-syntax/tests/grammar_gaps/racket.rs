//! Racket shapes the pinned tree-sitter-racket 0.25.0 grammar must accept.
//!
//! The vendored grammar under `vendor/tree-sitter-racket` is the
//! published crate unchanged (no patch), so every repro below parses
//! against the published crate too. These tests pin the shapes the
//! sloplint Racket adapter reads: definitions the adapter lowers to
//! functions, comments, and quoted or commented-out datum that must
//! stay transparent.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Racket,
        path: "gap.rkt",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_racket::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("racket language loads");
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
        "valid Racket must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis
        .facts
        .functions
        .iter()
        .map(|function| function.name)
        .collect()
}

#[test]
fn lang_line_and_plain_defines_parse() {
    let analysis = run_clean("#lang racket\n(define (square x) (* x x))\n(define plain 42)\n");
    assert_eq!(function_names(&analysis), ["square"]);
}

#[test]
fn lambda_and_named_let_forms_parse() {
    let analysis = run_clean(
        "#lang racket\n(define twice (lambda (x) (+ x x)))\n(define halve (λ (x) (/ x 2)))\n(let loop ([i 0]) (loop i))\n",
    );
    // A `define` around a `lambda` reports both the named definition
    // and the anonymous closure, as with initializers elsewhere.
    assert_eq!(
        function_names(&analysis),
        ["twice", "", "halve", "", "loop"]
    );
}

#[test]
fn branches_loops_imports_and_modules_parse() {
    run_clean(
        "#lang racket\n(require racket/list)\n(provide render)\n(define (render xs) (if (null? xs) '() (for/list ([x xs]) (match x [(list a b) (+ a b)] [_ x]))))\n(module+ test (require rackunit))\n",
    );
}

#[test]
fn comments_and_quoted_datum_parse_without_functions() {
    let analysis = run_clean(
        "#lang racket\n; a line comment\n#| a block comment |#\n#;(define commented-out 1)\n'(define quoted 2)\n`(define quasiquoted ,(datum 3))\n",
    );
    assert!(
        function_names(&analysis).is_empty(),
        "quoted and commented-out defines are datum, not code: {:?}",
        function_names(&analysis)
    );
}

#[test]
fn here_strings_and_reader_forms_parse() {
    run_clean(
        "#lang racket\n(define banner #<<END\nhello\nEND\n)\n#s(point 1 2)\n#(1 2 3)\n#hash((a . 1))\n",
    );
}

#[test]
fn contract_and_keyword_params_parse() {
    let analysis = run_clean(
        "#lang racket\n(provide checked)\n(define/contract (checked [x 1] #:flag flag . rest) (->* () () any) (list x flag rest))\n",
    );
    let names = function_names(&analysis);
    assert_eq!(names, ["checked"]);
    let params = &analysis.facts.functions[0].params;
    let rendered: Vec<(&str, bool, bool, bool)> = params
        .iter()
        .map(|param| {
            (
                param.name,
                param.has_default,
                param.keyword_only,
                param.is_variadic,
            )
        })
        .collect();
    assert_eq!(
        rendered,
        [
            ("x", true, false, false),
            ("flag", false, true, false),
            ("rest", false, false, true),
        ],
        "optional, keyword and rest parameters must be classified"
    );
}

#[test]
fn provided_names_are_exported() {
    let analysis =
        run_clean("#lang racket\n(provide shown)\n(define (shown) 1)\n(define (hidden) 2)\n");
    let visibility: Vec<(&str, bool)> = analysis
        .facts
        .functions
        .iter()
        .map(|function| {
            (
                function.name,
                function.visibility == sloplint_core::facts::Visibility::Exported,
            )
        })
        .collect();
    assert_eq!(visibility, [("shown", true), ("hidden", false)]);
}

#[test]
fn unterminated_forms_still_report_a_parse_error() {
    let analysis = run("(define (broken x)\n");
    assert!(
        analysis.parse_error.is_some(),
        "an unterminated define must still be a parse error"
    );
}
