use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_scala_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Scala, "case.scala", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn sibling_block_var_ends_before_outer_write() {
    let source = "def f(c: Boolean): Int = {\n  var n = 0\n  if (c) {\n    var n = 1\n    n += 1\n  }\n  n += 10\n  n\n}\n";
    with_scala_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "inner and outer vars hold separate bindings: {found:?}"
        );
        assert_eq!(
            (ns[0].write_lines, ns[0].first_write_line),
            (1, 7),
            "post-block write lands on the outer binding, was 0 with the write stuck on the dead inner binding: {found:?}"
        );
        assert_eq!(
            ns[0].last_write_line, 7,
            "outer span ends at the post-block write: {found:?}"
        );
        assert_eq!(
            (ns[1].write_lines, ns[1].first_write_line),
            (1, 5),
            "inner binding keeps only its own write: {found:?}"
        );
    });
}

#[test]
fn for_generator_binder_shadows_outer_write() {
    let source = "def f(xs: List[Int]): Int = {\n  var x = 0\n  for (x <- xs) {\n    x += 1\n  }\n  x += 10\n  x\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the generator binds no outer-visible name: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 6, 6),
            "only the post-loop write reaches the outer binding, was 2 when the body write leaked out: {found:?}"
        );
    });
}

#[test]
fn case_binder_shadows_outer_write() {
    let source = "def f(v: Any): Int = {\n  var x = 0\n  v match {\n    case x => x += 1\n  }\n  x += 10\n  x\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the case pattern binds no outer-visible name: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 6, 6),
            "only the post-match write reaches the outer binding, was 2 when the arm write leaked out: {found:?}"
        );
    });
}

#[test]
fn named_argument_is_not_a_write() {
    let source = "def build(): Unit = {\n  val host = loadHost()\n  val a = Config(host = host, port = 80)\n  val b = Config(host = host, port = 81)\n}\n";
    with_scala_bindings(source, |found| {
        let hosts = named_all(found, "host");
        assert_eq!(
            hosts.len(),
            1,
            "named arguments declare no binding of their own: {found:?}"
        );
        assert_eq!(
            hosts[0].write_lines, 0,
            "reusing a local as a named argument records no write, was 2 with one write per call site: {found:?}"
        );
    });
}

#[test]
fn cats_inequality_is_not_a_write() {
    let source = "def f(): Int = {\n  var a = 1\n  val ok = a =/= 2\n  a\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "a");
        assert_eq!(xs.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            xs[0].write_lines, 0,
            "a =/= comparison records no write, was 1 when the operator counted as a compound assignment: {found:?}"
        );
    });
}

#[test]
fn compound_and_plain_assignment_still_write() {
    let source = "def f(): Int = {\n  var a = 0\n  a += 1\n  a = 2\n  a\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "a");
        assert_eq!(xs.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 3, 4),
            "+= and = keep recording writes on the same binding: {found:?}"
        );
    });
}
