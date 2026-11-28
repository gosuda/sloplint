use sloplint_core::facts::{MutBinding, WildcardArm};
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_scala_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Scala, "case.scala", source);
    read(&analysis.facts.bindings)
}

fn wildcard_arms(source: &str) -> Vec<WildcardArm> {
    run_clean(Lang::Scala, "case.scala", source)
        .facts
        .wildcard_arms
        .clone()
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
fn sibling_for_binder_does_not_shadow_later_call() {
    // A loop binder that already ended is out of scope: after
    // `for (f <- xs) ...`, the call `f(counter = 1)` targets the outer
    // `def f`, whose parameter list has no `counter`, so the assignment
    // writes `counter` (a Scala 2.12 `Unit`-valued assignment).
    let source = "def f(other: Unit) = ()\ndef m(xs: List[Int]): Unit = {\n  var counter = 0\n  for (f <- xs) println(f)\n  f(counter = 1)\n}\n";
    with_scala_bindings(source, |found| {
        let counters = named_all(found, "counter");
        assert_eq!(counters.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            counters[0].write_lines, 1,
            "the ended loop binder must not hide the write: {found:?}"
        );
    });
}

#[test]
fn sibling_case_binder_does_not_shadow_later_arm_call() {
    // An earlier arm's binder is out of scope in a later arm: after
    // `case f: String => ...`, the call `f(counter = 1)` targets the outer
    // `def f`, so the assignment writes `counter`.
    let source = "def f(other: Unit) = ()\ndef m(v: Any): Unit = {\n  var counter = 0\n  v match {\n    case f: String => println(f)\n    case _ => f(counter = 1)\n  }\n}\n";
    with_scala_bindings(source, |found| {
        let counters = named_all(found, "counter");
        assert_eq!(counters.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            counters[0].write_lines, 1,
            "the earlier arm binder must not hide the write: {found:?}"
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
fn lowercase_type_alias_tuple_arm_binds_nothing() {
    // With `type status = Int` in scope, `case given (status, String)`
    // matches a tuple-typed given: `status` is a type, so the arm binds
    // nothing and the write reaches the outer variable.
    let source = "object T {\n  type status = Int\n  def f(v: Any): Int = {\n    var status = 0\n    v match {\n      case given (status, String) => status += 1\n      case _ => 0\n    }\n    status\n  }\n}\n";
    with_scala_bindings(source, |found| {
        let ss = named_all(found, "status");
        assert_eq!(ss.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                ss[0].write_lines,
                ss[0].first_write_line,
                ss[0].last_write_line
            ),
            (1, 6, 6),
            "the alias tuple arm binds nothing; the write reaches the outer var: {found:?}"
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

#[test]
fn symbolic_call_with_unlisted_name_records_the_write() {
    // A symbolic callee resolves like any other: `++(%% = 2)` consults the
    // visible `def ++`, whose parameter list has no `%%`, so the assignment
    // writes `%%` instead of vanishing as a named argument.
    let source = "object T {\n  def ++(arg: Unit): Int = 0\n  def m(): Int = {\n    var %% = 0\n    ++(%% = 2)\n    %%\n  }\n}\n";
    with_scala_bindings(source, |found| {
        let names = named_all(found, "%%");
        assert_eq!(names.len(), 1, "one binding for the var: {found:?}");
        assert_eq!(
            names[0].write_lines, 1,
            "the assignment to the non-parameter name records the write: {found:?}"
        );
    });
}

#[test]
fn given_extractor_binds_underscore_prefixed_argument() {
    // `case given(_status, other)` is an extractor: an underscore-prefixed
    // name binds a local like any lowercase name, shadowing the outer
    // variable for writes inside the arm. Before the fix the underscore
    // rejected the whole arm and both writes leaked out.
    let source = "def f(v: Any): Int = {\n  var _status = 0\n  var other = 0\n  v match {\n    case given(_status, other) => _status += 1\n    case _ => 0\n  }\n  _status += 10\n  other += 10\n  _status + other\n}\n";
    with_scala_bindings(source, |found| {
        let ss = named_all(found, "_status");
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
            (1, 8, 8),
            "the extractor arm's write is shadowed; only the post-match write lands: {found:?}"
        );
        let os = named_all(found, "other");
        assert_eq!(os.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                os[0].write_lines,
                os[0].first_write_line,
                os[0].last_write_line
            ),
            (1, 9, 9),
            "only the post-match write reaches the outer binding: {found:?}"
        );
    });
}

#[test]
fn given_extractor_with_wildcard_binds_named_argument() {
    // `case given(x, _)` is an extractor with a wildcard member: the
    // wildcard binds nothing but must not discard `x`. Before the fix the
    // arm bound nothing and the write leaked to the outer `x`.
    let source = "def f(v: Any): Int = {\n  var x = 0\n  v match {\n    case given(x, _) => x += 1\n    case _ => 0\n  }\n  x += 10\n  x\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 7, 7),
            "the extractor arm's write is shadowed; only the post-match write lands: {found:?}"
        );
    });
}

#[test]
fn given_extractor_with_imported_type_alias_binds_nothing() {
    // `case given(_status, other)` is a given of tuple type when both
    // members are type aliases in scope through `import Types.*`: nothing
    // binds, so the arm write lands on the outer variable. Before the fix
    // the alias lookup missed imports and both writes went to an invented
    // shadowing binder.
    let source = "object Types {\n  type _status = Int\n  type other = String\n}\ndef f(v: Any): Int = {\n  import Types.*\n  var _status = 0\n  v match {\n    case given(_status, other) => _status += 1\n    case Other() => 0\n    case _ => 0\n  }\n  _status += 10\n  _status\n}\n";
    with_scala_bindings(source, |found| {
        let ss = named_all(found, "_status");
        assert_eq!(
            ss.len(),
            1,
            "no invented binder: only the outer var exists: {found:?}"
        );
        assert_eq!(
            (
                ss[0].write_lines,
                ss[0].first_write_line,
                ss[0].last_write_line
            ),
            (2, 9, 13),
            "both the arm write and the post-match write land on the outer var: {found:?}"
        );
    });
}

#[test]
fn given_extractor_with_type_shaped_member_binds_the_binder() {
    // `case given(x, None)` is a Scala 2 extractor application with a
    // type-shaped member: `x` binds a local while uppercase `None` reads
    // as a tuple type. Before the fix the uppercase member discarded the
    // whole arm and the write leaked to the outer `x`.
    let source = "def f(v: Any): Int = {\n  var x = 0\n  v match {\n    case given(x, None) => x += 1\n    case _ => 0\n  }\n  x += 10\n  x\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 7, 7),
            "the extractor arm's write is shadowed; only the post-match write lands: {found:?}"
        );
    });
}

// The outer `_status` var's (write count, first, last write line) when
// `case given(_status, other)` follows `import_line` and `Types` declares
// the type members `st` and `_status`. A type import leaves the arm write
// on the outer var, `(2, 9, 12)`; a fresh binder shadows it, `(1, 12, 12)`.
fn outer_status_writes(import_line: &str) -> (u32, u32, u32) {
    let source = import_case_source(import_line);
    with_scala_bindings(&source, |found| {
        let ss = named_all(found, "_status");
        assert_eq!(ss.len(), 1, "only the outer var binds: {found:?}");
        (
            ss[0].write_lines,
            ss[0].first_write_line,
            ss[0].last_write_line,
        )
    })
}

// The `case given(_status, other)` match `outer_status_writes` analyzes,
// with `import_line` spliced in.
fn import_case_source(import_line: &str) -> String {
    format!(
        "object Types {{\n  type st = Int\n  type _status = Int\n}}\ndef f(v: Any): Int = {{\n  {import_line}\n  var _status = 0\n  v match {{\n    case given(_status, other) => _status += 1\n    case _ => 0\n  }}\n  _status += 10\n  _status\n}}\n"
    )
}

fn wildcard_variant_count(source: &str) -> u32 {
    let arms = wildcard_arms(source);
    assert_eq!(arms.len(), 1, "one wildcard arm: {arms:?}");
    arms[0].variant_count
}

#[test]
fn bare_member_import_before_a_comma_names_a_type_alias() {
    // Each comma-separated import expression resolves on its own, so the
    // bare `Types._status` before the comma imports the type.
    assert_eq!(
        outer_status_writes("import Types._status, Types.st"),
        (2, 9, 12),
        "the imported alias binds nothing; the arm write lands outside"
    );
    assert_eq!(
        wildcard_variant_count(&import_case_source("import Types._status, Types.st")),
        1,
        "the arm stays an extractor application and counts as a variant"
    );
}

#[test]
fn renamed_member_import_names_a_type_alias() {
    // `Types.st as _status` and `Types.{st => _status}` bring the type
    // `st` in under the name `_status`.
    for import_line in ["import Types.st as _status", "import Types.{st => _status}"] {
        assert_eq!(
            outer_status_writes(import_line),
            (2, 9, 12),
            "`{import_line}` makes `_status` an alias that binds nothing"
        );
        assert_eq!(
            wildcard_variant_count(&import_case_source(import_line)),
            1,
            "`{import_line}` leaves an extractor arm that counts as a variant"
        );
    }
}

#[test]
fn hidden_member_stays_out_of_a_wildcard_import() {
    // `{_status => _, *}` hides `_status` from the wildcard, so the
    // extractor member is a fresh binder that shadows the arm write.
    assert_eq!(
        outer_status_writes("import Types.{_status => _, *}"),
        (1, 12, 12),
        "the hidden name binds; only the post-match write lands outside"
    );
    assert_eq!(
        wildcard_variant_count(&import_case_source("import Types.{_status => _, *}")),
        1,
        "the shadowing arm is still an extractor application and counts"
    );
}

#[test]
fn qualified_import_does_not_borrow_unrelated_aliases() {
    // `Actual.Types` holds no aliases, but an unrelated top-level
    // `Types` does. The qualified import must not borrow them: the arm
    // binders shadow without creating bindings of their own, so the
    // outer `x` keeps the only binding and the arm write never lands
    // on it. A borrowed alias would read the arm as a tuple type
    // instead, landing the arm write outside and dropping the variant.
    let source = "object Types {\n  type x = Int\n  type y = Int\n}\nobject Actual {\n  object Types {\n  }\n}\ndef f(v: Any): Int = {\n  import Actual.Types.*\n  var x = 0\n  v match {\n    case given(x, y) => x += 1\n    case _ => 0\n  }\n  x += 10\n  x\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "only the outer var binds: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 16, 16),
            "the arm write is shadowed; only the post-match write lands outside: {found:?}"
        );
    });
    assert_eq!(
        wildcard_variant_count(source),
        1,
        "the arm is an extractor application, not a borrowed tuple type"
    );
}

#[test]
fn lowercase_type_parameter_is_not_a_binder() {
    // `t` is a type parameter of `f`, so `case given(t, Int)` is a
    // tuple-typed given: it binds nothing and counts no variant.
    let source = "def f[t](v: Any): Int = {\n  v match {\n    case given(t, Int) => 1\n    case Other(x) => x\n    case _ => 0\n  }\n}\n";
    with_scala_bindings(source, |found| {
        assert!(
            named_all(found, "t").is_empty(),
            "the type parameter binds no local: {found:?}"
        );
        // The constructor arm's binder shadows without creating a
        // binding of its own, so nothing binds anywhere here.
        assert!(
            found.is_empty(),
            "no binding is recorded: {found:?}"
        );
    });
    assert_eq!(
        wildcard_variant_count(source),
        1,
        "only the constructor arm counts; the tuple-typed given does not"
    );
}

#[test]
fn inline_comment_does_not_break_a_type_import() {
    // A block comment inside a qualified path is transparent: the
    // qualifier still resolves as a whole. Dropping the qualifier at
    // the comment would re-resolve from `Types` alone and borrow the
    // unrelated top-level aliases.
    let source = "object Types {\n  type x = Int\n  type y = Int\n}\nobject Actual {\n  object Types {\n  }\n}\ndef f(v: Any): Int = {\n  import Actual /* note */ .Types.*\n  var x = 0\n  v match {\n    case given(x, y) => x += 1\n    case _ => 0\n  }\n  x += 10\n  x\n}\n";
    with_scala_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "only the outer var binds: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 16, 16),
            "the arm write is shadowed; only the post-match write lands outside: {found:?}"
        );
    });
    assert_eq!(
        wildcard_variant_count(source),
        1,
        "the arm is an extractor application, not a borrowed tuple type"
    );
}

#[test]
fn comment_inside_a_given_tuple_keeps_both_binders() {
    // A note between the extractor members must not discard them: both
    // shadow, so the arm write never lands outside, and the arm still
    // counts as a variant.
    let source = "def f(v: Any): Int = {\n  var x = 0\n  var y = 0\n  v match {\n    case given(x, /* explanation */ y) => x += y\n    case _ => 0\n  }\n  x += 10\n  x + y\n}\n";
    with_scala_bindings(source, |found| {
        // Arm binders shadow without creating bindings of their own.
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "only the outer var binds: {found:?}");
        assert_eq!(
            (xs[0].write_lines, xs[0].first_write_line, xs[0].last_write_line),
            (1, 8, 8),
            "the arm write is shadowed; only the post-match write lands outside: {found:?}"
        );
    });
    assert_eq!(
        wildcard_variant_count(source),
        1,
        "the commented arm is still an extractor application and counts"
    );
}

#[test]
fn symbolic_param_shadowing_method_is_not_a_write() {
    // A parameter named `++` shadows the outer `def ++`: `++(count = 1)`
    // calls the parameter value's `apply`, so the named argument is not
    // an assignment and no write lands on the outer `count`.
    let source = "class Sink {\n  def apply(count: Int): Int = count\n}\ndef ++(other: Int): Int = 0\ndef run(++ : Sink): Int = {\n  var count = 0\n  ++(count = 1)\n  count\n}\n";
    with_scala_bindings(source, |found| {
        let counts = named_all(found, "count");
        assert_eq!(counts.len(), 1, "outer var keeps its binding: {found:?}");
        assert_eq!(
            counts[0].write_lines, 0,
            "the shadowing parameter's named argument records no write: {found:?}"
        );
    });
}

#[test]
fn default_arg_call_past_same_clause_param_records_the_write() {
    // A parameter is not in scope in its own parameter clause (SLS 4.6), so
    // a call inside a default value that names a same-clause parameter
    // resolves to the outer method of that name. The assignment is a real
    // write, not a named argument.
    let source = "object T {\n  def sink(ignored: Unit): Int = 0\n  def run(sink: Unit => Int, value: Int = { var count = 0; sink(count = 1); count }): Int = value\n}\n";
    with_scala_bindings(source, |found| {
        let counts = named_all(found, "count");
        assert_eq!(
            counts.len(),
            1,
            "the default block holds one count binding: {found:?}"
        );
        assert_eq!(
            counts[0].write_lines, 1,
            "the call through the outer method records the write: {found:?}"
        );
    });
}
