//! Condition binders and closure frames resolve writes to the binding they target:
//! a `var` bound by `if`, `for`, `case` or `guard` owns the writes in its body,
//! and a closure body owns its own locals.

use sloplint_core::facts::{AmbientKind, MutBinding};
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_swift_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Swift, "case.swift", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

fn in_function_writes(source: &str) -> usize {
    run_clean(Lang::Swift, "case.swift", source)
        .facts
        .ambient
        .iter()
        .filter(|site| site.kind == AmbientKind::GlobalWrite && site.in_function.is_some())
        .count()
}

const CONDITION_BINDERS: &str = "\
func f(o: Int?, xs: [Int], v: E) {
    var x = 0
    if var x = o {
        x = 1
    }
    for var y in xs {
        y = 2
    }
    switch v {
    case .a(var z):
        z = 3
    default:
        break
    }
}
";

#[test]
fn if_for_and_case_var_binders_shadow_the_outer_var() {
    with_swift_bindings(CONDITION_BINDERS, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the write on line 4 binds the if binder, it was recorded against the outer var x: {found:?}"
        );
        assert_eq!(
            (xs[0].pos.line, xs[0].write_lines, xs[0].last_write_line),
            (2, 0, 2),
            "the outer var x keeps no writes: {found:?}"
        );
        assert_eq!(
            (
                xs[1].pos.line,
                xs[1].first_write_line,
                xs[1].last_write_line,
                xs[1].write_lines,
                xs[1].loop_depth
            ),
            (3, 4, 4, 1, 0),
            "the if binder owns the write on line 4: {found:?}"
        );
        let ys = named_all(found, "y");
        assert_eq!(
            ys.len(),
            1,
            "the write on line 7 binds the for binder, it was dropped with no binding: {found:?}"
        );
        assert_eq!(
            (
                ys[0].pos.line,
                ys[0].first_write_line,
                ys[0].last_write_line,
                ys[0].write_lines,
                ys[0].loop_depth
            ),
            (6, 7, 7, 1, 0),
            "the for binder owns the write on line 7: {found:?}"
        );
        let zs = named_all(found, "z");
        assert_eq!(
            zs.len(),
            1,
            "the write on line 11 binds the case binder, it was dropped with no binding: {found:?}"
        );
        assert_eq!(
            (
                zs[0].pos.line,
                zs[0].first_write_line,
                zs[0].last_write_line,
                zs[0].write_lines,
                zs[0].loop_depth
            ),
            (10, 11, 11, 1, 0),
            "the case binder owns the write on line 11: {found:?}"
        );
    });
}

const CLOSURE_FRAME: &str = "\
func f() {
    var x = 0
    let g = {
        var x = 1
        x += 1
    }
    x = 5
    let h = { x in x = 6 }
}
";

#[test]
fn closure_body_var_stays_inside_its_frame() {
    let analysis = run_clean(Lang::Swift, "case.swift", CLOSURE_FRAME);
    let found = &analysis.facts.bindings;
    assert_eq!(
        analysis.facts.functions.len(),
        3,
        "each closure gets its own frame: {found:?}"
    );
    let xs = named_all(found, "x");
    assert_eq!(
        xs.len(),
        2,
        "the write on line 7 targets the outer x, it was recorded against the closure's var x: {found:?}"
    );
    assert_eq!(
        (
            xs[0].pos.line,
            xs[0].first_write_line,
            xs[0].last_write_line,
            xs[0].write_lines
        ),
        (2, 7, 7, 1),
        "the outer x owns only the write on line 7: {found:?}"
    );
    assert_eq!(
        (
            xs[1].pos.line,
            xs[1].first_write_line,
            xs[1].last_write_line,
            xs[1].write_lines,
            xs[1].loop_depth
        ),
        (4, 5, 5, 1, 0),
        "the closure var owns only the write on line 5: {found:?}"
    );
}

const GUARD_BINDER: &str = "\
func outer(o: Int?) {
    var x = 0
    func inner() {
        guard var x = o else {
            return
        }
        x = 1
    }
}
";

#[test]
fn guard_var_binds_the_rest_of_the_block() {
    with_swift_bindings(GUARD_BINDER, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the write on line 7 binds the guard binder, it was recorded against the outer var x: {found:?}"
        );
        assert_eq!(
            (xs[0].pos.line, xs[0].write_lines, xs[0].last_write_line),
            (2, 0, 2),
            "the outer var x keeps no writes: {found:?}"
        );
        assert_eq!(
            (
                xs[1].pos.line,
                xs[1].first_write_line,
                xs[1].last_write_line,
                xs[1].write_lines
            ),
            (4, 7, 7, 1),
            "the guard binder owns the write on line 7: {found:?}"
        );
    });
}

const GUARD_SHADOWS_GLOBAL: &str = "\
var x = 0
func f(o: Int?) {
    guard var x = o else {
        return
    }
    x = 1
}
";

#[test]
fn guard_var_write_emits_no_global_write() {
    assert_eq!(
        in_function_writes(GUARD_SHADOWS_GLOBAL),
        0,
        "the write on line 6 targets the guard binder, it emitted a GlobalWrite for x"
    );
    with_swift_bindings(GUARD_SHADOWS_GLOBAL, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the guard binder is the only local binding: {found:?}"
        );
        assert_eq!(
            (xs[0].pos.line, xs[0].last_write_line, xs[0].write_lines),
            (3, 6, 1),
            "the guard binder owns the write on line 6: {found:?}"
        );
    });
}

const PLAIN_SPAN: &str = "\
func f() {
    var x = 0
    x = 1
}
";

#[test]
fn plain_declaration_span_is_unchanged() {
    with_swift_bindings(PLAIN_SPAN, |found| {
        assert_eq!(found.len(), 1, "one declaration is one binding: {found:?}");
        assert_eq!(
            (
                found[0].pos.line,
                found[0].first_write_line,
                found[0].last_write_line,
                found[0].write_lines,
                found[0].loop_depth
            ),
            (2, 3, 3, 1, 0),
            "the write still extends the declared span: {found:?}"
        );
    });
}

const IF_LET_ELSE_BODY: &str = "\
func f(o: Int?) {
    var x = 0
    if let x = o {
        print(x)
    } else {
        x = 1
    }
    x = 2
}";

#[test]
fn if_let_else_write_reaches_the_outer_var() {
    with_swift_bindings(IF_LET_ELSE_BODY, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the else write targets the outer var, it was recorded against a second binding: {found:?}"
        );
        assert_eq!(
            (
                xs[0].pos.line,
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 2, 6, 8),
            "the else write on line 6 reaches the outer var, it was dropped with write_lines 1: {found:?}"
        );
    });
}

const IF_LET_BODY_WRITE: &str = "\
func f(o: Int?) {
    var x = 0
    if let x = o {
        x = 1
    }
    x = 2
}";

#[test]
fn if_let_body_write_to_the_binder_name_stays_dropped() {
    with_swift_bindings(IF_LET_BODY_WRITE, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the body write targets the let binder, it leaked onto a second binding: {found:?}"
        );
        assert_eq!(
            (
                xs[0].pos.line,
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 1, 6, 6),
            "the body write on line 4 stays dropped, only the write on line 6 reaches the outer var: {found:?}"
        );
    });
}

const WHILE_VAR_BINDER: &str = "\
func f(o: Int?) {
    var x = 0
    while var x = o {
        x += 1
    }
    x += 10
}";

#[test]
fn while_var_binder_shadows_the_outer_var() {
    with_swift_bindings(WHILE_VAR_BINDER, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the loop binds its own x, it was never registered and both writes hit the outer var: {found:?}"
        );
        assert_eq!(
            (
                xs[0].pos.line,
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 1, 6, 6),
            "the outer var keeps only the write on line 6, it had 2 when the loop write leaked out: {found:?}"
        );
        assert_eq!(
            (
                xs[1].pos.line,
                xs[1].first_write_line,
                xs[1].last_write_line,
                xs[1].write_lines,
                xs[1].loop_depth
            ),
            (3, 4, 4, 1, 0),
            "the loop binder owns the write on line 4: {found:?}"
        );
    });
}

const WHILE_LET_BODY: &str = "\
func f(o: Int?) {
    var x = 0
    while let x = o {
        x += 1
    }
    x += 10
}";

#[test]
fn while_let_body_write_stays_dropped() {
    with_swift_bindings(WHILE_LET_BODY, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the loop binds no outer-visible name: {found:?}"
        );
        assert_eq!(
            (
                xs[0].pos.line,
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 1, 6, 6),
            "only the post-loop write reaches the outer binding, was 2 when the body write leaked out: {found:?}"
        );
    });
}
