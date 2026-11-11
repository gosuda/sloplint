//! Write targets resolve through declaration scopes: the latest declaration of a
//! name wins inside a function, a write reaches the nearest enclosing function that
//! declares the name, and a function's names leave scope when it ends.

use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

const SHADOWED: &str = "\
function f() {
  var total = 0;
  var total = 1;
  total = 2;
}
";

const CLOSED_OVER: &str = "\
function outer() {
  var count = 0;
  function inner() {
    count += 1;
  }
}
";

const EXITED: &str = "\
function outer() {
  function inner() {
    var hidden = 0;
  }
  hidden = 1;
}
";

const FRAME_DEPTH: &str = "\
function g() {
  var outer = 0;
  for (;;) {
    var inner = 0;
    outer += 1;
    inner += 1;
  }
}
";

fn with_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::JavaScript, "case.js", source);
    read(&analysis.facts.bindings)
}

fn named<'a>(found: &'a [MutBinding<'a>], name: &str) -> &'a MutBinding<'a> {
    found
        .iter()
        .find(|binding| binding.name == name)
        .unwrap_or_else(|| panic!("no binding named {name} in {found:?}"))
}

#[test]
fn the_latest_declaration_of_a_name_receives_later_writes() {
    with_bindings(SHADOWED, |found| {
        assert_eq!(found.len(), 2, "each declaration is a binding: {found:?}");
        assert_eq!(
            (found[0].last_write_line, found[0].write_lines),
            (2, 0),
            "the shadowed declaration is never written again"
        );
        assert_eq!(
            (
                found[1].first_write_line,
                found[1].write_lines,
                found[1].last_write_line
            ),
            (4, 1, 4),
            "the write belongs to the latest declaration"
        );
    });
}

#[test]
fn a_nested_function_writes_the_binding_it_closes_over() {
    with_bindings(CLOSED_OVER, |found| {
        assert_eq!(found.len(), 1, "inner declares nothing: {found:?}");
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line
            ),
            (4, 1, 4),
            "the write inside inner reaches the binding declared in outer"
        );
    });
}

#[test]
fn names_declared_in_an_exited_function_are_out_of_scope() {
    with_bindings(EXITED, |found| {
        assert_eq!(found.len(), 1, "only inner declares a name: {found:?}");
        assert_eq!(
            (found[0].last_write_line, found[0].write_lines),
            (3, 0),
            "a write after inner ends cannot reach inner's binding"
        );
    });
}

#[test]
fn write_depth_counts_loop_frames_open_since_the_declaration() {
    with_bindings(FRAME_DEPTH, |found| {
        let outer = named(found, "outer");
        assert_eq!(
            (outer.loop_depth, outer.first_write_line, outer.write_lines),
            (1, 5, 1),
            "outer is declared before the loop and written once inside it"
        );
        assert_eq!(
            outer.last_write_line, 5,
            "the deepest write to outer is the += inside the loop"
        );
        let inner = named(found, "inner");
        assert_eq!(
            (inner.loop_depth, inner.write_lines),
            (0, 1),
            "inner is declared and written in the same loop frame"
        );
        assert_eq!(
            (inner.first_write_line, inner.last_write_line),
            (6, 6),
            "inner's write extends its mutable span"
        );
    });
}
fn with_py_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Python, "case.py", source);
    read(&analysis.facts.bindings)
}
#[test]
fn for_in_const_write_does_not_reach_the_outer_let() {
    let source = "function f(o) {\nlet k = 0;\nfor (const k in o) {\nk = 1;\n}\nreturn k;\n}\n";
    with_bindings(source, |found| {
        let outer = named(found, "k");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (0, 2),
            "wrong-before recorded the loop k = on the outer k: {found:?}"
        );
    });
}
#[test]
fn for_in_write_without_inner_shadow_still_reaches_the_outer_let() {
    let source = "function f(o) {\nlet k = 0;\nfor (const j in o) {\nk = 1;\n}\nreturn k;\n}\n";
    with_bindings(source, |found| {
        let outer = named(found, "k");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (1, 4),
            "the loop k = must reach the outer k: {found:?}"
        );
    });
}
#[test]
fn arrow_param_write_does_not_reach_the_outer_let() {
    let source = "function outer() {\nlet v = 0;\n[1].map((v) => {\nv = 1;\n});\nreturn v;\n}\n";
    with_bindings(source, |found| {
        let outer = named(found, "v");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (0, 2),
            "wrong-before recorded the arrow v = on the outer v: {found:?}"
        );
    });
}
#[test]
fn arrow_write_without_shadowing_still_reaches_the_outer_let() {
    let source = "function outer() {\nlet v = 0;\n[1].map(() => {\nv = 1;\n});\nreturn v;\n}\n";
    with_bindings(source, |found| {
        let outer = named(found, "v");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (1, 4),
            "the closure v = must reach the outer v: {found:?}"
        );
    });
}
#[test]
fn nested_def_param_augment_does_not_reach_the_outer_local() {
    let source =
        "def outer():\n    v = 0\n    def inner(v):\n        v += 1\n    inner(1)\n    return v\n";
    with_py_bindings(source, |found| {
        let outer = named(found, "v");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (0, 2),
            "wrong-before recorded the nested v += on the outer v: {found:?}"
        );
    });
}
#[test]
fn nested_augment_without_shadowing_still_reaches_the_outer_local() {
    let source =
        "def outer():\n    v = 0\n    def inner():\n        v += 1\n    inner()\n    return v\n";
    with_py_bindings(source, |found| {
        let outer = named(found, "v");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (1, 4),
            "the nested v += must reach the outer v: {found:?}"
        );
    });
}
