use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_php_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Php, "case.php", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn closure_param_does_not_write_outer() {
    let mut source = String::from(
        "<?php\nfunction run(array $rows): void {\n    $name = 'x';\n    $f = function ($name) { $name = strtoupper($name); return $name; };\n",
    );
    source.push_str(&"    // filler\n".repeat(21));
    source.push_str("    $name = 'y';\n}\n");
    with_php_bindings(&source, |found| {
        let names = named_all(found, "name");
        assert_eq!(
            names.len(),
            2,
            "outer and closure-local $name hold separate bindings: {found:?}"
        );
        let outer = names
            .iter()
            .find(|binding| binding.pos.line == 3)
            .expect("outer $name declared on line 3");
        assert_eq!(
            (
                outer.write_lines,
                outer.first_write_line,
                outer.last_write_line
            ),
            (1, 26, 26),
            "F3: closure parameter must not write the outer $name, was 2 writes spanning lines 4 to 26: {found:?}"
        );
        let inner = names
            .iter()
            .find(|binding| binding.pos.line == 4)
            .expect("closure-local $name declared on line 4");
        assert_eq!(
            (inner.write_lines, inner.first_write_line),
            (0, 4),
            "closure-local $name keeps only its declaration: {found:?}"
        );
    });
}

#[test]
fn closure_by_ref_use_writes_outer() {
    let source = "<?php\nfunction run(): void {\n    $y = 0;\n    $z = 0;\n    $f = function () use ($y, &$z) { $y = 1; $z = 2; };\n    $y = 3;\n    $z = 4;\n}\n";
    with_php_bindings(source, |found| {
        let ys = named_all(found, "y");
        assert_eq!(
            ys.len(),
            2,
            "by-value use($y) writes a closure-local, not the outer $y: {found:?}"
        );
        let outer_y = ys
            .iter()
            .find(|binding| binding.pos.line == 3)
            .expect("outer $y declared on line 3");
        assert_eq!(
            (
                outer_y.write_lines,
                outer_y.first_write_line,
                outer_y.last_write_line
            ),
            (1, 6, 6),
            "by-value use($y) write on line 5 must stay local, was an outer write: {found:?}"
        );
        let zs = named_all(found, "z");
        assert_eq!(
            zs.len(),
            1,
            "by-reference use(&$z) writes the single outer binding, was a split closure-local: {found:?}"
        );
        assert_eq!(
            (
                zs[0].write_lines,
                zs[0].first_write_line,
                zs[0].last_write_line
            ),
            (2, 5, 7),
            "use(&$z) writes on lines 5 and 7 reach the outer $z: {found:?}"
        );
    });
}

#[test]
fn foreach_and_catch_write_function_variable() {
    let mut source = String::from(
        "<?php\nfunction run($xs) {\n    $i = 0;\n    foreach ($xs as $i) {\n    }\n    try {\n        run($xs);\n    } catch (Exception $e) {\n        throw $e;\n    }\n",
    );
    source.push_str(&"    // filler\n".repeat(29));
    source.push_str("    $i = 5;\n    $e = 'x';\n}\n");
    with_php_bindings(&source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            1,
            "foreach reuses the function-level $i: {found:?}"
        );
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (2, 4, 40),
            "F5b: foreach writes $i on line 4, was a single write on line 40: {found:?}"
        );
        assert_eq!(
            is[0].loop_depth, 0,
            "header write lands before the loop frame opens: {found:?}"
        );
        let es = named_all(found, "e");
        assert_eq!(
            es.len(),
            1,
            "catch declares the function-level $e once: {found:?}"
        );
        assert_eq!(
            (
                es[0].write_lines,
                es[0].first_write_line,
                es[0].last_write_line
            ),
            (1, 41, 41),
            "F5b: catch declares $e on line 8 and line 41 writes it, was unrecorded: {found:?}"
        );
    });
}

#[test]
fn plain_reassignment_keeps_write() {
    let source = "<?php\nfunction run(): void {\n    $i = 0;\n    $i = 5;\n}\n";
    with_php_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(is.len(), 1, "one function-level $i: {found:?}");
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (1, 4, 4),
            "neighbour: plain reassignment still records one write: {found:?}"
        );
    });
}

#[test]
fn closure_by_ref_foreach_writes_outer() {
    let source = "<?php\nfunction run($xs): void {\n    $v = 0;\n    $f = function () use (&$v) { foreach ($xs as $v) { } };\n    $v = 1;\n}\n";
    with_php_bindings(source, |found| {
        let vs = named_all(found, "v");
        assert_eq!(
            vs.len(),
            1,
            "by-reference use(&$v) keeps a single outer $v, was a split closure-local: {found:?}"
        );
        assert_eq!(
            (
                vs[0].write_lines,
                vs[0].first_write_line,
                vs[0].last_write_line
            ),
            (2, 4, 5),
            "F3: foreach writes outer $v on line 4, was a single write on line 5: {found:?}"
        );
    });
}

#[test]
fn arrow_increment_keeps_outer_write_count() {
    let source =
        "<?php\nfunction run(): void {\n    $n = 0;\n    $f = fn($x) => $n++;\n    $n = 1;\n}\n";
    with_php_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "arrow capture by value keeps a single outer $n: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 5, 5),
            "F8: arrow increment must not write outer $n, was 2 writes spanning lines 4 to 5: {found:?}"
        );
    });
}

#[test]
fn closure_by_value_foreach_keeps_outer_untouched() {
    let source = "<?php\nfunction run($xs): void {\n    $v = 0;\n    $f = function () use ($v) { foreach ($xs as $v) { } };\n    $v = 1;\n}\n";
    with_php_bindings(source, |found| {
        let vs = named_all(found, "v");
        assert_eq!(
            vs.len(),
            2,
            "by-value use($v) keeps closure-local foreach separate from outer $v: {found:?}"
        );
        let outer = vs
            .iter()
            .find(|binding| binding.pos.line == 3)
            .expect("outer $v declared on line 3");
        assert_eq!(
            (
                outer.write_lines,
                outer.first_write_line,
                outer.last_write_line
            ),
            (1, 5, 5),
            "neighbour: by-value foreach must not write outer $v: {found:?}"
        );
    });
}
