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

#[test]
fn extractor_head_is_not_a_binder() {
    let source = "def f(v: Any): Int = {\n  var Foo = 0\n  v match {\n    case Foo(x) => x += 1\n  }\n  Foo += 10\n  Foo\n}\n";
    with_scala_bindings(source, |found| {
        let foos = named_all(found, "Foo");
        assert_eq!(
            foos.len(),
            1,
            "the extractor head binds no outer-visible name: {found:?}"
        );
        assert_eq!(
            (
                foos[0].write_lines,
                foos[0].first_write_line,
                foos[0].last_write_line
            ),
            (1, 6, 6),
            "the write on line 6 reaches the outer binding, was 0 when the head shadowed it: {found:?}"
        );
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            0,
            "the arm binder creates no binding of its own: {found:?}"
        );
    });
}

#[test]
fn extractor_argument_still_shadows_outer_write() {
    let source = "def f(v: Any): Int = {\n  var y = 0\n  v match {\n    case Foo(y) => y += 1\n  }\n  y += 10\n  y\n}\n";
    with_scala_bindings(source, |found| {
        let ys = named_all(found, "y");
        assert_eq!(
            ys.len(),
            1,
            "the arm pattern binds no outer-visible name: {found:?}"
        );
        assert_eq!(
            (
                ys[0].write_lines,
                ys[0].first_write_line,
                ys[0].last_write_line
            ),
            (1, 6, 6),
            "only the post-match write reaches the outer binding, was 2 when the arm write leaked out: {found:?}"
        );
    });
}

#[test]
fn given_extractor_binds_its_argument() {
    // `case given(status)` is a `given` extractor — `status` binds a local
    // that shadows an outer name for writes inside the arm. The tuple-type
    // reading `case given (status, String)` keeps binding nothing.
    let source = "def f(v: Any): Int = {\n  var status = 0\n  v match {\n    case given(status) => status += 1\n    case given (status2, other) => status += 2\n  }\n  status += 10\n  status\n}\n";
    with_scala_bindings(source, |found| {
        let ss = named_all(found, "status");
        assert_eq!(
            ss.len(),
            1,
            "the extractor arg shadows the outer var, not a second binding: {found:?}"
        );
        assert_eq!(
            (
                ss[0].write_lines,
                ss[0].first_write_line,
                ss[0].last_write_line
            ),
            (2, 5, 7),
            "the extractor arm's write is shadowed; only the tuple arm and post-match writes land: {found:?}"
        );
    });
}

#[test]
fn named_argument_on_value_shadowing_method_is_not_a_write() {
    // `val f` shadows the outer `def f(other: Int)`: `f(counter = 1)` calls
    // the value's `apply`, so the named argument is not an assignment and
    // no write lands on the outer `counter`.
    let source = "def outer(): Unit = {\n  var counter = 0\n  {\n    val f = (x: Int) => x\n    f(counter = 1)\n  }\n}\n";
    with_scala_bindings(source, |found| {
        let counters = named_all(found, "counter");
        assert_eq!(counters.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            counters[0].write_lines, 0,
            "the shadowed callee's named argument records no write: {found:?}"
        );
    });
}

#[test]
fn multi_argument_given_extractor_binds_its_arguments() {
    // `case given(x, y)` is a two-argument extractor in Scala 2: both names
    // bind locals that shadow outer variables inside the arm.
    let source = "def f(v: Any): Int = {\n  var x = 0\n  var y = 0\n  v match {\n    case given(x, y) => x += 1\n    case _ => 0\n  }\n  x += 10\n  y += 10\n  x + y\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 8, 8),
            "the extractor arm's write is shadowed; only the post-match write lands: {found:?}"
        );
        let ys = named_all(found, "y");
        assert_eq!(ys.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                ys[0].write_lines,
                ys[0].first_write_line,
                ys[0].last_write_line
            ),
            (1, 9, 9),
            "the extractor arm binds y too; only the post-match write lands: {found:?}"
        );
    });
}

#[test]
fn given_tuple_type_arm_binds_nothing() {
    // `case given (Int, String)` matches a tuple-typed given instance: the
    // type names bind no locals, so a write inside the arm still reaches
    // the outer variable.
    let source = "def f(v: Any): Int = {\n  var status = 0\n  v match {\n    case given (Int, String) => status += 1\n    case _ => 0\n  }\n  status\n}\n";
    with_scala_bindings(source, |found| {
        let ss = named_all(found, "status");
        assert_eq!(ss.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                ss[0].write_lines,
                ss[0].first_write_line,
                ss[0].last_write_line
            ),
            (1, 4, 4),
            "the tuple-given arm binds nothing; the write reaches the outer var: {found:?}"
        );
    });
}

#[test]
fn symbolic_named_argument_is_not_a_write() {
    // Scala allows symbolic names. `sink(++ = 2)` names the `++` parameter,
    // not the enclosing `var ++`. The left side parses as an
    // `operator_identifier`, which the named-argument guard must screen the
    // same way it screens a plain `identifier`.
    let source = "object T {\n  def sink(++ : Int): Int = 0\n  def m(): Int = {\n    var ++ = 0\n    ++ = ++ + 1\n    sink(++ = 2)\n    ++\n  }\n}\n";
    with_scala_bindings(source, |found| {
        let names = named_all(found, "++");
        assert_eq!(names.len(), 1, "one binding for the var: {found:?}");
        assert_eq!(
            (names[0].write_lines, names[0].first_write_line),
            (1, 5),
            "the symbolic named argument records no write, was 2 with the left side miscounted: {found:?}"
        );
    });
}
