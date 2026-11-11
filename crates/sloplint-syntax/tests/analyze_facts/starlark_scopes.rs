use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_starlark_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Starlark, "case.star", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn for_loop_target_writes_the_existing_binding() {
    let source = "def f(xs):\n    i = 0\n    for i in xs:\n        pass\n    i += 1\n";
    with_starlark_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(is.len(), 1, "one binding for i: {found:?}");
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (2, 3, 5),
            "for header and trailing write both reach i, was (1, 5, 5) when the loop target recorded nothing: {found:?}"
        );
        assert_eq!(
            is[0].loop_depth, 0,
            "the header binds before the loop frame opens: {found:?}"
        );
    });
}

#[test]
fn for_tuple_target_writes_each_name() {
    let source = "def f(pairs):\n    a = 0\n    b = 0\n    for a, b in pairs:\n        pass\n    a += 1\n    b += 1\n";
    with_starlark_bindings(source, |found| {
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
            (2, 4, 6),
            "header and trailing writes both reach a, was (1, 6, 6) when tuple loop targets recorded nothing: {found:?}"
        );
        assert_eq!(
            (
                bs[0].write_lines,
                bs[0].first_write_line,
                bs[0].last_write_line
            ),
            (2, 4, 7),
            "header and trailing writes both reach b, was (1, 7, 7) when tuple loop targets recorded nothing: {found:?}"
        );
    });
}

#[test]
fn for_loop_target_declares_a_missing_binding() {
    let source = "def f(xs):\n    for i in xs:\n        pass\n    i += 1\n";
    with_starlark_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            1,
            "the loop header declares i, was 0 bindings when the header recorded nothing: {found:?}"
        );
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (1, 4, 4),
            "only the trailing write counts, the header is the declaration: {found:?}"
        );
    });
}

#[test]
fn comprehension_variable_records_nothing() {
    let source = "def f(xs):\n    y = [i * 2 for i in xs]\n    i = 0\n    i += 1\n";
    with_starlark_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            1,
            "the comprehension keeps no binding of its own: {found:?}"
        );
        assert_eq!(
            is[0].pos.line, 3,
            "the single i binding is the later declaration: {found:?}"
        );
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (1, 4, 4),
            "only the trailing write counts: {found:?}"
        );
    });
}
