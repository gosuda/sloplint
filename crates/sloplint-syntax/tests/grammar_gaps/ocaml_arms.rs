//! OCaml `#`-line arm-vs-directive disambiguation tests the published
//! tree-sitter-ocaml 0.26.0 grammar rejected.
//!
//! Split out of `ocaml.rs` — same contract: the vendored grammar under
//! `vendor/tree-sitter-ocaml` accepts each repro; the published crate
//! reported a parse error.

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

// `#name` arms with `when` guards are still arms — `| #error as e when
// ok e ->` must not be claimed as a cppo directive.
#[test]
fn hash_guarded_variant_arm_still_stays_arm() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error as e when ok e -> use e\n",
        "  | #stop when ready -> done_\n",
        "let g = 2\n",
    ));
}

// `'x'` char literals in a directive tail must not count toward paren
// depth — `#define RPAREN ')'` is a whole-line directive.
#[test]
fn cppo_directive_char_literal_paren_tail_parses() {
    run_clean(concat!("#define RPAREN ')'\n", "let x = 1\n"));
}

// A `'` after an identifier char is a name apostrophe, not a char
// literal: `#define ADD'(x) x` keeps its parens, and `| #error as
// e'->` keeps the `->` that ends the arm.
#[test]
fn cppo_directive_apostrophe_name_and_arm_parse() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "#define ADD'(x) x\n",
        "  | `B -> ADD'(2)\n",
        "let g = 3\n",
    ));
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error as e'-> use e'\n",
        "let g = 2\n",
    ));
}

// Arrows inside parens don't separate an arm: `#define when (fun x
// -> x)` between match arms is a directive, not a guarded arm.
#[test]
fn cppo_directive_paren_arrow_not_an_arm() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "#define when (fun x -> x)\n",
        "  | `B -> 2\n",
        "let g = 3\n",
    ));
}

// A guarded `#name` arm whose `->` sits on the next line is still an
// arm — the parenthesized lambda's arrow doesn't claim a directive.
#[test]
fn hash_multiline_guarded_variant_arm_stays_arm() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error when f (fun x -> x)\n",
        "    -> handle ()\n",
        "let g = 2\n",
    ));
}

// `#error as e -> ...` after `|` is a polymorphic-variant arm, never a
// directive — the arrow-bearing arm keeps its `->`.
#[test]
fn hash_error_variant_arm_still_stays_arm() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error as e -> pp_error e\n",
        "let g = 2\n",
    ));
}

// A bare section directive (`#endif`) inside a nested match must not
// claim the enclosing arm's `->` on the next line: conditional-section
// names skip the newline peek, so the directive stays a directive and
// the outer arm keeps its arrow.
#[test]
fn bare_directive_before_enclosing_arrow_stays_directive() {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    for name in ["#endif", "#else"] {
        let source = format!(
            concat!(
                "let f = function\n",
                "  | A when match y with\n",
                "           | B -> true\n",
                "           {}\n",
                "      -> outer\n",
                "  | C -> c\n",
                "let g = 2\n",
            ),
            name
        );
        let tree = parser.parse(&source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "{name} before the enclosing arm's arrow must parse clean, sexp: {sexp}"
        );
        assert!(
            sexp.contains("cppo_directive"),
            "{name} must lex as a cppo directive, not a method call, sexp: {sexp}"
        );
    }
}

// The mirror case: `| #error\n -> e` is a bare variant arm — `#error`
// is not a section name, so its next-line `->` still marks the arm.
#[test]
fn bare_variant_arm_with_nextline_arrow_stays_arm() {
    let source = concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error\n",
        "    -> handle ()\n",
        "let g = 2\n",
    );
    run_clean(source);
    // `#error` must stay a polymorphic-variant pattern, not a directive.
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        sexp.contains("polymorphic_variant_pattern"),
        "`#error` must parse as a variant pattern, sexp: {sexp}"
    );
    // `when`/`as` tails stay arm-shaped for EVERY name — a section name
    // used as a variant tag (`| #ifdef as e`) is still an arm.
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #ifdef as e\n",
        "    -> use e\n",
        "let g = 2\n",
    ));
}

// A literal in the arm BODY (`| #error -> "boom"`) must not count
// toward the empty-prefix arm check — the arm stays an arm.
#[test]
fn variant_arm_with_literal_body_stays_arm() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error -> \"boom\"\n",
        "let g = 2\n",
    ));
}

// A literal inside the guard (`| #error when "ok" = status -> 2`)
// must not read as a `when =` cppo condition — same-line and
// next-line arrows both stay arms.
#[test]
fn guarded_variant_arm_with_literal_guard_stays_arm() {
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error when \"ok\" = status -> 2\n",
        "let g = 2\n",
    ));
    run_clean(concat!(
        "let f = function\n",
        "  | `A -> 1\n",
        "  | #error when 'a' = x\n",
        "    -> 2\n",
        "let g = 2\n",
    ));
}

// `#define when "text"` defines a cppo macro named `when` — a lone
// literal after `when`/`as` is a directive argument, not a guard.
#[test]
fn define_macro_named_when_stays_directive() {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    for line in ["#define when \"text\"", "#define as 'c'"] {
        let source = format!(
            concat!(
                "let f = function\n",
                "  | A when match y with\n",
                "           | B -> true\n",
                "           {}\n",
                "      -> outer\n",
                "  | C -> c\n",
                "let g = 2\n",
            ),
            line
        );
        let tree = parser.parse(&source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "{line} before the enclosing arrow must parse clean, sexp: {sexp}"
        );
        assert!(
            sexp.contains("cppo_directive"),
            "{line} must lex as a cppo directive, sexp: {sexp}"
        );
    }
}

// `#if when = 1` is a cppo condition over a macro named `when`, not a
// pattern guard — it stays a directive before an enclosing arrow.
#[test]
fn cppo_condition_named_when_stays_directive() {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    let source = concat!(
        "let f = function\n",
        "  | A when match y with\n",
        "           | B -> true\n",
        "           #if when = 1\n",
        "      -> outer\n",
        "  | C -> c\n",
        "let g = 2\n",
    );
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "#if when = 1 must parse clean, sexp: {sexp}"
    );
    assert!(
        sexp.contains("cppo_directive"),
        "#if when = 1 must lex as a cppo directive, sexp: {sexp}"
    );
}

// A directive whose tail is only a quoted argument (`#include
// "cases.inc"`, `#warning "a -> b"`) is never an arm — the skipped
// string must not read as an empty arm prefix.
#[test]
fn quoted_directive_before_enclosing_arrow_stays_directive() {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    for line in ["#include \"cases.inc\"", "#warning \"a -> b\""] {
        let source = format!(
            concat!(
                "let f = function\n",
                "  | A when match y with\n",
                "           | B -> true\n",
                "           {}\n",
                "      -> outer\n",
                "  | C -> c\n",
                "let g = 2\n",
            ),
            line
        );
        let tree = parser.parse(&source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "{line} before the enclosing arrow must parse clean, sexp: {sexp}"
        );
        assert!(
            sexp.contains("cppo_directive"),
            "{line} must lex as a cppo directive, sexp: {sexp}"
        );
    }
}

#[test]
fn multiline_string_in_cppo_tail_stays_one_directive() {
    // A `->` at line-start inside an unterminated string literal is
    // string content, not an arm separator. The directive token spans
    // the string's newline and the next `|` still separates cases.
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    for source in [
        "let f = function\n  | `A -> 1\n  | #error when \"line1\n -> line2\"\n  | `C -> 3\n",
        "let f = function\n  | `A -> 1\n  | #error when \"a\nb\" = x -> 4\n  | `C -> 3\n",
        // `{|...|}`/`{delim|...|delim}` raw strings: a `"` inside is
        // content, not a string opener.
        "let f = function\n  | `A -> 1\n  | #error when s = {|a \"b|}\n  | `C -> 3\n",
        "let f = function\n  | `A -> 1\n  | #error when s = {tag|a \"b|tag}\n  | `C -> 3\n",
        "let f = function\n  | `A -> 1\n  | #endif\n  | `C -> 3\n",
    ] {
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "must parse clean, sexp: {sexp}"
        );
    }
}

#[test]
fn unclosed_raw_string_opener_in_cppo_comment_stays_hidden() {
    // `{x1|` inside a cppo comment looks like a quoted-string opener
    // (digits are legal inside a delimiter). Comment text is not code:
    // the tail scan skips `(* ... *)`, so the opener can never fire and
    // the following declarations stay visible.
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    // The comment sits inside the directive's tail (after `#error
    // when`), so the opener is inside the very region the tail scans.
    let source = "let f x =\n  match x with\n  | A -> 1\n  | #error when (* note \
                  {x1| no closer *) s = x -> e\n  | B -> 2\nlet g = 4\nlet h = 5\n";
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "must parse clean, sexp: {sexp}"
    );
    // All three bindings AND all three match arms must survive —
    // counting nodes, not just their presence.
    assert_eq!(
        sexp.matches("let_binding").count(),
        3,
        "every declaration must survive, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("match_case").count(),
        3,
        "every arm must survive, sexp: {sexp}"
    );
}

#[test]
fn cppo_directives_parse_in_interface_grammar() {
    // grammars/interface is generated separately and imports the ocaml
    // grammar — the cppo/`| directive` fold must hold for .mli too.
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml interface language loads");
    for source in [
        "val f : int -> int\n#ifdef X\nval g : int\n#endif\nval h : int\n",
        "module M : module type of struct\n  let f = function\n    | `A -> 1\n    \
         | #error\n    | `C -> 3\nend\nval g : int\n",
        "val f : int\n#define M 1\nval g : int\n",
    ] {
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "interface must parse clean, sexp: {sexp}"
        );
    }
}

#[test]
fn multiline_raw_string_in_cppo_tail_keeps_arm_arrow() {
    // A raw string inside a tail may span physical lines (and escaped
    // continuations) — the skip must reach `|}`/`|tag}` so a `->` after
    // the closer still reads as the arm's arrow.
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    for (source, match_cases, directives) in [
        // The guarded `#error` arm survives as a match_case with a
        // `when` guard — the multiline raw string can't eat its arrow.
        (
            "let f = function\n  | `A -> 1\n  | #error when s = {|a\nb|} -> e\n  | `C -> 3\n",
            3,
            0,
        ),
        // A `#define` with a multiline raw string stays a separate
        // directive node between the two real arms.
        (
            "let f = function\n  | `A -> 1\n#define M {|a \\\nb|}\n  | `C -> 3\n",
            2,
            1,
        ),
    ] {
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "must parse clean, sexp: {sexp}"
        );
        assert_eq!(
            sexp.matches("match_case").count(),
            match_cases,
            "arm structure wrong, sexp: {sexp}"
        );
        assert_eq!(
            sexp.matches("cppo_directive").count(),
            directives,
            "directive placement wrong, sexp: {sexp}"
        );
    }
}

#[test]
fn quoted_comment_markers_in_cppo_tail_comment_stay_hidden() {
    // OCaml comment lexing tracks strings: `(* "(*" *)` and `(* "*)" *)`
    // are each one complete comment — marker text inside the literal can
    // neither nest nor close early, so the arms after the directive must
    // survive intact.
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("ocaml language loads");
    for source in [
        "let f x =\n  match x with\n  | `A -> 1\n#if G (* \"(*\" *)\n  | `B -> 2\n#endif\n  | `C -> 3\n",
        "let f x =\n  match x with\n  | `A -> 1\n#if G (* \")\" *)\n  | `B -> 2\n#endif\n  | `C -> 3\n",
    ] {
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "must parse clean, sexp: {sexp}"
        );
        assert_eq!(
            sexp.matches("match_case").count(),
            3,
            "all three arms must survive, sexp: {sexp}"
        );
    }
}
