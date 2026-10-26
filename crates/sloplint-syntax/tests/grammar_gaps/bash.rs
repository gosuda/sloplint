//! Valid Bash that the pinned tree-sitter-bash 0.25.1 grammar rejected.
//!
//! Every repro here is valid input for `bash -n`. The vendored grammar under
//! `vendor/tree-sitter-bash` accepts it; the published crate reported a parse
//! error for each one. The control tests read facts from the parsed tree, so a
//! node-shape regression shows up as a changed fact and not only as a new error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Bash,
        path: "gap.sh",
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("bash language loads");
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
        "valid Bash must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

#[test]
fn a_here_string_after_a_file_redirect_parses() {
    let source = "f() {\n  tee -a $c >/dev/null <<<\"x $y\"\n}\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    let tee: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "tee")
        .collect();
    assert_eq!(tee.len(), 1, "one `tee` call: {:?}", analysis.facts.calls);
    assert_eq!(
        tee[0].args.len(),
        2,
        "`-a` and `$c` are the arguments; the redirects are not"
    );
}

#[test]
fn several_redirects_ending_in_a_here_string_parse() {
    let analysis = run_clean("cat 2>&1 <<<x\ncat >f <<<x >g\ncat <<<x\n");
    assert_eq!(callees(&analysis), ["cat", "cat", "cat"]);
}

#[test]
fn the_read_write_redirect_parses() {
    let source = "f() {\n  exec 0<> \"${o}/fake-stdin\"\n  exec 3<>file\n}\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["exec", "exec"]);
}

#[test]
fn a_case_pattern_with_quoted_parentheses_and_several_chunks_parses() {
    let source = concat!(
        "f() {\n",
        "  case \"$(echo x)\" in\n",
        "    *\"Copyright (c) \"*\"Uber\"*) ;;\n",
        "    *\"a b\"*\"c d\") ;;\n",
        "  esac\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(
        callees(&analysis),
        ["echo"],
        "the substitution in the case word is still a call"
    );
}

#[test]
fn a_case_pattern_with_one_quoted_chunk_still_parses() {
    let analysis = run_clean("case x in *\"a b\"*) ;; esac\ncase x in *\"(c)\") ;; esac\n");
    assert!(
        analysis.facts.calls.is_empty(),
        "quoted case pattern records no calls: {:?}",
        analysis.facts.calls
    );
}

#[test]
fn an_assignment_with_a_redirect_parses_in_an_and_list() {
    let source = "Y=\"a\" && Y+=\" b\" 2>/dev/null\n";
    let analysis = run_clean(source);
    assert!(analysis.facts.calls.is_empty(), "no call: only assignments");
}

#[test]
fn an_assignment_with_a_redirect_still_counts_as_a_write() {
    let source = "f() {\n  Y=\"a\" &&\n  Y+=\" b\" 2>/dev/null\n}\n";
    let analysis = run_clean(source);
    let names: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    assert_eq!(names, ["Y"], "one binding, written twice");
    assert_eq!(
        analysis.facts.bindings[0].last_write_line, 3,
        "the redirected `+=` on line 3 is a write to `Y`, not a nameless command"
    );
}

#[test]
fn a_lone_assignment_before_a_redirect_and_a_command_name_both_parse() {
    // The lone assignments end each source: a newline is whitespace to the grammar, so a
    // following line would otherwise be taken as the missing command name.
    let analysis = run_clean("A=1 >f cmd\nx=1 y=2 2>/dev/null\nY=a >f\n");
    assert_eq!(
        callees(&analysis),
        ["cmd"],
        "only `A=1 >f cmd` is a command"
    );
    run_clean("Y=a <<<x\n");
}

#[test]
fn a_redirect_to_a_word_that_follows_a_bare_word_parses() {
    let analysis = run_clean("echo a<>b\nls 2>/dev/null\n");
    assert_eq!(callees(&analysis), ["echo", "ls"]);
    assert_eq!(
        analysis.facts.calls[0].args.len(),
        1,
        "`a` is the only argument; `<>b` is a redirect"
    );
}

#[test]
fn a_function_with_a_trailing_redirect_or_here_string_still_parses() {
    let analysis = run_clean("f() { :; } >f\ng() { :; } <<<x\n");
    assert_eq!(function_names(&analysis), ["f", "g"]);
}

#[test]
fn a_parameter_expansion_with_a_grouped_pattern_parses() {
    let analysis = run_clean("f() {\n  x=${y/a(b c)/z}\n  x=${y#a(b c)}\n}\n");
    assert_eq!(function_names(&analysis), ["f"]);
    let names: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    assert_eq!(
        names,
        ["x"],
        "the second assignment writes the same binding"
    );
}

#[test]
fn a_process_substitution_parses_as_a_call_argument_source() {
    let analysis = run_clean("diff <(echo a b) <(echo c)\n");
    assert_eq!(callees(&analysis), ["diff", "echo", "echo"]);
}

#[test]
fn a_regex_match_with_grouping_spanning_a_continuation_parses() {
    let analysis = run_clean("[[ $x =~ (a|b)c ]] &&\n  echo y\n");
    assert_eq!(callees(&analysis), ["echo"]);
}

#[test]
fn a_backtick_substitution_concatenated_with_words_keeps_its_shape() {
    let analysis = run_clean("echo a`b`c\necho a`b` c\n");
    assert_eq!(callees(&analysis), ["echo", "b", "echo", "b"]);
    assert_eq!(analysis.facts.calls[0].args.len(), 2);
    assert_eq!(analysis.facts.calls[2].args.len(), 2);
}

#[test]
fn a_word_concatenated_backtick_substitution_with_leading_space_stays_one_argument() {
    let analysis = run_clean("echo a` date`\n");
    assert_eq!(callees(&analysis), ["echo", "date"]);
    assert_eq!(
        analysis.facts.calls[0].args.len(),
        1,
        "the word and the substitution are one concatenated argument"
    );
}

#[test]
fn an_assignment_value_concatenated_backtick_substitution_stays_an_assignment() {
    let source = "f() {\n  v=a` b`\n}\n";
    let analysis = run_clean(source);
    let names: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    assert_eq!(names, ["v"], "the value is a concatenation, not a command");
    assert_eq!(callees(&analysis), ["b"]);
}

#[test]
fn a_word_concatenated_backtick_substitution_across_a_newline_stays_one_argument() {
    let analysis = run_clean("echo a`\nb`\n");
    assert_eq!(callees(&analysis), ["echo", "b"]);
    assert_eq!(
        analysis.facts.calls[0].args.len(),
        1,
        "the word and the substitution are one concatenated argument"
    );
}

#[test]
fn a_base_prefixed_number_with_a_variable_parses() {
    let source = "f() {\n  h=ff\n  x=$((16#$h))\n  echo 16#$h $((16#ff)) 16#${h}\n}\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    let names: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    assert_eq!(names, ["h", "x"]);
}

#[test]
fn a_bare_double_equals_word_does_not_swallow_the_next_lines() {
    let source = concat!(
        "f() {\n",
        "  echo == Loading $f\n",
        "  echo \"== Deleting range [$key, $end_key) from $f\"\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["echo", "echo"]);
}

#[test]
fn a_double_equals_test_argument_keeps_its_shape() {
    let analysis = run_clean("test $a == b* && x\n[[ $x =~ a(b c) ]]\n");
    assert_eq!(callees(&analysis), ["test", "x"]);
}

#[test]
fn an_unterminated_if_still_reports_a_parse_error() {
    let analysis = run("if true; then\n  echo x\n");
    let pos = analysis.parse_error.expect("a missing `fi` must not parse");
    assert!(pos.line >= 1, "unterminated if reports a line: {pos:?}");
}

#[test]
fn an_unterminated_function_body_with_a_redirect_still_reports_a_parse_error() {
    let analysis = run("f() {\n  tee -a $c >/dev/null <<<\"x\"\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing closing brace must not parse"
    );
}

#[test]
fn a_broken_arithmetic_expansion_with_a_base_prefix_still_reports_a_parse_error() {
    let analysis = run("x=$((16#$h\n");
    assert!(
        analysis.parse_error.is_some(),
        "an unclosed `$((` must not parse"
    );
}

#[test]
fn a_for_loop_without_an_in_list_parses_in_an_if_branch() {
    let source =
        "f() {\n  if [ -n \"$tty\" ]; then\n    for arg do\n      shift\n    done\n  fi\n}\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["shift"], "the loop body is a call");
}

#[test]
fn a_for_loop_without_an_in_list_with_do_on_the_next_line_parses() {
    let analysis = run_clean("for arg\ndo\n  echo \"$arg\"\ndone\n");
    assert_eq!(callees(&analysis), ["echo"]);
}

#[test]
fn a_parameter_alternate_with_a_closing_paren_after_an_expansion_parses() {
    let source = "f() {\n  echo \"$1${2:+ (from $2)}\"\n  return 1\n}\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["echo", "return"]);
}

#[test]
fn a_case_pattern_with_a_leading_glob_and_a_quoted_space_parses() {
    let analysis = run_clean("case \"$remotes\" in\n?*' '?*)\n  echo y\n  ;;\nesac\n");
    assert_eq!(callees(&analysis), ["echo"], "the arm body is a call");
}

#[test]
fn a_case_pattern_with_an_escaped_single_quote_parses() {
    let analysis = run_clean("case x in\n*\\'*) echo y ;;\nesac\n");
    assert_eq!(callees(&analysis), ["echo"]);
}

#[test]
fn a_heredoc_operator_right_side_after_the_body_is_not_read_from_the_body() {
    let analysis = run_clean("cat >f <<\\EOF &&\nx\nEOF\necho z\n");
    assert_eq!(
        callees(&analysis),
        ["cat", "echo"],
        "`x` is heredoc text, not a command"
    );
}

#[test]
fn a_heredoc_operator_with_a_brace_body_is_not_a_group() {
    let analysis = run_clean("cat >f <<\\EOF ||\n{\n}\nEOF\ndie x\n");
    assert_eq!(
        callees(&analysis),
        ["cat", "die"],
        "the brace line is heredoc text, not a group"
    );
}

#[test]
fn a_same_line_heredoc_operator_chain_keeps_its_right_side() {
    let analysis = run_clean("cat <<EOF && echo a && echo b\nx\nEOF\n");
    assert_eq!(callees(&analysis), ["cat", "echo", "echo"]);
}

#[test]
fn a_here_string_without_a_word_still_reports_a_parse_error() {
    let analysis = run("cat >f <<<\n");
    assert!(
        analysis.parse_error.is_some(),
        "`<<<` needs a word after it"
    );
}
