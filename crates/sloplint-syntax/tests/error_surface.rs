//! Adversarial tests for the parse-error surface of `analyze`.
//!
//! Tree-sitter recovery can leave a tree with `has_error` set while no `ERROR`
//! node is reachable through `children()` or the cursor: a MISSING node inserted
//! during recovery is hidden from both, and only `has_error` and `to_sexp` see
//! it. `analyze` must still report a position then, falling back to the root's
//! end. The tests here pin that fallback on one fixed input, the true position
//! of a visible error, and a deep nesting walk on a small stack. The `.h`
//! fallback route lives in syntax_kernel_contracts/header_fallbacks.rs.
//!
//! Each test names the defect class it stops in its doc comment.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze, spec};
use tree_sitter::Node;

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(lang: Lang, path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// The raw tree the routed grammar produces, without `analyze`'s fallbacks.
fn raw_tree(lang: Lang, source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&spec(lang).language)
        .expect("grammar loads");
    parser.parse(source, None).expect("parser returns a tree")
}

/// Whether an `ERROR` node is reachable from `node` through `children()`.
///
/// This is a recursive walk, so it shares no loop with `analyze`'s cursor walk
/// and a fault in that loop cannot hide from it. Recovery's MISSING nodes are
/// not reachable this way, so a tree whose only fault is MISSING reads as clean.
fn visible_error_in(node: Node<'_>) -> bool {
    if node.is_error() {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(visible_error_in)
}

/// Runs `f` on a thread with a 256 KiB stack, so a recursive walk over a deep
/// tree overflows here while a loop-based walk does not.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(f)
        .expect("spawn small-stack thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Defect class: a tree flagged `has_error` whose only fault is a hidden MISSING
/// node reports no parse error, so the file passes as clean with empty facts and
/// no finding. The fixed input is a Go statement glued to a label: recovery
/// inserts a MISSING terminator and no ERROR node is reachable. The premises
/// prove that hidden branch is the one reached; the assertions pin the root-end
/// fallback unconditionally.
#[test]
fn hidden_missing_only_tree_reports_root_end_not_none() {
    const SOURCE: &str = "func f() { go f()L1: }\n";
    let tree = raw_tree(Lang::Go, SOURCE);
    let root = tree.root_node();
    assert!(root.has_error(), "premise: recovery flags the tree");
    assert!(
        root.to_sexp().contains("MISSING"),
        "premise: recovery inserted a MISSING node: {}",
        root.to_sexp()
    );
    assert!(
        !visible_error_in(root),
        "premise: no ERROR node is reachable, so only the hidden MISSING carries the fault"
    );
    let analysis = run(Lang::Go, "recovered.go", SOURCE);
    let position = analysis
        .parse_error
        .expect("a has_error tree reports a parse error, never None");
    assert_eq!(
        position.byte,
        u32::try_from(SOURCE.len()).expect("small source"),
        "the fallback is the root end: the byte offset of the end of the source"
    );
    assert_eq!(
        position.line, 2,
        "the root end is on the line after the source"
    );
    assert!(
        analysis.facts.functions.is_empty(),
        "a failed parse yields no functions: {:?}",
        analysis.facts.functions
    );
    assert!(
        analysis.facts.calls.is_empty(),
        "a failed parse yields no calls: {:?}",
        analysis.facts.calls
    );
}

/// Defect class: a visible ERROR node is reported at the root's end instead of at
/// the fault, so a diagnostic points past the bad line. A stray `@` sits on line 3
/// of five; the root ends on line 6, and the reported position must be the `@`.
#[test]
fn visible_error_reports_its_own_position_not_the_root_end() {
    const SOURCE: &str = "fn ok_one() {}\nfn ok_two() {}\n@\nfn ok_three() {}\nfn ok_four() {}\n";
    let tree = raw_tree(Lang::Rust, SOURCE);
    assert!(
        visible_error_in(tree.root_node()),
        "premise: an ERROR node is reachable through children()"
    );
    let position = run(Lang::Rust, "bad.rs", SOURCE)
        .parse_error
        .expect("invalid Rust reports a parse error");
    let at = SOURCE.find('@').expect("source holds the stray @");
    assert_eq!(
        position.line, 3,
        "the stray @ is on line 3, not the root end on line 6"
    );
    assert_eq!(
        usize::try_from(position.byte).expect("byte offset fits"),
        at,
        "the byte offset points at the stray @"
    );
}

/// Defect class: analyze's walks recurse per nesting level, so a deep file
/// overflows a small stack, valid or not. `first_error_pos` and the walker's
/// `WalkState::run` both advance with cursor loops, so depth costs heap, not
/// stack. The walk runs on a 256 KiB thread; a recursive walk at this depth is
/// the regression this stops.
#[test]
fn deep_nesting_does_not_overflow_a_small_stack() {
    const DEPTH: usize = 2000;
    let parens = format!(
        "int f(void) {{ return {}1{}; }}\n",
        "(".repeat(DEPTH),
        ")".repeat(DEPTH)
    );
    let blocks = format!(
        "void g(int x) {{ {} {} }}\n",
        "if (x) {".repeat(1000),
        "}".repeat(1000)
    );
    let unbalanced = format!(
        "int f(void) {{ return {}1{}; }}\n",
        "(".repeat(DEPTH),
        ")".repeat(DEPTH - 1)
    );
    let (valid_parens, valid_blocks, bad) = on_small_stack(move || {
        let valid_parens = run(Lang::C, "parens.c", &parens).parse_error;
        let valid_blocks = run(Lang::C, "blocks.c", &blocks).parse_error;
        let bad = run(Lang::C, "unbalanced.c", &unbalanced).parse_error;
        (valid_parens, valid_blocks, bad)
    });
    assert_eq!(valid_parens, None, "2000 nested parens parse cleanly");
    assert_eq!(valid_blocks, None, "1000 nested blocks parse cleanly");
    assert!(
        bad.is_some(),
        "an unbalanced 2000-deep paren still reports a parse error"
    );
}
