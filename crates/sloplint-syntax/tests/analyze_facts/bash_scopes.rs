use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_bash_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Bash, "case.sh", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

fn function_with_gap(second: &str, third: &str, fortieth: &str) -> String {
    let mut source = String::from("f() {\n");
    source.push_str(second);
    source.push('\n');
    source.push_str(third);
    source.push('\n');
    for _ in 0..36 {
        source.push_str("  :\n");
    }
    source.push_str(fortieth);
    source.push_str("\n}\n");
    source
}

#[test]
fn for_in_variable_writes_the_function_binding() {
    let source = function_with_gap("  i=0", "  for i in a b; do :; done", "  i=5");
    with_bash_bindings(&source, |found| {
        let targets = named_all(found, "i");
        assert_eq!(
            targets.len(),
            1,
            "the loop reuses the function binding instead of declaring a fresh one: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (2, 3, 40),
            "iteration writes at line 3, was write_lines 1 with the loop write missing: {found:?}"
        );
    });
}

#[test]
fn subshell_assignment_does_not_reach_the_function() {
    let source = function_with_gap("  x=1", "  ( x=2 )", "  x=3");
    with_bash_bindings(&source, |found| {
        let targets = named_all(found, "x");
        assert_eq!(
            targets.len(),
            1,
            "the subshell holds no function-level binding of its own: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (1, 40, 40),
            "only the line 40 write counts, was write_lines 2 spanning lines 3 to 40: {found:?}"
        );
    });
}

#[test]
fn substitution_assignment_does_not_reach_the_function() {
    let source = "f() {\n  x=1\n  y=$(x=2)\n  x=3\n}\n";
    with_bash_bindings(source, |found| {
        let targets = named_all(found, "x");
        assert_eq!(
            targets.len(),
            1,
            "x keeps a single function binding: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (1, 4, 4),
            "the substitution write at line 3 is fork-local, was write_lines 2 spanning lines 3 to 4: {found:?}"
        );
        let outer = named_all(found, "y");
        assert_eq!(
            (outer.len(), outer[0].write_lines),
            (1, 0),
            "the substitution target itself still declares at line 3: {found:?}"
        );
    });
}

#[test]
fn local_declaration_still_binds_then_writes() {
    let source = "f() {\n  local x=1\n  x=2\n  x=3\n}\n";
    with_bash_bindings(source, |found| {
        let targets = named_all(found, "x");
        assert_eq!(
            targets.len(),
            1,
            "local keeps one function binding: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (2, 3, 4),
            "declare-then-write shape is unchanged by the loop and subshell fixes: {found:?}"
        );
    });
}

#[test]
fn prefixed_assignment_still_skipped() {
    let source = "f() {\n  x=1\n  X=2 env\n  x=3\n}\n";
    with_bash_bindings(source, |found| {
        assert!(
            named_all(found, "X").is_empty(),
            "a command-prefix assignment still binds nothing: {found:?}"
        );
        let targets = named_all(found, "x");
        assert_eq!(
            targets.len(),
            1,
            "x keeps a single function binding: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (1, 4, 4),
            "only the line 4 write counts, prefix line 3 stays skipped: {found:?}"
        );
    });
}

#[test]
fn deeply_nested_substitutions_leave_outer_write_count_unchanged() {
    let depth = 200;
    let mut source = String::from("f() {\n  x=1\n  y=$(x=2");
    for n in 3..=(depth + 1) {
        source.push_str(&format!("; echo $(x={n}"));
    }
    for _ in 0..depth {
        source.push(')');
    }
    source.push_str("\n  x=999\n}\n");
    with_bash_bindings(&source, |found| {
        let targets = named_all(found, "x");
        assert_eq!(
            targets.len(),
            1,
            "nested substitutions hold no function-level binding of their own: {found:?}"
        );
        assert_eq!(
            (
                targets[0].write_lines,
                targets[0].first_write_line,
                targets[0].last_write_line
            ),
            (1, 4, 4),
            "only the line 4 write counts, all nested writes stay fork-local: {found:?}"
        );
        let outer = named_all(found, "y");
        assert_eq!(
            (outer.len(), outer[0].write_lines),
            (1, 0),
            "the outermost substitution target still declares at line 3: {found:?}"
        );
    });
}

#[test]
fn function_defined_inside_a_fork_keeps_its_own_writes() {
    let source = "out=$(\n  f() {\n    acc=0\n    acc=1\n    acc=2\n  }\n  f\n)\n";
    with_bash_bindings(source, |found| {
        let targets = named_all(found, "acc");
        assert_eq!(
            (targets.len(), targets[0].write_lines),
            (1, 2),
            "a fork around the function does not hide writes inside it: {found:?}"
        );
    });
}

#[test]
fn increment_inside_a_fork_does_not_reach_the_function() {
    let source = "f() {\n  local n=0\n  n=1\n  out=$( (( n++ )) )\n  (( n++ ))\n}\n";
    with_bash_bindings(source, |found| {
        let targets = named_all(found, "n");
        assert_eq!(
            (
                targets.len(),
                targets[0].write_lines,
                targets[0].last_write_line
            ),
            (1, 2, 5),
            "the line 4 increment runs in a fork, was 3 write lines: {found:?}"
        );
    });
}
