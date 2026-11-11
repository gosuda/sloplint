use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_python_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Python, "case.py", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn for_target_rebinds_the_function_binding() {
    let source = "def f(xs):\n    i = 0\n    for i in xs:\n        pass\n    i += 1\n";
    with_python_bindings(source, |found| {
        let targets = named_all(found, "i");
        assert_eq!(
            targets.len(),
            1,
            "loop target reuses the function binding: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (2, 3, 5),
            "loop line and += line both write, was (1, 5, 5) when the loop target was ignored: {found:?}"
        );
    });
}

#[test]
fn loop_target_declares_the_first_binding() {
    let source = "def f(xs):\n    for i in xs:\n        pass\n    i += 1\n";
    with_python_bindings(source, |found| {
        let targets = named_all(found, "i");
        assert_eq!(
            targets.len(),
            1,
            "loop target declares the binding, was 0 when it created nothing: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (1, 4, 4),
            "only the += line counts as a write after the loop declaration: {found:?}"
        );
    });
}

#[test]
fn walrus_writes_the_named_binding() {
    let source = "def f(xs):\n    x = 0\n    (x := len(xs))\n    return x\n";
    with_python_bindings(source, |found| {
        let targets = named_all(found, "x");
        assert_eq!(
            targets.len(),
            1,
            "walrus reuses the function binding: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (1, 3, 3),
            "walrus line writes, was (0, 2, 2) when named_expression was ignored: {found:?}"
        );
    });
}

#[test]
fn comprehension_variable_records_nothing() {
    let source = "def f(xs):\n    ys = [i * 2 for i in xs]\n    return ys\n";
    with_python_bindings(source, |found| {
        assert!(
            named_all(found, "i").is_empty(),
            "comprehension variable stays local: {found:?}"
        );
        assert_eq!(
            named_all(found, "ys").len(),
            1,
            "the assignment target still binds: {found:?}"
        );
    });
}

#[test]
fn with_attribute_alias_binds_nothing() {
    let source =
        "def f(p):\n    with open(p) as self.fh:\n        pass\n    self.x = 1\n    self.x = 2\n";
    with_python_bindings(source, |found| {
        assert!(
            named_all(found, "self").is_empty(),
            "attribute alias binds no name, was 1 when identifiers were collected out of it: {found:?}"
        );
    });
}

#[test]
fn nested_with_alias_binds_once_on_its_own_line() {
    let source = "def f(a, b):\n    with a() as x:\n        with b() as y:\n            pass\n    return x\n";
    with_python_bindings(source, |found| {
        let inner = named_all(found, "y");
        assert_eq!(inner.len(), 1, "inner alias holds one binding: {found:?}");
        assert_eq!(
            (
                inner[0].write_lines,
                inner[0].first_write_line,
                inner[0].last_write_line
            ),
            (0, 3, 3),
            "outer statement must not rebind it, was (1, 3, 3) when the body was descended into: {found:?}"
        );
    });
}
