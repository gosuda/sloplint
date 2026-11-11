//! Mutable-span write facts observed through `analyze` only.
//!
//! Each write that reaches a binding through an assignment, a compound
//! assignment, or an increment rooted at the binding advances the binding's
//! span. A read-only call leaves the span at the declaration line.

use sloplint_core::lang::Lang;

use super::audit_analyze;

#[test]
fn audit_dart_write_assignment_advances_span() {
    for write in ["total = 1", "total += 2", "total.value = 1", "total[0] = 1"] {
        let src = format!("void f() {{\n var total = value();\n {write};\n}}\n");
        let a = audit_analyze(Lang::Dart, "w.dart", &src);
        assert!(a.parse_error.is_none(), "{write} parses");
        let b = a
            .facts
            .bindings
            .iter()
            .find(|b| b.name == "total")
            .expect("total binding exists");
        assert_eq!(
            (b.first_write_line, b.write_lines, b.last_write_line),
            (3, 1, 3),
            "{write} updates its root binding once"
        );
        assert_eq!(b.loop_depth, 0, "{write} stays at depth 0");
    }
}

#[test]
fn audit_dart_write_increments_advance_span() {
    for write in ["total++", "++total"] {
        let src = format!("void f() {{\n var total = 0;\n {write};\n}}\n");
        let a = audit_analyze(Lang::Dart, "w.dart", &src);
        assert!(a.parse_error.is_none(), "{write} parses");
        let b = a
            .facts
            .bindings
            .iter()
            .find(|b| b.name == "total")
            .expect("total binding exists");
        assert_eq!(
            (b.first_write_line, b.write_lines, b.last_write_line),
            (3, 1, 3),
            "{write} updates its root binding once"
        );
        assert_eq!(b.loop_depth, 0, "{write} stays at depth 0");
    }
}

#[test]
fn audit_dart_write_nested_loop_and_read_control() {
    let src = "void f() {\n var total = 0;\n for (var i = 0; i < 2; i++) {\n for (var j = 0; j < 2; j++) {\n total += j;\n }\n }\n}\n";
    let a = audit_analyze(Lang::Dart, "w.dart", src);
    assert!(a.parse_error.is_none(), "dart nested source parses");
    let b = a
        .facts
        .bindings
        .iter()
        .find(|b| b.name == "total")
        .expect("total binding exists");
    assert_eq!(
        (b.first_write_line, b.write_lines, b.last_write_line),
        (5, 1, 5),
        "nested += advances span to line 5"
    );
    assert_eq!(b.loop_depth, 2, "doubly nested write has loop depth 2");
    let src = "void f() {\n var total = 0;\n print(total);\n}\n";
    let a = audit_analyze(Lang::Dart, "r.dart", src);
    assert!(a.parse_error.is_none(), "dart read-only source parses");
    let b = a
        .facts
        .bindings
        .iter()
        .find(|b| b.name == "total")
        .expect("total binding exists");
    assert_eq!(
        (b.first_write_line, b.write_lines, b.last_write_line),
        (2, 0, 2),
        "read-only call leaves span at decl line 2"
    );
    assert_eq!(b.loop_depth, 0, "read-only call keeps depth 0");
}

#[test]
fn audit_java_reflection_arity() {
    let cases = [
        ("noarg", "Object r = request.getMethod();", 0, ""),
        (
            "comments",
            "Object r = request.getMethod(/* name */);",
            0,
            "",
        ),
        ("constructor", "Object r = cls.getConstructor();", 0, ""),
        (
            "commented literal",
            "Object r = Class.forName(/* type */ \"java.lang.String\");",
            0,
            "",
        ),
        (
            "literal",
            "Object r = Class.forName(\"java.lang.String\");",
            0,
            "",
        ),
        (
            "dynamic",
            "Object a = Class.forName(/* type */ name); Object b = request.getMethod(name);",
            2,
            "reflection by name",
        ),
    ];
    for (label, statement, count, kind) in cases {
        let source = format!("void probe(String name) {{ {statement} }}\n");
        let analysis = audit_analyze(Lang::Java, "reflect.java", &source);
        assert!(analysis.parse_error.is_none(), "{label} source parses");
        let sites = &analysis.facts.dynamic_symbols;
        assert_eq!(sites.len(), count, "{label} dynamic symbol count");
        assert!(
            sites.iter().all(|site| site.what == kind),
            "{label} reports {kind}"
        );
    }
}
