//! Web block scopes: switch-case lets, static-block vars, for-in var heads.

use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_js<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::JavaScript, "case.js", source);
    read(&analysis.facts.bindings)
}

fn with_ts<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::TypeScript, "case.ts", source);
    read(&analysis.facts.bindings)
}

fn named<'a>(found: &'a [MutBinding<'a>], name: &str) -> &'a MutBinding<'a> {
    found
        .iter()
        .find(|binding| binding.name == name)
        .unwrap_or_else(|| panic!("no binding named {name} in {found:?}"))
}

#[test]
fn js_switch_case_let_spans_later_cases() {
    let source = "function f(mode) {\nswitch (mode) {\ncase 'a':\nlet n = 0;\nn = 1;\ncase 'b':\nn = 2;\nn = 3;\n}\n}\n";
    with_js(source, |found| {
        assert_eq!(found.len(), 1, "one switch-body binding for n: {found:?}");
        let inner = named(found, "n");
        assert_eq!(
            (
                inner.first_write_line,
                inner.write_lines,
                inner.last_write_line
            ),
            (5, 3, 8),
            "wrong-before popped the switch-case let at case exit, was (5, 1, 5): {found:?}"
        );
    });
}

#[test]
fn ts_switch_case_let_spans_later_cases() {
    let source = "function f(mode: string) {\nswitch (mode) {\ncase 'a':\nlet n = 0;\nn = 1;\ncase 'b':\nn = 2;\nn = 3;\n}\n}\n";
    with_ts(source, |found| {
        assert_eq!(found.len(), 1, "one switch-body binding for n: {found:?}");
        let inner = named(found, "n");
        assert_eq!(
            (
                inner.first_write_line,
                inner.write_lines,
                inner.last_write_line
            ),
            (5, 3, 8),
            "wrong-before popped the switch-case let at case exit, was (5, 1, 5): {found:?}"
        );
    });
}

#[test]
fn js_static_block_var_stays_in_block() {
    let source =
        "function f() {\nclass A {\nstatic {\nvar t = 0;\nt = 1;\nt = 2;\n}\n}\nt = 5;\n}\n";
    with_js(source, |found| {
        assert_eq!(found.len(), 1, "one static-block binding for t: {found:?}");
        let inner = named(found, "t");
        assert_eq!(
            (
                inner.first_write_line,
                inner.write_lines,
                inner.last_write_line
            ),
            (5, 2, 6),
            "wrong-before hoisted the static-block var to the function, was (5, 3, 9): {found:?}"
        );
    });
}

#[test]
fn ts_static_block_var_stays_in_block() {
    let source =
        "function f() {\nclass A {\nstatic {\nvar t = 0;\nt = 1;\nt = 2;\n}\n}\nt = 5;\n}\n";
    with_ts(source, |found| {
        assert_eq!(found.len(), 1, "one static-block binding for t: {found:?}");
        let inner = named(found, "t");
        assert_eq!(
            (
                inner.first_write_line,
                inner.write_lines,
                inner.last_write_line
            ),
            (5, 2, 6),
            "wrong-before hoisted the static-block var to the function, was (5, 3, 9): {found:?}"
        );
    });
}

#[test]
fn js_for_in_var_without_outer_declares_binding() {
    let source = "function f(items) {\nfor (var item of items) {\nitem = item.trim();\nitem = item.toLowerCase();\n}\n}\n";
    with_js(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "wrong-before created no binding for a for-in var, was 0 bindings: {found:?}"
        );
        let inner = named(found, "item");
        assert_eq!(
            (
                inner.first_write_line,
                inner.write_lines,
                inner.last_write_line
            ),
            (3, 2, 4),
            "the for-in var binding keeps the body writes: {found:?}"
        );
    });
}

#[test]
fn ts_for_in_var_without_outer_declares_binding() {
    let source = "function f(items: string[]) {\nfor (var item of items) {\nitem = item.trim();\nitem = item.toLowerCase();\n}\n}\n";
    with_ts(source, |found| {
        assert_eq!(
            found.len(),
            1,
            "wrong-before created no binding for a for-in var, was 0 bindings: {found:?}"
        );
        let inner = named(found, "item");
        assert_eq!(
            (
                inner.first_write_line,
                inner.write_lines,
                inner.last_write_line
            ),
            (3, 2, 4),
            "the for-in var binding keeps the body writes: {found:?}"
        );
    });
}

#[test]
fn js_write_after_switch_reaches_the_outer_let() {
    let source = "function f(mode) {\nlet n = 0;\nswitch (mode) {\ncase 'a':\nlet n = 1;\nn = 2;\n}\nn = 3;\n}\n";
    with_js(source, |found| {
        assert_eq!(
            found.len(),
            2,
            "outer and case bindings stay separate: {found:?}"
        );
        let outer = found
            .iter()
            .find(|binding| binding.pos.line == 2)
            .unwrap_or_else(|| panic!("no outer binding in {found:?}"));
        let inner = found
            .iter()
            .find(|binding| binding.pos.line == 5)
            .unwrap_or_else(|| panic!("no case binding in {found:?}"));
        assert_eq!(
            (inner.write_lines, inner.last_write_line),
            (1, 6),
            "the case-local write stays on the case binding: {found:?}"
        );
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (1, 8),
            "the write after the switch stays on the outer binding: {found:?}"
        );
    });
}
