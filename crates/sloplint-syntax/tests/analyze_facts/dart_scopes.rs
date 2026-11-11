//! Dart block scopes for write attribution.

use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_dart_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Dart, "case.dart", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn closure_body_var_does_not_capture_later_outer_write() {
    let source = "void f() {\nvar x = 0;\nvar g = () { var x = 1; x++; };\nx = 5;\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "x");
        assert_eq!(outer.len(), 2, "closure and outer hold separate x names");
        assert_eq!(
            (
                outer[0].write_lines,
                outer[0].first_write_line,
                outer[0].last_write_line
            ),
            (1, 4, 4),
            "wrong-before recorded x = 5 on the closure x, leaving the outer x unwritten"
        );
        assert_eq!(
            (
                outer[1].write_lines,
                outer[1].first_write_line,
                outer[1].last_write_line
            ),
            (1, 3, 3),
            "the closure x++ stays on the closure x"
        );
    });
}
#[test]
fn for_init_var_does_not_capture_later_outer_write() {
    let source = "void f() {\nvar i = 0;\nfor (var i = 0; i < 3; i++) {}\ni = 7;\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "i");
        assert_eq!(outer.len(), 2, "loop and outer hold separate i names");
        assert_eq!(
            (
                outer[0].write_lines,
                outer[0].first_write_line,
                outer[0].last_write_line
            ),
            (1, 4, 4),
            "wrong-before recorded i = 7 on the loop i, leaving the outer i unwritten"
        );
    });
}

#[test]
fn for_in_var_write_does_not_reach_the_outer_var() {
    let source = "void f(List<int> xs) {\nvar x = 0;\nfor (var x in xs) { x = 1; }\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "x");
        assert_eq!(
            outer.len(),
            2,
            "the var for-in x is a loop binding plus the outer x"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 2),
            "wrong-before recorded the loop x = 1 on the outer x"
        );
        assert_eq!(
            (
                outer[1].write_lines,
                outer[1].first_write_line,
                outer[1].last_write_line
            ),
            (1, 3, 3),
            "the loop x = 1 stays on the loop x"
        );
    });
}

#[test]
fn catch_param_write_does_not_reach_the_outer_var() {
    let source = "void f() {\nvar e = 0;\ntry { f(); } catch (e) { e = 1; }\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "e");
        assert_eq!(
            outer.len(),
            1,
            "the catch e is shadow-only so only the outer e is a binding"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 2),
            "wrong-before recorded the catch e = 1 on the outer e"
        );
    });
}

#[test]
fn catch_finally_write_reaches_the_outer_var() {
    let source =
        "void f() {\nvar e = 0;\ntry { f(); } catch (e, s) { print(s); }\nfinally { e = 1; }\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "e");
        assert_eq!(
            outer.len(),
            1,
            "the catch e is scoped to its body so the outer e is the only e binding"
        );
        assert_eq!(
            (
                outer[0].write_lines,
                outer[0].first_write_line,
                outer[0].last_write_line
            ),
            (1, 4, 4),
            "wrong-before dropped the finally e = 1 leaving the outer e unwritten"
        );
    });
}

#[test]
fn for_final_in_creates_no_binding() {
    let source = "void f(List<int> xs) {\nvar x = 0;\nfor (final x in xs) { x = 1; }\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "x");
        assert_eq!(
            outer.len(),
            1,
            "the final for-in x is shadow-only so only the outer x is a binding"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 2),
            "the loop x = 1 must not reach the outer x"
        );
    });
}

#[test]
fn if_case_var_write_does_not_reach_the_outer_var() {
    let source = "void f(Object? o) {\nvar x = 0;\nif (o case var x?) { x = 1; }\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "x");
        assert_eq!(
            outer.len(),
            1,
            "the case x is shadow-only so only the outer x is a binding"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 2),
            "wrong-before recorded the case x = 1 on the outer x"
        );
    });
}

#[test]
fn pattern_declaration_binds_every_name_in_the_pattern() {
    let source =
        "void f((int, int) p) {\nvar a = 0;\n{\nvar (a, b) = p;\na = 1;\n}\nprint(a);\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "a");
        assert_eq!(outer.len(), 2, "pattern and outer hold separate a names");
        assert_eq!(
            (
                outer[1].write_lines,
                outer[1].first_write_line,
                outer[1].last_write_line
            ),
            (1, 5, 5),
            "wrong-before recorded a = 1 on the outer a"
        );
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (0, 2),
            "the pattern a = 1 must not reach the outer a"
        );
        assert_eq!(
            named_all(found, "b").len(),
            1,
            "the second pattern name is a binding too"
        );
    });
}

#[test]
fn closure_write_without_shadowing_still_reaches_the_outer_var() {
    let source = "void f() {\nvar x = 0;\nvar g = () { x = 1; };\nx = 5;\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "x");
        assert_eq!(outer.len(), 1, "no shadowing x inside the closure");
        assert_eq!(
            (
                outer[0].write_lines,
                outer[0].first_write_line,
                outer[0].last_write_line
            ),
            (2, 3, 4),
            "the closure x = 1 must reach the outer x"
        );
    });
}

#[test]
fn closure_param_write_does_not_reach_the_outer_var() {
    let source = "void f() {\nvar x = 0;\nvar g = (x) { x = 1; };\nx = 5;\n}\n";
    with_dart_bindings(source, |found| {
        let outer = named_all(found, "x");
        assert_eq!(outer.len(), 1, "the closure parameter declares nothing new");
        assert_eq!(
            (outer[0].write_lines, outer[0].last_write_line),
            (1, 4),
            "wrong-before recorded the parameter x = 1 on the outer x"
        );
    });
}
