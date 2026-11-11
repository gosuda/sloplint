//! Go block bindings for `:=`: a short declaration reuses the outer binding only
//! in the same block scope, sibling and nested blocks declare fresh bindings,
//! and a func literal parameter keeps inner writes local.

use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_go_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Go, "case.go", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn sibling_short_declaration_does_not_extend_the_outer_span() {
    let source = "package p\n\nfunc f(fi int, opts int) int {\nname := opts\nif name == 0 {\nname = fi\n}\nif fi > 0 {\nname := fi\n_ = name\n}\n_ = name\nreturn name\n}\n";
    with_go_bindings(source, |found| {
        let outer = named_all(found, "name");
        assert_eq!(
            outer.len(),
            2,
            "sibling blocks hold separate names: {found:?}"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (1, 6),
            "the nested = still reaches the outer binding: {found:?}"
        );
        assert_eq!(
            (outer[1].write_lines, outer[1].pos.line),
            (0, 9),
            "the sibling := declares a fresh binding: {found:?}"
        );
    });
}

#[test]
fn if_init_short_declaration_shadows_the_outer_name() {
    let source = "package p\n\nfunc f() bool {\nok := false\n_ = ok\nif x, ok := 1, true; ok {\n_ = x\nreturn ok\n}\nreturn ok\n}\n";
    with_go_bindings(source, |found| {
        let outer = named_all(found, "ok");
        assert_eq!(outer.len(), 2, "the if init holds its own ok: {found:?}");
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 4),
            "the if init := never touches the outer ok: {found:?}"
        );
        assert_eq!(
            outer[1].pos.line, 6,
            "the inner ok starts at the if: {found:?}"
        );
    });
}

#[test]
fn loop_body_short_declarations_stay_in_their_own_loop() {
    let source = "package p\n\nfunc f(xs []int, ys []int) int {\ntag := 0\n_ = tag\nfor _, x := range xs {\ntag := x\n_ = tag\n}\nfor _, y := range ys {\ntag := y\n_ = tag\n}\nreturn tag\n}\n";
    with_go_bindings(source, |found| {
        let tags = named_all(found, "tag");
        assert_eq!(
            tags.len(),
            3,
            "each loop body declares its own tag: {found:?}"
        );
        assert_eq!(
            (tags[0].write_lines, tags[0].last_write_line),
            (0, 4),
            "loop bodies never write the outer tag: {found:?}"
        );
        assert_eq!(
            tags[1].pos.line, 7,
            "first loop tag lives in its body: {found:?}"
        );
        assert_eq!(
            tags[2].pos.line, 11,
            "second loop tag lives in its body: {found:?}"
        );
    });
}

#[test]
fn second_loop_variable_does_not_extend_the_first_loop_span() {
    let source = "package p\n\nfunc f(n int) int {\ntotal := 0\nfor k := 0; k < n; k++ {\ntotal += k\n}\nfor k := 0; k < n; k++ {\ntotal += k\n}\nreturn total\n}\n";
    with_go_bindings(source, |found| {
        let ks = named_all(found, "k");
        assert_eq!(ks.len(), 2, "each loop declares its own k: {found:?}");
        assert_eq!(
            (ks[0].write_lines, ks[1].write_lines),
            (1, 1),
            "each k carries only its own increment: {found:?}"
        );
        assert_eq!(
            (ks[0].pos.line, ks[1].pos.line),
            (5, 8),
            "loop order: {found:?}"
        );
    });
}

#[test]
fn func_literal_parameter_keeps_inner_writes_local() {
    let source = "package p\n\nfunc f(v int) int {\nrt := v\nadd := func(rt int) int {\nrt = rt + 1\nreturn rt\n}\n_ = add(rt)\nreturn rt\n}\n";
    with_go_bindings(source, |found| {
        let rts = named_all(found, "rt");
        assert_eq!(
            rts.len(),
            1,
            "the literal parameter declares nothing new: {found:?}"
        );
        assert_eq!(
            (rts[0].write_lines, rts[0].last_write_line),
            (0, 4),
            "the assignment to the parameter never reaches the outer rt: {found:?}"
        );
    });
}

#[test]
fn closure_write_without_shadowing_still_reaches_the_outer_binding() {
    let source = "package p\n\nfunc g(v int) int {\nrt := v\nadd := func() {\nrt = v + 1\n}\nadd()\nreturn rt\n}\n";
    with_go_bindings(source, |found| {
        let rts = named_all(found, "rt");
        assert_eq!(rts.len(), 1, "no shadowing parameter here: {found:?}");
        assert_eq!(
            (rts[0].write_lines, rts[0].last_write_line),
            (1, 6),
            "the closure assignment reaches the outer rt: {found:?}"
        );
    });
}

#[test]
fn same_scope_short_redeclaration_still_writes_the_shared_name() {
    let source = "package p\n\nfunc f() (int, int, bool) {\na, err := 1, false\n_ = a\n_ = err\nb, err := 2, true\n_ = b\n_ = err\nif err {\nreturn a, b, err\n}\nreturn a, b, err\n}\n";
    with_go_bindings(source, |found| {
        assert_eq!(found.len(), 3, "a, err, then b: {found:?}");
        let errs = named_all(found, "err");
        assert_eq!(errs.len(), 1, "same scope reuses err: {found:?}");
        assert_eq!(
            (
                errs[0].write_lines,
                errs[0].first_write_line,
                errs[0].last_write_line
            ),
            (1, 7, 7),
            "the second := writes err in the same scope: {found:?}"
        );
    });
}
#[test]
fn range_variable_write_does_not_reach_the_outer_binding() {
    let source = "package p\n\nfunc f(xs []int) int {\nv := 0\n_ = v\nfor _, v := range xs {\nv = v + 1\n}\nreturn v\n}\n";
    with_go_bindings(source, |found| {
        let outer = named_all(found, "v");
        assert_eq!(
            outer.len(),
            1,
            "range v is shadow-only so only the outer v is a binding: {found:?}"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 4),
            "wrong-before recorded the loop v = on the outer v: {found:?}"
        );
    });
}
#[test]
fn loop_write_without_inner_shadow_still_reaches_the_outer_binding() {
    let source = "package p\n\nfunc f(xs []int) int {\nv := 0\n_ = v\nfor _, x := range xs {\nv = x\n}\nreturn v\n}\n";
    with_go_bindings(source, |found| {
        let outer = named_all(found, "v");
        assert_eq!(outer.len(), 1, "no inner v shadows the outer v: {found:?}");
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (1, 7),
            "the loop v = x must reach the outer v: {found:?}"
        );
    });
}
#[test]
fn short_declaration_at_top_with_param_err_creates_no_err_binding() {
    let source = "package p\n\nfunc g() (int, int) {\nreturn 1, 2\n}\nfunc f(err int) int {\nx, err := g()\n_ = x\n_ = err\nreturn x\n}\n";
    with_go_bindings(source, |found| {
        let errs = named_all(found, "err");
        assert_eq!(
            errs.len(),
            0,
            "wrong-before created a fresh err binding for x, err := with param err: {found:?}"
        );
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "x is still a new binding: {found:?}");
    });
}
#[test]
fn short_declaration_in_inner_block_with_param_err_declares_a_fresh_binding() {
    let source = "package p\n\nfunc g() (int, int) {\nreturn 1, 2\n}\nfunc f(err int) int {\nx := 1\n_ = x\nif true {\ny, err := g()\n_ = y\n_ = err\n}\nreturn x\n}\n";
    with_go_bindings(source, |found| {
        let errs = named_all(found, "err");
        assert_eq!(
            errs.len(),
            1,
            "inner blocks still declare a new err: {found:?}"
        );
        assert_eq!(
            errs[0].pos.line, 10,
            "the fresh err lives in the if body: {found:?}"
        );
    });
}
#[test]
fn starlark_tuple_with_two_thousand_mixed_targets_completes() {
    let mut left = String::new();
    for i in 0..1000 {
        if i > 0 {
            left.push_str(", ");
        }
        left.push_str(&format!("a{i}, o.b{i}"));
    }
    let source = format!("def f():\n    {left} = 0\n    return a0\n");
    let analysis = run_clean(Lang::Starlark, "case.star", &source);
    assert_eq!(
        analysis.facts.bindings.len(),
        1000,
        "only the identifiers become bindings: {:?}",
        analysis.facts.bindings.len()
    );
}

#[test]
fn calling_a_local_closure_is_not_a_write() {
    let mut source = String::from(
        "package p\n\nfunc f() int {\n\tadd := func(a, b int) int { return a + b }\n\tx := add(1, 2)\n\tx++\n",
    );
    for _ in 0..14 {
        source.push_str("\t_ = x\n");
    }
    source.push_str("\ty := add(x, 3)\n\treturn y\n}\n");
    with_go_bindings(&source, |found| {
        let adds = named_all(found, "add");
        assert_eq!(adds.len(), 1, "one add binding: {found:?}");
        assert_eq!(
            adds[0].write_lines, 0,
            "calls on lines 5 and 21 were recorded as writes to add: {found:?}"
        );
        let xs = named_all(found, "x");
        assert_eq!(xs[0].write_lines, 1, "x++ still writes x: {found:?}");
    });
}
