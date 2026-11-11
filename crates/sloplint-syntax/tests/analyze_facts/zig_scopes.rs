use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_zig_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Zig, "case.zig", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn sibling_block_vars_hold_separate_bindings() {
    let source = "fn f(c: bool) void {\n    if (c) {\n        var n: u32 = 0;\n        n += 1;\n    } else {\n        var n: u32 = 5;\n        n += 2;\n    }\n}\n";
    with_zig_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "sibling vars hold separate bindings, was 1 when the else var merged into the if var: {found:?}"
        );
        assert_eq!(
            (
                ns[0].write_lines,
                ns[0].first_write_line,
                ns[0].last_write_line
            ),
            (1, 4, 4),
            "if-branch write stays on the first binding, was 2 writes ending at line 7 on the dead binding: {found:?}"
        );
        assert_eq!(
            (
                ns[1].write_lines,
                ns[1].first_write_line,
                ns[1].last_write_line
            ),
            (1, 7, 7),
            "else-branch write stays on the second binding: {found:?}"
        );
    });
}

#[test]
fn pointer_payload_write_stays_local() {
    let source = "fn f(xs: []u32) void {\n    {\n        var x: u32 = 0;\n        x += 1;\n    }\n    for (xs) |*x| x.* += 1;\n}\n";
    with_zig_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "payload capture registers no binding of its own: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 4, 4),
            "payload write is dropped, was 2 writes ending at line 6 on the closed block var: {found:?}"
        );
        assert_eq!(
            xs[0].loop_depth, 0,
            "dropped payload write adds no loop depth: {found:?}"
        );
    });
}

#[test]
fn var_type_annotation_records_no_write() {
    let source = "fn f() void {\n    {\n        var Node: u8 = 0;\n        Node += 1;\n    }\n    const Node = u8;\n    var x: Node = 0;\n    x += 1;\n}\n";
    with_zig_bindings(source, |found| {
        let nodes = named_all(found, "Node");
        assert_eq!(nodes.len(), 1, "const declares no binding: {found:?}");
        assert_eq!(
            (
                nodes[0].write_lines,
                nodes[0].first_write_line,
                nodes[0].last_write_line
            ),
            (1, 4, 4),
            "type annotation adds no write to the closed block binding: {found:?}"
        );
        let xs = named_all(found, "x");
        assert_eq!(xs.len(), 1, "one binding for x: {found:?}");
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (1, 8, 8),
            "only the increment writes x: {found:?}"
        );
    });
}
