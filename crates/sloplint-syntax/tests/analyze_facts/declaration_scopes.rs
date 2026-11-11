//! Write targets resolve through declaration scopes: the latest declaration of a
//! name wins inside a function, a write reaches the nearest enclosing function that
//! declares the name, and a function's names leave scope when it ends.

use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

const REDECLARED: &str = "\
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
fn a_redeclared_var_writes_the_single_function_binding() {
    with_bindings(REDECLARED, |found| {
        assert_eq!(
            found.len(),
            1,
            "one function-level binding for var total: {found:?}"
        );
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line
            ),
            (3, 2, 4),
            "wrong-before kept one binding per declaration and lost the later write: {found:?}"
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

fn with_ts_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::TypeScript, "case.ts", source);
    read(&analysis.facts.bindings)
}

#[test]
fn block_let_in_inner_block_does_not_capture_later_outer_write() {
    let source = "function f(c) {\nlet t = 0;\nif (c) {\nlet t = 1;\n}\nt = 2;\n}\n";
    with_bindings(source, |found| {
        assert_eq!(
            found.len(),
            2,
            "outer and inner let are separate bindings: {found:?}"
        );
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line
            ),
            (6, 1, 6),
            "wrong-before recorded the outer t = on the dead inner binding: {found:?}"
        );
        assert_eq!(
            (found[1].write_lines, found[1].last_write_line),
            (0, 4),
            "the inner t is never written after its block ends: {found:?}"
        );
    });
}

#[test]
fn ts_block_let_in_inner_block_does_not_capture_later_outer_write() {
    let source = "function f(c: boolean) {\nlet t = 0;\nif (c) {\nlet t = 1;\n}\nt = 2;\n}\n";
    with_ts_bindings(source, |found| {
        assert_eq!(
            found.len(),
            2,
            "outer and inner let are separate bindings: {found:?}"
        );
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line
            ),
            (6, 1, 6),
            "wrong-before recorded the outer t = on the dead inner binding: {found:?}"
        );
        assert_eq!(
            (found[1].write_lines, found[1].last_write_line),
            (0, 4),
            "the inner t is never written after its block ends: {found:?}"
        );
    });
}

#[test]
fn second_var_in_shared_function_writes_the_one_binding() {
    let source = "function f(xs, ys) {\nfor (var i = 0; i < xs.length; i++) { }\nfor (var i = 0; i < ys.length; i++) { }\n}\n";
    with_bindings(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "one function-level binding for var i: {found:?}"
        );
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line,
                found[0].loop_depth
            ),
            (2, 2, 3, 0),
            "wrong-before kept one binding per loop and stopped counting early: {found:?}"
        );
    });
}

#[test]
fn ts_second_var_in_shared_function_writes_the_one_binding() {
    let source = "function f(xs, ys) {\nfor (var i = 0; i < xs.length; i++) { }\nfor (var i = 0; i < ys.length; i++) { }\n}\n";
    with_ts_bindings(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "one function-level binding for var i: {found:?}"
        );
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line,
                found[0].loop_depth
            ),
            (2, 2, 3, 0),
            "wrong-before kept one binding per loop and stopped counting early: {found:?}"
        );
    });
}

#[test]
fn var_loop_head_write_reaches_the_function_level_var() {
    let source = "function f(xs) {\nvar k = 0;\nfor (var k of xs) {\nk = k + 1;\n}\n}\n";
    with_bindings(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "the loop head declares no new binding: {found:?}"
        );
        let outer = named(found, "k");
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line,
                outer.loop_depth
            ),
            (3, 2, 4, 1),
            "for-in var head writes once on the existing function binding: {found:?}"
        );
    });
}

#[test]
fn ts_var_loop_head_write_reaches_the_function_level_var() {
    let source = "function f(xs: number[]) {\nvar k = 0;\nfor (var k of xs) {\nk = k + 1;\n}\n}\n";
    with_ts_bindings(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "the loop head declares no new binding: {found:?}"
        );
        let outer = named(found, "k");
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line,
                outer.loop_depth
            ),
            (3, 2, 4, 1),
            "for-in var head writes once on the existing function binding: {found:?}"
        );
    });
}

#[test]
fn catch_param_write_does_not_reach_the_outer_let() {
    let source = "function f() {\nlet e = 0;\ntry {\ng();\n} catch (e) {\ne = null;\n}\n}\n";
    with_bindings(source, |found| {
        let outer = named(found, "e");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (0, 2),
            "wrong-before recorded the catch e = on the outer e: {found:?}"
        );
    });
}

#[test]
fn ts_catch_param_write_does_not_reach_the_outer_let() {
    let source = "function f() {\nlet e = 0;\ntry {\ng();\n} catch (e) {\ne = null;\n}\n}\n";
    with_ts_bindings(source, |found| {
        let outer = named(found, "e");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (0, 2),
            "wrong-before recorded the catch e = on the outer e: {found:?}"
        );
    });
}

#[test]
fn ts_destructuring_assignment_writes_each_target() {
    let source =
        "function swap(o) {\nlet a = 1;\nlet b = 2;\n[a, b] = [b, a];\n({ a, b } = o);\n}\n";
    with_ts_bindings(source, |found| {
        assert_eq!(found.len(), 2, "a and b are the only bindings: {found:?}");
        let a = named(found, "a");
        let b = named(found, "b");
        assert_eq!(
            (a.first_write_line, a.write_lines, a.last_write_line),
            (4, 2, 5),
            "wrong-before recorded no write for the destructured targets: {found:?}"
        );
        assert_eq!(
            (b.first_write_line, b.write_lines, b.last_write_line),
            (4, 2, 5),
            "wrong-before recorded no write for the destructured targets: {found:?}"
        );
    });
}

#[test]
fn js_destructuring_assignment_writes_each_target() {
    let source =
        "function swap(o) {\nlet a = 1;\nlet b = 2;\n[a, b] = [b, a];\n({ a, b } = o);\n}\n";
    with_bindings(source, |found| {
        assert_eq!(found.len(), 2, "a and b are the only bindings: {found:?}");
        let a = named(found, "a");
        let b = named(found, "b");
        assert_eq!(
            (a.first_write_line, a.write_lines, a.last_write_line),
            (4, 2, 5),
            "the JavaScript destructuring write roots must keep working: {found:?}"
        );
        assert_eq!(
            (b.first_write_line, b.write_lines, b.last_write_line),
            (4, 2, 5),
            "the JavaScript destructuring write roots must keep working: {found:?}"
        );
    });
}

#[test]
fn catch_body_write_to_other_name_still_reaches_the_outer_let() {
    let source = "function f() {\nlet x = 0;\ntry {\ng();\n} catch (e) {\nx = 1;\n}\n}\n";
    with_bindings(source, |found| {
        assert_eq!(found.len(), 1, "only the outer let is a binding: {found:?}");
        let outer = named(found, "x");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (1, 6),
            "the catch body x = must reach the outer x: {found:?}"
        );
    });
}

#[test]
fn distinct_vars_in_one_function_stay_distinct() {
    let source = "function f() {\nvar a = 0;\nvar b = 1;\na = 2;\nb = 3;\n}\n";
    with_bindings(source, |found| {
        assert_eq!(
            found.len(),
            2,
            "distinct vars keep distinct bindings: {found:?}"
        );
        let a = named(found, "a");
        let b = named(found, "b");
        assert_eq!(
            (a.first_write_line, a.write_lines, a.last_write_line),
            (4, 1, 4),
            "the a = write belongs to a: {found:?}"
        );
        assert_eq!(
            (b.first_write_line, b.write_lines, b.last_write_line),
            (5, 1, 5),
            "the b = write belongs to b: {found:?}"
        );
    });
}

#[test]
fn nested_block_var_write_reaches_the_function_binding() {
    let source = "function f() {\nvar x = 0;\n{\nvar x = 1;\n}\nx = 2;\n}\n";
    with_bindings(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "var hoists to one function binding: {found:?}"
        );
        assert_eq!(
            (
                found[0].first_write_line,
                found[0].write_lines,
                found[0].last_write_line
            ),
            (4, 2, 6),
            "the nested block var = must reuse the function binding: {found:?}"
        );
    });
}
