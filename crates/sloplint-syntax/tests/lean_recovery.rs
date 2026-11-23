//! Lean recovery preserves healthy commands and the original source coordinates.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Lean,
        path: "src/recovery.lean",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn assert_partial(analysis: &Analysis<'_>, error_line: u32) {
    assert!(analysis.partial, "the surviving commands must be analyzed");
    assert!(
        !analysis.timed_out,
        "small malformed inputs must not time out"
    );
    assert_eq!(
        analysis
            .parse_error
            .expect("the original error remains")
            .line,
        error_line,
    );
}

fn functions<'s>(analysis: &Analysis<'s>) -> Vec<(&'s str, u32)> {
    analysis
        .facts
        .functions
        .iter()
        .map(|function| (function.name, function.pos.line))
        .collect()
}

fn import_lines(analysis: &Analysis<'_>) -> Vec<u32> {
    analysis
        .facts
        .imports
        .iter()
        .map(|import| import.pos.line)
        .collect()
}

#[test]
fn hidden_missing_keeps_both_neighbors() {
    let source = "import Lean\n\
                  \n\
                  open Lean Meta\n\
                  \n\
                  def good (x : Nat) : Nat := x + 1\n\
                  \n\
                  theorem le_mid : MyNat.le = (.≤.) := rfl\n\
                  \n\
                  def after (y : Nat) : Nat := y * 2\n\
                  \n\
                  open Foo.Bar\n";
    let analysis = run(source);
    assert_partial(&analysis, 7);
    assert_eq!(functions(&analysis), [("good", 5), ("after", 9)]);
    assert_eq!(import_lines(&analysis), [1, 3, 3, 11]);
    assert!(analysis.tainted.contains(7));
    for line in [1, 3, 5, 9, 11] {
        assert!(!analysis.tainted.contains(line), "healthy line {line}");
    }
}

#[test]
fn indented_commands_recover_independently() {
    let source = "namespace Demo\n\
                  \n\
                  \x20\x20 open Lean Meta\n\
                  \n\
                  \x20\x20 theorem mid : MyNat.le = (.≤.) := rfl\n\
                  \n\
                  \x20\x20 open Foo.Bar\n\
                  \n\
                  end Demo\n";
    let analysis = run(source);
    assert_partial(&analysis, 5);
    assert_eq!(import_lines(&analysis), [3, 3, 7]);
    assert!(analysis.tainted.contains(5));
    for line in [1, 3, 7, 9] {
        assert!(!analysis.tainted.contains(line), "healthy line {line}");
    }
}

#[test]
fn braced_tactic_taints_only_its_swallowed_commands() {
    let source = "import Lean\n\
                  \n\
                  open Lean Meta\n\
                  \n\
                  def good (x : Nat) : Nat := x + 1\n\
                  \n\
                  example (h : A ∨ B) : True := by\n\
                  \x20 constructor\n\
                  \x20 {\n\
                  \x20   cases h with\n\
                  \x20   | inl a => trivial\n\
                  \x20   | inr b => trivial\n\
                  \x20 }\n\
                  \n\
                  open Foo.Bar\n";
    let analysis = run(source);
    assert_partial(&analysis, 7);
    assert_eq!(functions(&analysis), [("good", 5)]);
    assert_eq!(import_lines(&analysis), [1, 3, 3]);
    assert!(!analysis.tainted.contains(3));
    assert!(!analysis.tainted.contains(5));
    assert!(analysis.tainted.contains(7));
    assert!(analysis.tainted.contains(15));
}

const GAPLESS: &str = "import Lean\n\
                      \n\
                      open Lean Meta Std.HashSet\n\
                      \n\
                      variable [BEq α] [Hashable α]\n\
                      \n\
                      def good (x : Nat) : Nat := x\n\
                      \n\
                      def anyM [Monad m] (s : HashSet α) (f : α → m Bool) : m Bool := do\n\
                      \x20 for a in s do\n\
                      \x20   if ← f a then\n\
                      \x20     return true\n\
                      \x20 return false\n";

#[test]
fn gapless_error_root_keeps_healthy_prefix() {
    let analysis = run(GAPLESS);
    assert_partial(&analysis, 9);
    assert_eq!(functions(&analysis), [("good", 7)]);
    assert_eq!(import_lines(&analysis), [1, 3, 3, 3]);
    assert!(!analysis.tainted.contains(7));
    assert!(analysis.tainted.contains(9));
    assert!(analysis.tainted.contains(13));
}

#[test]
fn trailing_comments_do_not_erase_healthy_commands() {
    for comment in [
        "-- ordinary comment",
        "/- ordinary comment -/",
        "-- sloplint-allow-file: retained trailing directive",
        "/- sloplint-allow-file: retained trailing directive -/",
    ] {
        let source = GAPLESS.replace(
            "open Lean Meta Std.HashSet",
            &format!("open Lean Meta Std.HashSet {comment}"),
        );
        let analysis = run(&source);
        assert_partial(&analysis, 9);
        assert_eq!(import_lines(&analysis), [1, 3, 3, 3], "{comment}");
        assert_eq!(functions(&analysis), [("good", 7)], "{comment}");
        assert!(!analysis.tainted.contains(3), "{comment}");
        assert_eq!(
            analysis.comments.iter().any(|text| text.text == comment),
            comment.contains("sloplint-allow-file"),
            "only directive comments remain available for suppression processing",
        );
    }
}

#[test]
fn fragment_recovery_keeps_earlier_blanking_and_taint() {
    let source = GAPLESS.replace(
        "def anyM",
        "theorem le_mid : MyNat.le = (.≤.) := rfl\n\ndef anyM",
    );
    let analysis = run(&source);
    assert_partial(&analysis, 9);
    assert_eq!(functions(&analysis), [("good", 7)]);
    assert_eq!(import_lines(&analysis), [1, 3, 3, 3]);
    for line in [9, 11, 12, 13, 14, 15] {
        assert!(analysis.tainted.contains(line), "removed line {line}");
    }
    for line in [1, 3, 5, 7] {
        assert!(!analysis.tainted.contains(line), "healthy line {line}");
    }
}

#[test]
fn comments_from_both_recovery_stages_remain_available() {
    let source = GAPLESS
        .replace(
            "def anyM",
            "theorem le_mid : MyNat.le = (.≤.) := rfl -- sloplint-allow-file: removed theorem\n\ndef anyM",
        )
        .replace(
            " return false",
            " -- sloplint-allow-file: removed loop\n return false",
        );
    let analysis = run(&source);
    assert_partial(&analysis, 9);
    assert_eq!(functions(&analysis), [("good", 7)]);
    for text in ["removed theorem", "removed loop"] {
        assert!(
            analysis
                .comments
                .iter()
                .any(|comment| comment.text.contains(text)),
            "the original parse must retain comment {text:?}",
        );
    }
}

#[test]
fn error_root_keeps_a_healthy_suffix() {
    let source = format!("{GAPLESS}\ndef after (y : Nat) : Nat := y\n");
    let analysis = run(&source);
    assert_partial(&analysis, 9);
    assert_eq!(functions(&analysis), [("good", 7), ("after", 15)]);
    assert_eq!(import_lines(&analysis), [1, 3, 3, 3]);
    assert!(!analysis.tainted.contains(15));
}

#[test]
fn recovered_suffix_keeps_its_attributes_and_modifiers() {
    // The failed parse places a MISSING identifier in `@[simp]`; the other
    // prefixes expose no concrete error node before fragment recovery.
    for (prefix, error_line) in [
        ("@[simp]\nprivate ", 15),
        ("public meta ", 9),
        ("noncomputable ", 9),
    ] {
        let source = format!("{GAPLESS}\n{prefix}def after (y : Nat) : Nat := y\n");
        let analysis = run(&source);
        assert_partial(&analysis, error_line);
        let after = analysis
            .facts
            .functions
            .iter()
            .find(|function| function.name == "after")
            .expect("the declaration after the failed command survives");
        assert!(!analysis.tainted.contains(after.pos.line), "{prefix}");
        assert!(!analysis.tainted.contains(15), "prefix row: {prefix}");
    }
}

#[test]
fn command_text_inside_literals_and_comments_is_not_recovered() {
    for opaque in [
        "\"before\ndef ghost (x : Nat) : Nat := x\nafter\"",
        "r#\"before\ndef ghost (x : Nat) : Nat := x\nafter\"#",
        "/- before\n/- nested -/\ndef ghost (x : Nat) : Nat := x\nafter -/",
        "s!\"before {\"nested\"}\ndef ghost (x : Nat) : Nat := x\nafter\"",
        "`(command|\ndef ghost (x : Nat) : Nat := x\n)",
    ] {
        let source = format!("{GAPLESS}\n {opaque}\n\ndef after (y : Nat) : Nat := y\n");
        let analysis = run(&source);
        assert!(analysis.partial, "healthy commands survive: {opaque}");
        assert!(!analysis.timed_out, "small input must finish: {opaque}");
        assert!(analysis.parse_error.is_some(), "original error: {opaque}");
        let names: Vec<_> = functions(&analysis)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["good", "after"], "{opaque}");
    }
}

#[test]
fn commands_exposed_from_unsupported_string_arguments_are_not_code() {
    for contents in ["open Ghost\n", "def ghost (x : Nat) : Nat := x\nafter"] {
        let broken = format!("deprecated_module\n \"before\n{contents}\"\n");
        let failed = run(&broken);
        assert!(
            !failed.partial,
            "no independent command survives: {contents}"
        );
        assert!(
            failed.facts.functions.is_empty(),
            "quoted declarations: {contents}"
        );
        assert!(
            failed.facts.imports.is_empty(),
            "quoted imports: {contents}"
        );

        let source = format!("{broken}\ndef after (y : Nat) : Nat := y\n");
        let analysis = run(&source);
        assert!(
            analysis.partial,
            "the real declaration survives: {contents}"
        );
        assert_eq!(
            functions(&analysis)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            ["after"]
        );
        assert!(
            analysis.facts.imports.is_empty(),
            "quoted imports: {contents}"
        );
        assert!(
            analysis.tainted.contains(3),
            "quoted code is removed: {contents}"
        );
    }
}

#[test]
fn module_preamble_does_not_become_a_failed_fragment() {
    let source = format!("module\n{GAPLESS}");
    let analysis = run(&source);
    assert_partial(&analysis, 10);
    assert_eq!(functions(&analysis), [("good", 8)]);
    assert_eq!(import_lines(&analysis), [2, 4, 4, 4]);
    assert!(!analysis.tainted.contains(1));
}

#[test]
fn a_file_without_surviving_commands_stays_failed() {
    let source = "deprecated_module\n\
                  \x20 \"This module has been moved\"\n\
                  \x20 (since := \"2026-10-05\")\n";
    let analysis = run(source);
    assert!(analysis.parse_error.is_some());
    assert!(!analysis.partial);
    assert!(analysis.facts.functions.is_empty());
    assert!(analysis.facts.imports.is_empty());
}

#[test]
fn comments_alone_do_not_count_as_recovered_code() {
    let source = "-- sloplint-allow-file: comment without surviving commands\n\
                  theorem le_mid : MyNat.le = (.≤.) := rfl\n";
    let analysis = run(source);
    assert!(analysis.parse_error.is_some());
    assert!(!analysis.partial, "a surviving comment is not a command");
    assert!(analysis.facts.functions.is_empty());
    assert!(analysis.facts.imports.is_empty());
    assert_eq!(analysis.comments.len(), 1);
}
