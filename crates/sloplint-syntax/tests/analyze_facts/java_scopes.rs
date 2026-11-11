use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_java_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Java, "case.java", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn sibling_block_local_does_not_capture_a_later_write() {
    let source = "class A {\nvoid m(boolean c) {\nint n = 0;\nif (c) { int n = 1; n++; }\nn = 10;\nuse(n);\n}\n}\n";
    with_java_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "both the outer and the block-local n declare: {found:?}"
        );
        let outer = ns
            .iter()
            .find(|binding| binding.pos.line == 3)
            .expect("outer n: {found:?}");
        let inner = ns
            .iter()
            .find(|binding| binding.pos.line == 4)
            .expect("inner n: {found:?}");
        assert_eq!(
            (inner.write_lines, inner.last_write_line),
            (1, 4),
            "wrong-before recorded the later n = 10 on the dead block-local n: {found:?}"
        );
        assert_eq!(
            (outer.write_lines, outer.last_write_line),
            (1, 5),
            "the n = 10 write past the block reaches the outer n: {found:?}"
        );
    });
}

#[test]
fn for_init_write_does_not_leak_past_the_loop() {
    let source =
        "class A {\nvoid m(int lim) {\nfor (int i = 0; i < lim; i++) {\nuse(i);\n}\ni = 5;\n}\n}\n";
    with_java_bindings(source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            1,
            "the for init holds no binding of its own: {found:?}"
        );
        assert_eq!(
            (is[0].write_lines, is[0].last_write_line),
            (1, 3),
            "wrong-before recorded the post-loop i = 5 on the dead for init i: {found:?}"
        );
    });
}

#[test]
fn try_resource_shadows_the_outer_name_in_the_body() {
    let source = "class A {\nvoid m() {\nString s = \"outer\";\ntry (java.io.BufferedReader s = open()) {\ns = \"inner\";\n}\ns = \"after\";\n}\n}\n";
    with_java_bindings(source, |found| {
        let ss = named_all(found, "s");
        assert_eq!(
            ss.len(),
            1,
            "the resource s holds no binding of its own: {found:?}"
        );
        assert_eq!(
            (ss[0].write_lines, ss[0].last_write_line),
            (1, 7),
            "wrong-before recorded the try-body s = \"inner\" on the outer String s: {found:?}"
        );
    });
}

#[test]
fn catch_write_does_not_land_on_a_dead_block_local() {
    let source = "class A {\nvoid m(boolean c) {\nif (c) { int e = 1; e++; }\ntry { work(); } catch (RuntimeException e) { e = null; }\n}\n}\n";
    with_java_bindings(source, |found| {
        let es = named_all(found, "e");
        assert_eq!(
            es.len(),
            1,
            "the catch variable holds no binding of its own: {found:?}"
        );
        assert_eq!(
            (es[0].write_lines, es[0].last_write_line),
            (1, 3),
            "wrong-before recorded the catch e = null on the dead block-local e: {found:?}"
        );
    });
}

#[test]
fn instanceof_pattern_keeps_its_write_off_the_outer_name() {
    let source = "class A {\nvoid m(Object o) {\nString s = \"outer\";\nif (o instanceof String s) {\ns = \"inner\";\n}\ns = \"after\";\n}\n}\n";
    with_java_bindings(source, |found| {
        let ss = named_all(found, "s");
        assert_eq!(
            ss.len(),
            1,
            "the pattern variable s holds no binding of its own: {found:?}"
        );
        assert_eq!(
            (ss[0].write_lines, ss[0].last_write_line),
            (1, 7),
            "wrong-before recorded the pattern s = \"inner\" on the outer String s: {found:?}"
        );
    });
}

#[test]
fn switch_pattern_keeps_its_write_off_the_outer_name() {
    let source = "class A {\nvoid m(Object o) {\nString s = \"outer\";\nswitch (o) {\ncase String s -> { s = \"inner\"; }\ndefault -> { }\n}\ns = \"after\";\n}\n}\n";
    with_java_bindings(source, |found| {
        let ss = named_all(found, "s");
        assert_eq!(
            ss.len(),
            1,
            "the switch pattern s holds no binding of its own: {found:?}"
        );
        assert_eq!(
            (ss[0].write_lines, ss[0].last_write_line),
            (1, 8),
            "wrong-before recorded the case-body s = \"inner\" on the outer String s: {found:?}"
        );
    });
}

#[test]
fn lambda_parameter_stops_the_write_to_the_outer_local() {
    let source = "class A {\nvoid m() {\nint x = 0;\njava.util.function.IntUnaryOperator f = x -> { x = 1; return x; };\nx = 2;\n}\n}\n";
    with_java_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the lambda parameter declares no binding: {found:?}"
        );
        assert_eq!(
            (xs[0].write_lines, xs[0].last_write_line),
            (1, 5),
            "wrong-before recorded the lambda x = 1 on the outer int x: {found:?}"
        );
    });
}

#[test]
fn nested_block_write_without_inner_shadow_still_reaches_the_outer_binding() {
    let source = "class A {\nvoid m() {\nint v = 0;\nif (true) { v = 1; }\nv = 2;\n}\n}\n";
    with_java_bindings(source, |found| {
        let vs = named_all(found, "v");
        assert_eq!(vs.len(), 1, "no inner v shadows the outer v: {found:?}");
        assert_eq!(
            (vs[0].write_lines, vs[0].last_write_line),
            (2, 5),
            "the block v = 1 must still reach the outer v: {found:?}"
        );
    });
}

#[test]
fn old_style_case_local_stays_live_across_case_groups() {
    let source = "class C {\nvoid f(int k) {\nswitch (k) {\ncase 1:\nint n = 0;\nn = 1;\nbreak;\ncase 2:\nn = 2;\nn = 3;\nbreak;\n}\n}\n}\n";
    with_java_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "the case local declares one binding: {found:?}"
        );
        assert_eq!(
            (ns[0].write_lines, ns[0].last_write_line),
            (3, 10),
            "wrong-before popped n at the case 1 group exit and kept (1, 6): {found:?}"
        );
    });
}

#[test]
fn arrow_rule_block_local_does_not_capture_a_later_rule_write() {
    let source = "class C {\nvoid f(int k) {\nswitch (k) {\ncase 1 -> {\nint n = 0;\nn = 1;\n}\ndefault -> {\nn = 2;\n}\n}\n}\n}\n";
    with_java_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            1,
            "the arrow block declares one binding: {found:?}"
        );
        assert_eq!(
            (ns[0].write_lines, ns[0].last_write_line),
            (1, 6),
            "wrong-before widened the arrow block local past its block and kept (2, 9): {found:?}"
        );
    });
}
