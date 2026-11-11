use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_c_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::C, "case.c", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn for_initializer_scope_ends_with_the_loop() {
    let source = "void f(int n) {\n    int i = 0;\n    for (int i = 0; i < n; i++) {\n    }\n    i = 5;\n}\n";
    with_c_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            2,
            "outer and loop initializers hold separate bindings: {found:?}"
        );
        assert_eq!(
            (is[0].pos.line, is[0].write_lines, is[0].first_write_line),
            (2, 1, 5),
            "post-loop write lands on the outer binding, was 0 when the dead loop binding absorbed it: {found:?}"
        );
        assert_eq!(
            is[0].last_write_line, 5,
            "outer span ends at the post-loop write: {found:?}"
        );
        assert_eq!(
            is[0].loop_depth, 0,
            "outer write sits outside any loop frame: {found:?}"
        );
        assert_eq!(
            (
                is[1].pos.line,
                is[1].write_lines,
                is[1].first_write_line,
                is[1].last_write_line
            ),
            (3, 1, 3, 3),
            "loop binding keeps only its own increment, was 2 when the later outer write leaked into it: {found:?}"
        );
    });
}

#[test]
fn nested_block_declaration_ends_with_its_block() {
    let source = "void f(void) {\n    int x = 0;\n    {\n        int x = 1;\n        x = 2;\n    }\n    x = 3;\n}\n";
    with_c_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "outer and block declarations hold separate bindings: {found:?}"
        );
        assert_eq!(
            (xs[0].pos.line, xs[0].write_lines, xs[0].first_write_line),
            (2, 1, 7),
            "post-block write lands on the outer binding, was 0 when the dead block binding absorbed it: {found:?}"
        );
        assert_eq!(
            xs[0].last_write_line, 7,
            "outer span ends at the post-block write: {found:?}"
        );
        assert_eq!(xs[0].loop_depth, 0, "no loop frames involved: {found:?}");
        assert_eq!(
            (
                xs[1].pos.line,
                xs[1].write_lines,
                xs[1].first_write_line,
                xs[1].last_write_line
            ),
            (4, 1, 5, 5),
            "block binding keeps only its own write, was 2 when the later outer write leaked into it: {found:?}"
        );
    });
}

#[test]
fn parameter_write_records_no_binding() {
    let source = "void f(int n) {\n    n = 1;\n}\n";
    with_c_bindings(source, |found| {
        assert!(
            named_all(found, "n").is_empty(),
            "parameter writes still record no binding: {found:?}"
        );
        assert!(
            found.is_empty(),
            "no local declares n, so no binding exists: {found:?}"
        );
    });
}

#[test]
fn case_label_declaration_spans_later_cases() {
    let source = "void f(int k) {\n    switch (k) {\n    case 1:\n        int n = 0;\n        n = 1;\n        break;\n    case 2:\n        n = 2;\n        n = 3;\n        break;\n    }\n}\n";
    with_c_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(ns.len(), 1, "case declaration holds one binding: {found:?}");
        assert_eq!(
            (ns[0].pos.line, ns[0].write_lines, ns[0].first_write_line),
            (4, 3, 5),
            "later-case writes reach the case 1 binding, was 1 starting at line 5 when the case exit dropped them: {found:?}"
        );
        assert_eq!(
            ns[0].last_write_line, 9,
            "span ends at the second later-case write: {found:?}"
        );
    });
}

#[test]
fn braced_block_under_case_ends_with_its_block() {
    let source = "void f(int k) {\n    int n = 0;\n    switch (k) {\n    case 1: {\n        int n = 0;\n        n = 1;\n    }\n        break;\n    case 2:\n        n = 2;\n        break;\n    }\n}\n";
    with_c_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "block and outer declarations stay separate: {found:?}"
        );
        assert_eq!(
            (ns[0].pos.line, ns[0].write_lines, ns[0].first_write_line),
            (2, 1, 10),
            "later-case write lands on the outer binding: {found:?}"
        );
        assert_eq!(
            (ns[1].pos.line, ns[1].write_lines, ns[1].first_write_line),
            (5, 1, 6),
            "braced case block keeps only its own write: {found:?}"
        );
    });
}
