use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_odin_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Odin, "case.odin", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn nested_short_declaration_declares_a_fresh_binding() {
    let source = "main :: proc() {\n    x := 0\n    if true {\n        x := x\n        x += 1\n    }\n    x += 2\n}\n";
    with_odin_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "nested and outer declarations hold separate bindings, was one merged binding: {found:?}"
        );
        assert_eq!(
            (xs[0].pos.line, xs[1].pos.line),
            (2, 4),
            "declaration order: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 7, 7),
            "post-block write lands on the outer binding, was lost to the dead inner binding: {found:?}"
        );
        assert_eq!(
            (
                xs[1].write_lines,
                xs[1].first_write_line,
                xs[1].last_write_line
            ),
            (1, 5, 5),
            "inner binding keeps only its own increment: {found:?}"
        );
        assert_eq!(
            (xs[0].loop_depth, xs[1].loop_depth),
            (0, 0),
            "no loop frames involved: {found:?}"
        );
    });
}

#[test]
fn sequential_c_style_for_loops_declare_separate_counters() {
    let source = "main :: proc() {\n    for i := 0; i < 3; i += 1 {\n    }\n    for i := 0; i < 3; i += 1 {\n    }\n}\n";
    with_odin_bindings(source, |found| {
        let counters = named_all(found, "i");
        assert_eq!(
            counters.len(),
            2,
            "each loop declares its own counter, was one merged binding: {found:?}"
        );
        assert_eq!(
            (counters[0].pos.line, counters[1].pos.line),
            (2, 4),
            "loop order: {found:?}"
        );
        assert_eq!(
            (
                counters[0].write_lines,
                counters[0].first_write_line,
                counters[0].last_write_line
            ),
            (1, 2, 2),
            "first counter carries only its own increment, was extended by the second loop: {found:?}"
        );
        assert_eq!(
            (
                counters[1].write_lines,
                counters[1].first_write_line,
                counters[1].last_write_line
            ),
            (1, 4, 4),
            "second counter carries only its own increment: {found:?}"
        );
    });
}

#[test]
fn nested_typed_declaration_does_not_capture_outer_writes() {
    let source = "main :: proc() {\n    x := 0\n    if true {\n        x: int = 1\n        x += 1\n    }\n    x += 2\n}\n";
    with_odin_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "typed and outer declarations hold separate bindings, was one merged binding: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 7, 7),
            "post-block write lands on the outer binding, was recorded on the dead inner binding: {found:?}"
        );
        assert_eq!(
            (
                xs[1].write_lines,
                xs[1].first_write_line,
                xs[1].last_write_line
            ),
            (1, 5, 5),
            "inner typed binding keeps only its own increment: {found:?}"
        );
    });
}

#[test]
fn call_argument_procedure_parameter_keeps_writes_local() {
    let source = "main :: proc() {\n    x := 0\n    apply(proc(x: int) {\n        x += 1\n    })\n    x += 2\n}\n";
    with_odin_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the literal parameter declares nothing new: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 6, 6),
            "the write to the literal parameter never reaches the outer x, was an extra outer write: {found:?}"
        );
        assert_eq!(xs[0].loop_depth, 0, "no loop frames involved: {found:?}");
    });
}

#[test]
fn procedure_literal_locals_stay_inside_the_literal() {
    let source = "main :: proc() {\n    run(proc() {\n        n := 0\n        n += 1\n    })\n    n := 5\n    n += 1\n}\n";
    with_odin_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "literal and outer declarations hold separate bindings, was one merged binding: {found:?}"
        );
        assert_eq!(
            (ns[0].pos.line, ns[1].pos.line),
            (3, 6),
            "declaration order: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 4, 4),
            "literal binding keeps only its own increment: {found:?}"
        );
        assert_eq!(
            (
                ns[1].write_lines,
                ns[1].first_write_line,
                ns[1].last_write_line
            ),
            (1, 7, 7),
            "outer binding keeps only its own increment: {found:?}"
        );
    });
}

#[test]
fn block_write_without_inner_declaration_still_reaches_the_outer_binding() {
    let source =
        "main :: proc() {\n    x := 0\n    if true {\n        x += 1\n    }\n    x += 2\n}\n";
    with_odin_bindings(source, |found| {
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
