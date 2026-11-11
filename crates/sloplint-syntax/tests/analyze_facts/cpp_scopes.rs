use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_cpp_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Cpp, "case.cpp", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn for_init_declaration_ends_with_the_loop() {
    let source = "void f(int n) {\n    int i = 0;\n    for (int i = 0; i < n; i++) {\n    }\n    i = 5;\n}\n";
    with_cpp_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            2,
            "loop header declares a fresh binding: {found:?}"
        );
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (1, 5, 5),
            "post-loop write lands on the outer binding, was 0 when the dead loop binding absorbed it: {found:?}"
        );
        assert_eq!(
            is[0].loop_depth, 0,
            "outer write sits outside any loop: {found:?}"
        );
        assert_eq!(
            (
                is[1].write_lines,
                is[1].first_write_line,
                is[1].last_write_line
            ),
            (1, 3, 3),
            "loop binding keeps only its own increment: {found:?}"
        );
        assert_eq!(
            is[1].loop_depth, 0,
            "loop increment sits at its own binding frame level, same as C: {found:?}"
        );
    });
}

#[test]
fn lambda_parameter_stops_the_write() {
    let source =
        "void f() {\n    int x = 0;\n    auto g = [&](int x) {\n        x = 1;\n    };\n}\n";
    with_cpp_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "lambda parameter creates no binding: {found:?}"
        );
        assert_eq!(
            xs[0].write_lines, 0,
            "outer x is untouched, was 1 when it absorbed the line 4 parameter write: {found:?}"
        );
    });
}

#[test]
fn range_for_variable_stays_outside_outer_binding() {
    let source = "void f(std::vector<int> xs) {\n    int x = 0;\n    for (auto x : xs) {\n        x = 0;\n    }\n}\n";
    with_cpp_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "range-for variable creates no binding: {found:?}"
        );
        assert_eq!(
            xs[0].write_lines, 0,
            "outer x is untouched, was 1 when it absorbed the line 4 loop write: {found:?}"
        );
        assert_eq!(
            xs[0].loop_depth, 0,
            "outer x carries no loop depth, was 1 from the misattributed loop write: {found:?}"
        );
    });
}

#[test]
fn catch_parameter_write_is_dropped() {
    let source =
        "void f() {\n    int e = 0;\n    try {\n    } catch (int e) {\n        e = 1;\n    }\n}\n";
    with_cpp_bindings(source, |found| {
        let es = named_all(found, "e");
        assert_eq!(es.len(), 1, "catch parameter creates no binding: {found:?}");
        assert_eq!(
            es[0].write_lines, 0,
            "outer e is untouched, was 1 when it absorbed the line 5 catch write: {found:?}"
        );
    });
}

#[test]
fn structured_binding_declares_each_name() {
    let source = "struct P { int x; int y; };\nvoid f() {\n    int a = 0;\n    P p{};\n    {\n        auto [a, b] = p;\n        a = 1;\n        b = 2;\n    }\n    a = 3;\n}\n";
    with_cpp_bindings(source, |found| {
        let as_ = named_all(found, "a");
        assert_eq!(
            as_.len(),
            2,
            "structured binding declares a fresh a, was 1 when the names were dropped: {found:?}"
        );
        assert_eq!(
            (as_[1].write_lines, as_[1].first_write_line),
            (1, 7),
            "inner a keeps only its own write: {found:?}"
        );
        let bs = named_all(found, "b");
        assert_eq!(bs.len(), 1, "structured binding declares b: {found:?}");
        assert_eq!(
            (
                bs[0].write_lines,
                bs[0].first_write_line,
                bs[0].last_write_line
            ),
            (1, 8, 8),
            "b records the line 8 write: {found:?}"
        );
        assert_eq!(
            (as_[0].write_lines, as_[0].last_write_line),
            (1, 10),
            "outer a keeps only the post-block write, was 2 ending at line 7: {found:?}"
        );
    });
}

#[test]
fn capture_write_without_parameter_reaches_outer() {
    let source = "void f() {\n    int x = 0;\n    auto g = [&]() {\n        x = 1;\n    };\n}\n";
    with_cpp_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "no shadowing parameter exists: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 4, 4),
            "capture-by-reference write still reaches the outer binding: {found:?}"
        );
    });
}

#[test]
fn case_label_declaration_spans_later_cases() {
    let source = "void f(int k) {\n    switch (k) {\n    case 1:\n        int n = 0;\n        n = 1;\n        break;\n    case 2:\n        n = 2;\n        n = 3;\n        break;\n    }\n}\n";
    with_cpp_bindings(source, |found| {
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
    with_cpp_bindings(source, |found| {
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
