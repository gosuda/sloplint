use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_csharp_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::CSharp, "case.cs", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn lambda_implicit_parameter_keeps_inner_write_local() {
    let source =
        "class A {\nvoid M() {\nint x = 0;\nSystem.Action<int> f = x => { x = 1; };\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the lambda parameter declares no binding: {found:?}"
        );
        assert_eq!(
            (xs[0].write_lines, xs[0].last_write_line),
            (0, 3),
            "wrong-before recorded the lambda x = 1 on the outer int x: {found:?}"
        );
    });
}

#[test]
fn sibling_for_init_does_not_extend_the_if_block_span() {
    let source = "class A {\nvoid F(int n) {\nif (n > 0) { int i = 1; Use(i); }\nfor (int i = 0; i < n; i++) { }\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            2,
            "wrong-before held a single binding for the if-block int i: {found:?}"
        );
        assert_eq!(
            (is[0].write_lines, is[0].last_write_line),
            (0, 3),
            "if-block int i keeps no writes: {found:?}"
        );
        assert_eq!(
            (
                is[1].pos.line,
                is[1].write_lines,
                is[1].first_write_line,
                is[1].last_write_line
            ),
            (4, 1, 4, 4),
            "for init i owns the header i++ on line 4: {found:?}"
        );
    });
}

#[test]
fn catch_variable_does_not_extend_the_try_block_span() {
    let source = "class A {\nvoid F() {\ntry { int e = 0; Use(e); } catch (System.Exception e) { e = null; }\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let es = named_all(found, "e");
        assert_eq!(
            es.len(),
            1,
            "the catch variable holds no binding of its own: {found:?}"
        );
        assert_eq!(
            (es[0].write_lines, es[0].last_write_line),
            (0, 3),
            "wrong-before recorded the catch e = null on the try-block int e: {found:?}"
        );
    });
}

#[test]
fn pattern_and_out_variables_declare_live_locals() {
    let source = "class A {\nvoid F(object o, string s) {\nif (o is int t) { t = 2; }\nint.TryParse(s, out var y);\ny = 2;\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let ts = named_all(found, "t");
        assert_eq!(
            ts.len(),
            1,
            "wrong-before dropped the pattern variable t without a fact: {found:?}"
        );
        assert_eq!(
            (ts[0].write_lines, ts[0].last_write_line),
            (1, 3),
            "the t = 2 write reaches the pattern binding: {found:?}"
        );
        let ys = named_all(found, "y");
        assert_eq!(
            ys.len(),
            1,
            "wrong-before dropped the out variable y without a fact: {found:?}"
        );
        assert_eq!(
            (ys[0].write_lines, ys[0].last_write_line),
            (1, 5),
            "the y = 2 write reaches the out binding: {found:?}"
        );
    });
}

#[test]
fn loop_write_without_inner_shadow_still_reaches_the_outer_binding() {
    let source =
        "class A {\nvoid F() {\nint v = 0;\nfor (int k = 0; k < 3; k++) { v = 1; }\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let vs = named_all(found, "v");
        assert_eq!(vs.len(), 1, "no inner v shadows the outer v: {found:?}");
        assert_eq!(
            (vs[0].write_lines, vs[0].last_write_line),
            (1, 4),
            "the loop v = 1 must still reach the outer v: {found:?}"
        );
    });
}

#[test]
fn for_init_declares_mutable_counter_with_body_writes() {
    let source = "class A {\nvoid F(int n, bool skip) {\nfor (int i = 0; i < n;) {\nif (skip) { i = i + 2; }\ni = 0;\n}\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            1,
            "wrong-before dropped the for init i without a binding: {found:?}"
        );
        assert_eq!(
            is[0].pos.line, 3,
            "wrong-before had no header binding for i: {found:?}"
        );
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (2, 4, 5),
            "wrong-before lost the two body writes on lines 4 and 5: {found:?}"
        );
    });
}

#[test]
fn foreach_variable_stays_shadow_only() {
    let source = "class A {\nvoid F(System.Collections.Generic.List<int> xs) {\nforeach (var x in xs) {\nx = 1;\n}\n}\n}\n";
    with_csharp_bindings(source, |found| {
        let found_x = named_all(found, "x");
        assert_eq!(
            found_x.len(),
            0,
            "foreach iteration variable stays read-only with no binding: {found:?}"
        );
    });
}
