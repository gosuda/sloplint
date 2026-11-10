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
