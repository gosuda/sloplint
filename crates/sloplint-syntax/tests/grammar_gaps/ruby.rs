//! Valid Ruby that the pinned tree-sitter-ruby 0.23.1 grammar rejected.
//!
//! Every repro here is valid input for `ruby -c`. The vendored grammar under
//! `vendor/tree-sitter-ruby` accepts it; the published crate reported a parse
//! error for each one. The control tests read facts from the parsed tree, so a
//! node-shape regression shows up as a changed fact and not only as a new error.
//!
//! Two gaps are fixed: a `=begin` block comment that ended too early, and a
//! method named `self` or `super` (`def self`), which the grammar read as the
//! start of `def self.name`.
//!
//! The context-dependent heredoc-in-interpolation forms (nested heredocs such
//! as `#{<<A}` inside another heredoc body) still fail: the scanner reads
//! pending heredoc bodies first-opened-first, while Ruby reads them
//! depth-first. A queue redesign was tried and regressed five upstream corpus
//! cases, so it was reverted. Those forms are recorded in
//! `vendor/tree-sitter-ruby/SLOPLINT.md`, not here.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Ruby,
        path: "gap.rb",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("ruby language loads");
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
        "valid Ruby must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

#[test]
fn a_midline_end_word_does_not_close_the_block_comment() {
    let analysis = run_clean("=begin\nthis has =end with spaces after it\n=end\nputs 1\n");
    assert_eq!(callees(&analysis), ["puts"]);
}

#[test]
fn an_indented_end_word_does_not_close_the_block_comment() {
    let analysis = run_clean("=begin\n  =end\n=end\nputs 1\n");
    assert_eq!(callees(&analysis), ["puts"]);
}

#[test]
fn an_end_word_with_a_letter_suffix_does_not_close_the_block_comment() {
    let analysis = run_clean("=begin\n=endpoint\n=ends here\n=end\nputs 1\n");
    assert_eq!(callees(&analysis), ["puts"]);
}

#[test]
fn trailing_text_on_the_end_line_stays_inside_the_comment() {
    let analysis = run_clean("=begin some title\nbody\n=end trailing text\nputs 1\n");
    assert_eq!(callees(&analysis), ["puts"]);
}

#[test]
fn an_end_word_at_end_of_input_without_a_newline_closes_the_comment() {
    let analysis = run_clean("=begin\nx\n=end");
    assert!(analysis.facts.calls.is_empty());
}

#[test]
fn a_block_comment_with_crlf_line_endings_parses() {
    let analysis = run_clean("=begin\r\nx\r\n=end\r\nputs 1\r\n");
    assert_eq!(callees(&analysis), ["puts"]);
}

#[test]
fn a_block_comment_between_methods_keeps_both_methods() {
    let source = "def a\n  1\nend\n=begin\ndoc with =end in the text\n=end\ndef b\n  2\nend\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["a", "b"]);
}

#[test]
fn a_hash_comment_before_a_method_still_counts_as_a_function() {
    let analysis = run_clean("# doc\ndef f\n  1\nend\n");
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn a_heredoc_opened_in_a_single_line_interpolation_parses() {
    let source = "def build\n  n = [1, \"#{<<NN1}\", 3,\n  nn1\nNN1\n    4]\n  n\nend\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["build"]);
}

#[test]
fn sibling_heredocs_on_one_line_still_parse_in_order() {
    let source = "def build\n  c = [<<CCC1, <<\"CCC2\", <<'CCC3']\n  c1\nCCC1\nc 2\nCCC2\nc  3\nCCC3\n  c\nend\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["build"]);
}

#[test]
fn an_unterminated_block_comment_still_reports_a_parse_error() {
    let analysis = run("=begin\nfoo\nputs 1\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing `=end` must not parse"
    );
}

#[test]
fn an_end_word_with_a_letter_suffix_at_end_of_input_still_reports_a_parse_error() {
    let analysis = run("=begin\n=endx\n");
    assert!(
        analysis.parse_error.is_some(),
        "`=endx` must not close the comment"
    );
}

#[test]
fn a_method_named_self_parses_as_a_method() {
    let analysis = run_clean("class T\n  def self\n    1\n  end\nend\n");
    assert_eq!(function_names(&analysis), ["self"]);
    let types: Vec<_> = analysis.facts.types.iter().map(|t| t.name).collect();
    assert_eq!(types, ["T"]);
}

#[test]
fn a_method_named_self_with_parameters_or_on_one_line_parses() {
    let source = concat!(
        "def self(a)\n  a\nend\n",
        "def self a, b\n  a\nend\n",
        "def self() 1 end\n",
        "def self; 1; end\n",
        "def self; end\n",
        "x = def self; 1; end\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["self"; 6]);
}

#[test]
fn an_endless_method_named_self_parses() {
    let analysis = run_clean("def self = 1\ndef self(a) = a\n");
    assert_eq!(function_names(&analysis), ["self", "self"]);
}

#[test]
fn a_setter_named_self_parses_as_one_method() {
    let analysis = run_clean("def self=(v)\n  @v = v\nend\n");
    assert_eq!(
        function_names(&analysis),
        ["self="],
        "one method, not an endless `def self` followed by loose statements"
    );
}

#[test]
fn a_method_named_super_parses() {
    let analysis = run_clean("def super\n  1\nend\ndef super(a)\n  a\nend\n");
    assert_eq!(function_names(&analysis), ["super", "super"]);
}

#[test]
fn a_method_named_self_beside_a_singleton_method_on_self_keeps_both() {
    let source = "class T\n  def self\n    1\n  end\n\n  def self.build\n    new\n  end\nend\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["self", "build"]);
}

#[test]
fn singleton_methods_named_self_and_super_keep_their_names() {
    let source = "def self.self\n  1\nend\ndef self.super\n  1\nend\ndef self::self\n  1\nend\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["self", "super", "self"]);
}

#[test]
fn alias_and_undef_with_self_still_parse() {
    let analysis = run_clean("alias self foo\nalias foo self\nundef self, super\n");
    assert!(analysis.facts.functions.is_empty());
}

#[test]
fn an_unterminated_method_named_self_still_reports_a_parse_error() {
    let analysis = run("def self\n  1\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing `end` after `def self` must not parse"
    );
}

#[test]
fn an_unterminated_singleton_method_on_self_still_reports_a_parse_error() {
    let analysis = run("def self.build\n  1\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing `end` after `def self.build` must not parse"
    );
}

#[test]
fn a_method_named_self_with_a_rescue_modifier_and_no_equals_still_reports_a_parse_error() {
    let analysis = run("def self rescue 1\n");
    assert!(
        analysis.parse_error.is_some(),
        "`def self rescue 1` is not valid Ruby: an endless method needs `=`"
    );
}

#[test]
fn a_missing_end_still_reports_a_parse_error() {
    let analysis = run("def f\n  1\n");
    let pos = analysis
        .parse_error
        .expect("a missing `end` must not parse");
    assert!(pos.line >= 1);
}
