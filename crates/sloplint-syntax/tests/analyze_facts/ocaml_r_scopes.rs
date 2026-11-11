use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_ocaml<T>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> T) -> T {
    let analysis = run_clean(Lang::Ocaml, "case.ml", source);
    read(&analysis.facts.bindings)
}

fn with_r<T>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> T) -> T {
    let analysis = run_clean(Lang::R, "case.R", source);
    read(&analysis.facts.bindings)
}

fn named<'a>(found: &'a [MutBinding<'a>], name: &str) -> &'a MutBinding<'a> {
    found
        .iter()
        .find(|binding| binding.name == name)
        .unwrap_or_else(|| panic!("no binding named {name} in {found:?}"))
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn ocaml_anonymous_fun_param_hides_outer_ref() {
    let source = "let total xs =\n  let x = ref 0 in\n  List.iter (fun x -> x := !x + 1) xs;\n  x := 1\n\nlet collect xs =\n  List.map (fun v -> let acc = ref v in acc := 2; !acc) xs";
    with_ocaml(source, |found| {
        let outer = named(found, "x");
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line
            ),
            (4, 1, 4),
            "wrong-before recorded the fun x := on the outer x for 2 writes: {found:?}"
        );
        let acc = named(found, "acc");
        assert_eq!(
            (acc.write_lines, acc.last_write_line),
            (1, 7),
            "the closure-local acc keeps its own write: {found:?}"
        );
    });
}

#[test]
fn ocaml_fun_write_without_shadowing_reaches_outer() {
    let source =
        "let total xs =\n  let x = ref 0 in\n  List.iter (fun y -> x := !x + 1) xs;\n  x := 1\n";
    with_ocaml(source, |found| {
        let outer = named(found, "x");
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line
            ),
            (3, 2, 4),
            "the fun body write to an unshadowed x must reach the outer x: {found:?}"
        );
    });
}

#[test]
fn r_anonymous_function_param_and_local_stay_inside() {
    let source = "f <- function(xs) {\n  x <- 0\n  ys <- lapply(xs, function(x) { x <- x + 1; x })\n  x <- 5\n}\n\nf <- function(xs) {\n  lapply(xs, function(v) { r <- v })\n  r <- 2\n}";
    with_r(source, |found| {
        let outer = named(found, "x");
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line
            ),
            (4, 1, 4),
            "wrong-before recorded the closure x <- on the outer x for 2 writes: {found:?}"
        );
        let rs = named_all(found, "r");
        assert_eq!(
            rs.len(),
            2,
            "wrong-before merged the closure r into the outer r for 1 binding: {found:?}"
        );
        for binding in rs {
            assert_eq!(
                (binding.write_lines, binding.last_write_line),
                (0, binding.pos.line),
                "neither the closure r nor the outer r is written: {found:?}"
            );
        }
    });
}

#[test]
fn r_plain_assign_inside_closure_stays_local() {
    let source =
        "f <- function(xs) {\n  x <- 0\n  lapply(xs, function(v) { x <- x + 1 })\n  x <- 5\n}";
    with_r(source, |found| {
        let bindings = named_all(found, "x");
        assert_eq!(
            bindings.len(),
            2,
            "plain <- inside the closure declares a local x: {found:?}"
        );
        let outer = bindings
            .iter()
            .find(|binding| binding.pos.line == 2)
            .unwrap_or_else(|| panic!("no outer x in {found:?}"));
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line
            ),
            (4, 1, 4),
            "only x <- 5 writes the outer x: {found:?}"
        );
        let inner = bindings
            .iter()
            .find(|binding| binding.pos.line == 3)
            .unwrap_or_else(|| panic!("no inner x in {found:?}"));
        assert_eq!(
            (inner.write_lines, inner.last_write_line),
            (0, 3),
            "the closure x <- declares a local with no write: {found:?}"
        );
    });
}

#[test]
fn r_super_assignment_records_no_write() {
    let source = "make <- function() {\n  x <- 0\n  g <- function(x) { x <<- 1 }\n  g\n}\n\nmake <- function() {\n  x <- 0\n  g <- function() { x <- 2; x <<- 1 }\n  g\n}";
    with_r(source, |found| {
        for binding in found.iter().filter(|binding| binding.name == "x") {
            assert_eq!(
                binding.write_lines, 0,
                "wrong-before gave the inner x one <<- write: {found:?}"
            );
        }
    });
}

#[test]
fn ocaml_tuple_pattern_shadows_outer_ref() {
    let source =
        "let f p =\n  let n = ref 0 in\n  (match p with (_, n) -> n := 1 | _ -> ());\n  !n\n";
    with_ocaml(source, |found| {
        let outer = named(found, "n");
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (0, 2),
            "wrong-before recorded the arm n := on the outer n for 1 write: {found:?}"
        );
    });
}

#[test]
fn ocaml_nested_let_in_restores_outer_binding() {
    let source = "let f () =\n  let x = ref 0 in\n  (let x = ref 1 in incr x);\n  x := 2\n";
    with_ocaml(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "inner and outer x are separate bindings: {found:?}"
        );
        let outer = xs
            .iter()
            .find(|binding| binding.pos.line == 2)
            .unwrap_or_else(|| panic!("no outer x in {found:?}"));
        assert_eq!(
            (
                outer.first_write_line,
                outer.write_lines,
                outer.last_write_line
            ),
            (4, 1, 4),
            "wrong-before left the write on the dead inner x and the outer x at 0 writes: {found:?}"
        );
    });
}
