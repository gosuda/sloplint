use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_kotlin_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Kotlin, "case.kt", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn block_local_ends_at_block_exit() {
    let source = "fun f(c: Boolean): Int {\n    var n = 0\n    if (c) {\n        var n = 1\n        n++\n    }\n    n += 10\n    return n\n}\n";
    with_kotlin_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "inner and outer vars hold separate bindings: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 7, 7),
            "post-block write lands on the outer binding, was 0 writes when the dead inner binding absorbed it: {found:?}"
        );
        assert_eq!(
            (ns[1].write_lines, ns[1].first_write_line),
            (1, 5),
            "inner binding keeps only its own write: {found:?}"
        );
    });
}

#[test]
fn lambda_parameter_stops_writes_without_a_binding() {
    let source = "fun f(xs: List<Int>): Int {\n    var x = 0\n    xs.forEach { x -> x += 1 }\n    x += 10\n    return x\n}\n";
    with_kotlin_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "lambda parameter takes no binding of its own: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 4, 4),
            "lambda write stays on the parameter, was 2 writes when it leaked to the outer binding: {found:?}"
        );
    });
}

#[test]
fn implicit_it_takes_no_binding_and_outer_write_lands() {
    let source = "fun f(xs: List<Int>): Int {\n    var total = 0\n    xs.forEach { total += it }\n    return total\n}\n";
    with_kotlin_bindings(source, |found| {
        assert_eq!(
            named_all(found, "it").len(),
            0,
            "implicit it never becomes a binding: {found:?}"
        );
        let totals = named_all(found, "total");
        assert_eq!(totals.len(), 1, "one outer binding: {found:?}");
        assert_eq!(
            (totals[0].write_lines, totals[0].first_write_line),
            (1, 3),
            "lambda body still writes the outer total: {found:?}"
        );
        assert_eq!(
            totals[0].loop_depth, 0,
            "forEach keeps the write at loop depth 0: {found:?}"
        );
    });
}

#[test]
fn block_write_without_inner_shadow_reaches_the_outer_binding() {
    let source = "fun f(c: Boolean): Int {\n    var n = 0\n    if (c) {\n        n += 1\n    }\n    n += 10\n    return n\n}\n";
    with_kotlin_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(ns.len(), 1, "no inner declaration exists: {found:?}");
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (2, 4, 6),
            "both writes still reach the single outer binding: {found:?}"
        );
        assert_eq!(ns[0].loop_depth, 0, "no loop frames involved: {found:?}");
    });
}

#[test]
fn for_loop_variable_takes_no_binding_and_outer_write_lands() {
    let source = "fun f(xs: List<Int>): Int {\n    var x = 0\n    for (y in xs) {\n        x += y\n    }\n    x += 10\n    return x\n}\n";
    with_kotlin_bindings(source, |found| {
        assert_eq!(
            named_all(found, "y").len(),
            0,
            "for loop variable is shadow-only and takes no binding: {found:?}"
        );
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "one outer binding: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 4, 6),
            "loop body and post-loop writes both reach the outer binding: {found:?}"
        );
        assert_eq!(
            xs[0].loop_depth, 1,
            "body write runs inside one loop frame: {found:?}"
        );
    });
}

#[test]
fn when_subject_val_takes_no_binding_and_outer_write_lands() {
    let source = "fun f(v: Int): Int {\n    var total = 0\n    when (val w = v) {\n        1 -> {\n            total += 10\n        }\n        else -> {\n            total += 20\n        }\n    }\n    total += 1\n    return total\n}\n";
    with_kotlin_bindings(source, |found| {
        assert_eq!(
            named_all(found, "w").len(),
            0,
            "when subject val is shadow-only and takes no binding: {found:?}"
        );
        let totals = named_all(found, "total");
        assert_eq!(totals.len(), 1, "one outer binding: {found:?}");
        assert_eq!(
            (
                totals[0].write_lines,
                totals[0].first_write_line,
                totals[0].last_write_line
            ),
            (3, 5, 11),
            "entry and post-when writes all reach the outer binding: {found:?}"
        );
        assert_eq!(
            totals[0].loop_depth, 0,
            "when adds depth but no loop frame: {found:?}"
        );
    });
}

#[test]
fn lambda_local_ends_at_lambda_exit() {
    let source = "fun f(xs: List<Int>): Int {\n    var n = 0\n    xs.forEach {\n        var n = it\n        n += 1\n    }\n    n += 10\n    return n\n}\n";
    with_kotlin_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "lambda local stays in the closure frame as its own binding: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 7, 7),
            "post-lambda write lands on the outer binding, was 0 writes when the leaked lambda local absorbed it: {found:?}"
        );
        assert_eq!(
            (ns[1].write_lines, ns[1].first_write_line),
            (1, 5),
            "lambda local keeps only its own write: {found:?}"
        );
    });
}
