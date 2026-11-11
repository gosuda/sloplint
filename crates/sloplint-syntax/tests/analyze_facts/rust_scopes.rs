use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_rust_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Rust, "case.rs", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn nested_block_let_declares_a_fresh_binding() {
    let source = "fn f(v: &[i32]) -> i32 {\n    let mut total = 0;\n    if !v.is_empty() {\n        let mut total = 1;\n        total += 2;\n    }\n    total += 3;\n    total\n}\n";
    with_rust_bindings(source, |found| {
        let totals = named_all(found, "total");
        assert_eq!(
            totals.len(),
            2,
            "inner and outer lets hold separate bindings: {found:?}"
        );
        assert_eq!(
            (totals[0].write_lines, totals[0].first_write_line),
            (1, 7),
            "post-block write lands on the outer binding, was 0 when the dead inner binding absorbed it: {found:?}"
        );
        assert_eq!(
            totals[0].last_write_line, 7,
            "outer span ends at the post-block write: {found:?}"
        );
        assert_eq!(
            (totals[1].write_lines, totals[1].first_write_line),
            (1, 5),
            "inner binding keeps only its own write: {found:?}"
        );
    });
}

#[test]
fn if_let_mut_pattern_write_stays_local() {
    let source = "fn f(o: Option<i32>) -> i32 {\n    let mut x = 0;\n    if let Some(mut x) = o {\n        x += 1;\n        x = 0;\n    }\n    x += 2;\n    x\n}\n";
    with_rust_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the if-let pattern declares its own binding, was 1 when mut binders were shadow-only: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 7, 7),
            "only the post-if write reaches the outer binding, was 3 when both arm writes leaked out: {found:?}"
        );
        assert_eq!(
            (
                xs[1].write_lines,
                xs[1].first_write_line,
                xs[1].last_write_line
            ),
            (2, 4, 5),
            "both arm writes land on the pattern binding, was missing when mut binders were shadow-only: {found:?}"
        );
    });
}

#[test]
fn for_mut_pattern_write_stays_local() {
    let source = "fn f() -> i32 {\n    let mut x = 0;\n    for mut x in 0..3 {\n        x += 1;\n        x = 0;\n    }\n    x += 2;\n    x\n}\n";
    with_rust_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the for pattern declares its own binding, was 1 when mut binders were shadow-only: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 7, 7),
            "only the post-loop write reaches the outer binding, was 3 when both body writes leaked out: {found:?}"
        );
        assert_eq!(
            (
                xs[1].write_lines,
                xs[1].first_write_line,
                xs[1].last_write_line
            ),
            (2, 4, 5),
            "both body writes land on the pattern binding, was missing when mut binders were shadow-only: {found:?}"
        );
    });
}

#[test]
fn if_let_body_write_to_other_name_reaches_outer_binding() {
    let source = "fn f(o: Option<i32>) -> i32 {\n    let mut y = 0;\n    if let Some(mut n) = o {\n        y += 1;\n    }\n    y += 2;\n    y\n}\n";
    with_rust_bindings(source, |found| {
        let ys = named_all(found, "y");
        assert_eq!(ys.len(), 1, "no inner declaration hides y: {found:?}");
        assert_eq!(
            (
                ys[0].write_lines,
                ys[0].first_write_line,
                ys[0].last_write_line
            ),
            (2, 4, 6),
            "both writes still reach the single outer binding: {found:?}"
        );
        let ns = named_all(found, "n");
        assert_eq!(
            (ns.len(), ns[0].write_lines),
            (1, 0),
            "the untouched pattern binder still declares a binding: {found:?}"
        );
    });
}

#[test]
fn closure_tuple_pattern_registers_every_identifier() {
    let source = "fn outer() {\n    let mut a = 0;\n    let f = |(mut a, b): (i32, i32)| {\n        a += b;\n        a\n    };\n    f((1, 2));\n}\n";
    with_rust_bindings(source, |found| {
        let aas = named_all(found, "a");
        assert_eq!(aas.len(), 1, "only the outer let declares a: {found:?}");
        assert_eq!(
            aas[0].write_lines, 0,
            "closure body writes stay on the closure parameter, was 1 when only the last identifier registered: {found:?}"
        );
    });
}

#[test]
fn let_initializer_write_reaches_the_prior_binding() {
    let source =
        "fn f() -> i32 {\n    let mut x = 0;\n    let mut x = { x += 1; x * 2 };\n    x\n}\n";
    with_rust_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 2, "shadowing let keeps both bindings: {found:?}");
        assert_eq!(
            (xs[0].write_lines, xs[0].first_write_line),
            (1, 3),
            "initializer write names the prior binding, was 0 when the new binding saw its own initializer: {found:?}"
        );
        assert_eq!(
            xs[1].write_lines, 0,
            "new binding starts with no writes: {found:?}"
        );
    });
}

#[test]
fn destructuring_assignment_writes_each_target() {
    let source = "fn f(arr: [i32; 2]) {\n    let mut a = 0;\n    let mut b = 0;\n    (a, b) = (1, 2);\n    [a, b] = arr;\n}\n";
    with_rust_bindings(source, |found| {
        let aas = named_all(found, "a");
        let bs = named_all(found, "b");
        assert_eq!(aas.len(), 1, "one binding for a: {found:?}");
        assert_eq!(bs.len(), 1, "one binding for b: {found:?}");
        assert_eq!(
            (
                aas[0].write_lines,
                aas[0].first_write_line,
                aas[0].last_write_line
            ),
            (2, 4, 5),
            "tuple and array targets each write a, was 0 when destructuring roots were missed: {found:?}"
        );
        assert_eq!(
            (
                bs[0].write_lines,
                bs[0].first_write_line,
                bs[0].last_write_line
            ),
            (2, 4, 5),
            "tuple and array targets each write b, was 0 when destructuring roots were missed: {found:?}"
        );
    });
}

#[test]
fn nested_block_write_without_inner_shadow_still_reaches_the_outer_binding() {
    let source = "fn f() -> i32 {\n    let mut x = 0;\n    if true {\n        x += 1;\n    }\n    x += 2;\n    x\n}\n";
    with_rust_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "no inner declaration exists: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 4, 6),
            "both writes still reach the single outer binding: {found:?}"
        );
        assert_eq!(xs[0].loop_depth, 0, "no loop frames involved: {found:?}");
    });
}

#[test]
fn if_let_non_mut_field_writes_leave_outer_alone() {
    let source = "fn f(o: Option<i32>) -> i32 {\n    let mut n = 0;\n    if let Some(n) = o {\n        n.count = 1;\n        n.total = 2;\n    }\n    n.count = 3;\n    n.count\n}\n";
    with_rust_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "the non-mut binder stays shadow-only: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 7, 7),
            "then-body writes vanish through the shadow, was 3 when they leaked out: {found:?}"
        );
    });
}

#[test]
fn for_non_mut_field_writes_leave_outer_alone() {
    let source = "fn f(xs: &[i32]) -> i32 {\n    let mut n = 0;\n    for n in xs.iter_mut() {\n        n.x = 1;\n        n.y = 2;\n    }\n    n.x = 3;\n    n.x\n}\n";
    with_rust_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "the non-mut binder stays shadow-only: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 7, 7),
            "loop-body writes vanish through the shadow, was 3 when they leaked out: {found:?}"
        );
    });
}

#[test]
fn if_let_non_mut_else_write_reaches_outer_binding() {
    let source = "fn f(o: Option<i32>) -> i32 {\n    let mut n = 0;\n    if let Some(n) = o {\n        n.count = 1;\n    } else {\n        n = 2;\n    }\n    n.count = 3;\n    n.count\n}\n";
    with_rust_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "the non-mut binder stays shadow-only: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (2, 6, 8),
            "only the else and post-if writes reach the outer binding, was 3 when then-body writes leaked out: {found:?}"
        );
    });
}

#[test]
fn match_non_mut_index_write_leaves_outer_alone() {
    let source = "fn f(v: Option<i32>) -> i32 {\n    let mut n = 0;\n    match v {\n        Some(n) => {\n            n[0] = 1;\n        }\n        None => {}\n    }\n    n[0] = 2;\n    n[0]\n}\n";
    with_rust_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "the non-mut arm binder stays shadow-only: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 9, 9),
            "arm-body write vanishes through the shadow, was 2 when it leaked out: {found:?}"
        );
    });
}
#[test]
fn struct_field_shorthand_binds_in_both_walkers() {
    let source = "fn f(p: P, q: Q) -> i32 {\n    let mut x = 0;\n    let mut y = 0;\n    if let P { mut x, .. } = p {\n        x.a = 1;\n        x.b = 2;\n    }\n    if let Q { y, .. } = q {\n        y.a = 1;\n    }\n    x.a = 3;\n    y.a = 4;\n    x.a\n}\n";
    with_rust_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the mut shorthand pattern declares its own binding, was 1 when shorthand binders were skipped: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 11, 11),
            "only the post-if write reaches the outer binding, was 3 when both arm writes leaked out: {found:?}"
        );
        assert_eq!(
            (
                xs[1].write_lines,
                xs[1].first_write_line,
                xs[1].last_write_line
            ),
            (2, 5, 6),
            "both arm writes land on the shorthand pattern binding, was missing when shorthand binders were skipped: {found:?}"
        );
        let ys = named_all(found, "y");
        assert_eq!(
            ys.len(),
            1,
            "the non-mut shorthand binder stays shadow-only: {found:?}"
        );
        assert_eq!(
            (
                ys[0].write_lines,
                ys[0].first_write_line,
                ys[0].last_write_line
            ),
            (1, 12, 12),
            "arm-body write vanishes through the shadow, was 2 when it leaked out: {found:?}"
        );
    });
}

#[test]
fn tuple_struct_path_segments_are_not_binders() {
    let source = "fn f(s: S) -> i32 {\n    let mut shapes = 0;\n    if let shapes::Circle(mut r) = s {\n        r += 1;\n        shapes += 1;\n    }\n    shapes += 2;\n    shapes\n}\n";
    with_rust_bindings(source, |found| {
        let targets = named_all(found, "shapes");
        assert_eq!(
            targets.len(),
            1,
            "the path segment declares no shadow of its own: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (2, 5, 7),
            "arm and post-if writes both reach the outer binding, was 1 when the path segment shadowed it: {found:?}"
        );
        let rs = named_all(found, "r");
        assert_eq!(
            (rs.len(), rs[0].write_lines, rs[0].first_write_line),
            (1, 1, 4),
            "the mut payload binder still declares its own binding: {found:?}"
        );
    });
}

#[test]
fn plain_let_then_let_mut_redeclares() {
    let source = "fn f() -> i32 {\n    let mut n = 0;\n    let n = 1;\n    let mut n = 2;\n    n += 3;\n    n\n}\n";
    with_rust_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "the middle plain let shadows without declaring: {found:?}"
        );
        assert_eq!(
            (ns[0].write_lines, ns[0].first_write_line),
            (0, 2),
            "the outer binding keeps no writes: {found:?}"
        );
        assert_eq!(
            (
                ns[1].write_lines,
                ns[1].first_write_line,
                ns[1].last_write_line
            ),
            (1, 5, 5),
            "the redeclared let mut takes the later write: {found:?}"
        );
    });
}
