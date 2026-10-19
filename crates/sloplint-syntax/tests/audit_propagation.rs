//! Audit-error propagation contracts observed through `analyze` only.
//!
//! Moved verbatim out of the `audit_propagation_tests` unit module in
//! `crates/sloplint-syntax/src/lib.rs` so that file stays under the
//! file-token budget; every case runs against the crate's public surface.

use std::sync::LazyLock;

use sloplint_core::facts::Stmt;
use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static EMPTY_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn audit_analyze<'s>(lang: Lang, path: &'s str, source: &'s str) -> Analysis<'s> {
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut parsers, &input, &EMPTY_CUSTOM)
}

#[test]
fn audit_propagation_error_bubble_emits_no_handler() {
    let cases = [
        (
            Lang::Zig,
            "propagate.zig",
            "fn load() !void { try work(); }\n",
        ),
        (
            Lang::Odin,
            "propagate.odin",
            "package sample\nload :: proc() -> (err: Error) { value := work() or_return; return; }\n",
        ),
    ];
    for (lang, path, source) in cases {
        let analysis = audit_analyze(lang, path, source);
        assert!(analysis.parse_error.is_none(), "{path} parses");
        assert!(
            analysis.facts.handlers.is_empty(),
            "{path} emits no handler"
        );
        let function = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == "load")
            .expect("load function");
        assert_eq!(function.cognitive, 0, "{path} adds no cognitive");
        assert!(
            analysis
                .facts
                .calls
                .iter()
                .any(|c| c.callee.contains("work")),
            "{path} still traverses the propagated call"
        );
    }
}

#[test]
fn audit_propagation_zig_catch_still_emits_handler() {
    let source = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() void {\n    read_value() catch {};\n}\n";
    let analysis = audit_analyze(Lang::Zig, "handler.zig", source);
    assert!(analysis.parse_error.is_none(), "empty catch source parses");
    assert_eq!(analysis.facts.handlers.len(), 1, "empty catch is a handler");
    assert!(
        analysis.facts.handlers.iter().all(|h| h.stmts.is_empty()),
        "empty catch block stays a swallow handler"
    );
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .any(|c| c.callee.contains("read_value")),
        "catch scrutinee still traverses calls"
    );
}

#[test]
fn audit_propagation_zig_catch_retains_body_calls() {
    let body_call = "fn read_value() !u32 {\n    return 1;\n}\n\nfn log_fallback() void {}\n\npub fn load() void {\n    read_value() catch {\n        log_fallback();\n    };\n}\n";
    let analysis = audit_analyze(Lang::Zig, "body_call.zig", body_call);
    assert!(
        analysis.parse_error.is_none(),
        "catch with body call parses"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "catch with body call is a handler"
    );
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .any(|c| c.callee.contains("log_fallback")),
        "catch body still traverses calls"
    );
}

#[test]
fn audit_zig_handler_rethrow_returns_bound_identifier() {
    let source = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() !u32 {\n    return read_value() catch |e| {\n        return e;\n    };\n}\n";
    let analysis = audit_analyze(Lang::Zig, "rethrow.zig", source);
    assert!(
        analysis.parse_error.is_none(),
        "rethrow catch source parses without error"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "rethrow catch emits exactly one handler"
    );
    let stmts = &analysis.facts.handlers[0].stmts;
    assert_eq!(
        stmts.len(),
        1,
        "rethrow catch body keeps its single return statement"
    );
    assert!(
        matches!(&stmts[0], Stmt::Rethrow),
        "return of the captured identifier classifies as Rethrow, got {:?}",
        stmts[0]
    );
}

#[test]
fn audit_zig_handler_recovery_value_is_other() {
    let source = "fn read_value() !u32 {\n    return 1;\n}\n\nfn recover_value() u32 {\n    return 0;\n}\n\npub fn load() u32 {\n    return read_value() catch {\n        return recover_value();\n    };\n}\n";
    let analysis = audit_analyze(Lang::Zig, "handle.zig", source);
    assert!(
        analysis.parse_error.is_none(),
        "recovery catch source parses without error"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "recovery catch emits exactly one handler"
    );
    let stmts = &analysis.facts.handlers[0].stmts;
    assert_eq!(
        stmts.len(),
        1,
        "recovery catch body keeps its single return statement"
    );
    assert!(
        matches!(&stmts[0], Stmt::Other),
        "return of a recovery value classifies as Other, got {:?}",
        stmts[0]
    );
}

#[test]
fn audit_zig_handler_body_call_is_call() {
    let source = "fn read_value() !u32 {\n    return 1;\n}\n\nfn log_fallback() void {}\n\npub fn load() void {\n    read_value() catch {\n        log_fallback();\n    };\n}\n";
    let analysis = audit_analyze(Lang::Zig, "handler_call.zig", source);
    assert!(
        analysis.parse_error.is_none(),
        "catch with body call parses without error"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "catch with body call emits exactly one handler"
    );
    let stmts = &analysis.facts.handlers[0].stmts;
    assert_eq!(
        stmts.len(),
        1,
        "catch body call keeps its single call statement"
    );
    assert!(
        matches!(&stmts[0], Stmt::Call { callee, .. } if callee.contains("log_fallback")),
        "catch body call retains its callee, got {:?}",
        stmts[0]
    );
}

#[test]
fn audit_zig_handler_empty_and_comment_only_stay_empty() {
    let empty = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() void {\n    read_value() catch {};\n}\n";
    let analysis = audit_analyze(Lang::Zig, "empty.zig", empty);
    assert!(
        analysis.parse_error.is_none(),
        "empty catch source parses without error"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "empty catch emits exactly one handler"
    );
    assert!(
        analysis.facts.handlers[0].stmts.is_empty(),
        "empty catch block stays empty, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let commented = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() void {\n    read_value() catch {\n        // nothing to do\n    };\n}\n";
    let analysis = audit_analyze(Lang::Zig, "comment_only.zig", commented);
    assert!(
        analysis.parse_error.is_none(),
        "comment-only catch source parses without error"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "comment-only catch emits exactly one handler"
    );
    assert!(
        analysis.facts.handlers[0].stmts.is_empty(),
        "comment-only catch block stays empty, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

#[test]
fn audit_missing_handler_rust() {
    let rethrow = "fn load(p: &str) -> Result<String, std::io::Error> { match std::fs::read_to_string(p) { Ok(s) => Ok(s), Err(e) => { return Err(e); } } }\n";
    let analysis = audit_analyze(Lang::Rust, "rethrow.rs", rethrow);
    assert!(analysis.parse_error.is_none(), "rethrow Err(e) parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "Err(e) with return Err(e) is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "return Err(e) classifies as Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let empty = "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Ok(s) => { return Ok(s); }, Err(_) => {} } }\n";
    let analysis = audit_analyze(Lang::Rust, "empty.rs", empty);
    assert!(analysis.parse_error.is_none(), "empty Err(_) parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "unguarded Err(_) is a handler"
    );
    assert!(
        analysis.facts.handlers[0].stmts.is_empty(),
        "empty Err(_) body stays empty, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let if_let = "fn load() -> Result<String, std::io::Error> { if let Err(e) = std::fs::read_to_string(\"p\") { return Err(e); } Ok(String::new()) }\n";
    let analysis = audit_analyze(Lang::Rust, "if_let.rs", if_let);
    assert!(analysis.parse_error.is_none(), "if let Err(e) parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "if let Err(e) is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "if let return Err(e) is Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let recovery = "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Ok(s) => { return Ok(s); }, Err(e) => { return Ok(String::new()); } } }\n";
    let analysis = audit_analyze(Lang::Rust, "recovery.rs", recovery);
    assert!(analysis.parse_error.is_none(), "recovery Err(e) parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "recovery Err(e) is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Other]),
        "recovery return stays Other, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let no_handler = [
        (
            "guarded",
            "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Err(e) if true => { return Err(e); }, _ => { return Ok(String::new()); } } }\n",
        ),
        (
            "unrelated",
            "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Ok(s) => { return Ok(s); }, _ => { return Ok(String::new()); } } }\n",
        ),
    ];
    for (name, source) in no_handler {
        let analysis = audit_analyze(Lang::Rust, "no_handler.rs", source);
        assert!(analysis.parse_error.is_none(), "{name} parses");
        assert!(
            analysis.facts.handlers.is_empty(),
            "{name} emits no unconditional handler"
        );
    }
}

#[test]
fn audit_selective_handler_recovery_suppresses_fallback_rethrow() {
    let source = "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Ok(s) => Ok(s), Err(e) if e.to_string().is_empty() => Ok(String::new()), Err(e) => return Err(e), } }\n";
    let analysis = audit_analyze(Lang::Rust, "selective.rs", source);
    assert!(
        analysis.parse_error.is_none(),
        "selective recovery source parses"
    );
    assert!(
        analysis.facts.handlers.is_empty(),
        "fallback rethrow after a guarded Err recovery is not a redundant handler, got {:?}",
        analysis
            .facts
            .handlers
            .iter()
            .map(|h| &h.stmts)
            .collect::<Vec<_>>()
    );
}

#[test]
fn audit_selective_handler_plain_ok_err_still_rethrows() {
    let source = "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Ok(s) => Ok(s), Err(e) => return Err(e), } }\n";
    let analysis = audit_analyze(Lang::Rust, "plain.rs", source);
    assert!(analysis.parse_error.is_none(), "plain Ok/Err source parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "lone unguarded Err rethrow stays a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "lone fallback return Err(e) classifies as Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

#[test]
fn audit_selective_handler_order_and_guard_cases() {
    let fallback_first = "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Err(e) => return Err(e), Err(e) if e.to_string().is_empty() => Ok(String::new()), } }\n";
    let analysis = audit_analyze(Lang::Rust, "order.rs", fallback_first);
    assert!(
        analysis.parse_error.is_none(),
        "fallback-first source parses"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "a leading unguarded Err rethrow still fires when the selective arm comes after it"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "leading fallback return Err(e) classifies as Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let guarded_ok = "fn load(x: Result<String, std::io::Error>) -> Result<String, std::io::Error> { match x { Ok(s) if s.is_empty() => Ok(String::new()), Err(e) => return Err(e), } }\n";
    let analysis = audit_analyze(Lang::Rust, "guarded_ok.rs", guarded_ok);
    assert!(analysis.parse_error.is_none(), "guarded Ok source parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "a guarded Ok arm never counts as selective error handling"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "fallback after guarded Ok classifies as Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let guarded_wildcard = "fn load(x: Result<String, std::io::Error>, retry: bool) -> Result<String, std::io::Error> { match x { _ if retry => Ok(String::new()), Err(e) => return Err(e), } }\n";
    let analysis = audit_analyze(Lang::Rust, "guarded_wildcard.rs", guarded_wildcard);
    assert!(
        analysis.parse_error.is_none(),
        "guarded wildcard source parses"
    );
    assert!(
        analysis.facts.handlers.is_empty(),
        "fallback rethrow after a guarded wildcard recovery is not redundant, got {:?}",
        analysis
            .facts
            .handlers
            .iter()
            .map(|h| &h.stmts)
            .collect::<Vec<_>>()
    );
}

#[test]
fn audit_selective_handler_alternative_patterns_preserve_recovery() {
    for pattern in ["Err(1) | Err(2)", "Ok(_) | _"] {
        let source = format!(
            "fn load(x: Result<u32, u32>, retry: bool) -> Result<u32, u32> {{ match x {{ {pattern} if retry => Ok(0), Ok(v) => Ok(v), Err(e) => return Err(e), }} }}"
        );
        let analysis = audit_analyze(Lang::Rust, "alternatives.rs", &source);
        assert!(analysis.parse_error.is_none(), "{pattern} parses");
        assert!(
            analysis.facts.handlers.is_empty(),
            "{pattern} handles errors before the fallback rethrow"
        );
    }
}

#[test]
fn audit_missing_handler_go() {
    let empties = [
        (
            "empty",
            "package sample\nfunc swallow() { if err := g(); err != nil { } }\n",
        ),
        (
            "comment-only",
            "package sample\nfunc swallow() { if err := g(); err != nil { // nothing\n } }\n",
        ),
    ];
    for (name, source) in empties {
        let analysis = audit_analyze(Lang::Go, "empty.go", source);
        assert!(analysis.parse_error.is_none(), "go {name} parses");
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "go {name} err check is a handler"
        );
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "go {name} body stays empty, got {:?}",
            analysis.facts.handlers[0].stmts
        );
    }
    let rethrow =
        "package sample\nfunc load() error { if err := g(); err != nil { return err } }\n";
    let analysis = audit_analyze(Lang::Go, "rethrow.go", rethrow);
    assert!(analysis.parse_error.is_none(), "go rethrow parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "go return err is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "go return err is Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let recovery =
        "package sample\nfunc load() error { if err := g(); err != nil { return nil } }\n";
    let analysis = audit_analyze(Lang::Go, "recovery.go", recovery);
    assert!(analysis.parse_error.is_none(), "go recovery parses");
    assert_eq!(analysis.facts.handlers.len(), 1, "go recovery is a handler");
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Other]),
        "go return nil stays Other, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let analysis = audit_analyze(
        Lang::Go,
        "nonerror.go",
        "package sample\nfunc f(x int) int { if x == 1 { return 1 }\n return 0 }\n",
    );
    assert!(analysis.parse_error.is_none(), "go nonerror parses");
    assert!(
        analysis.facts.handlers.is_empty(),
        "go non-nil check emits no handler"
    );
}

#[test]
fn audit_missing_handler_ruby() {
    let empties = [
        (
            "empty",
            "def read_config(path)\n File.read(path)\nrescue Errno::ENOENT\nend\n",
        ),
        (
            "comment-only",
            "def read_config(path)\n File.read(path)\nrescue Errno::ENOENT\n # missing is fine\nend\n",
        ),
    ];
    for (name, source) in empties {
        let analysis = audit_analyze(Lang::Ruby, "empty.rb", source);
        assert!(analysis.parse_error.is_none(), "ruby {name} rescue parses");
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "ruby {name} rescue is a handler"
        );
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "ruby {name} rescue stays empty, got {:?}",
            analysis.facts.handlers[0].stmts
        );
    }
    let analysis = audit_analyze(
        Lang::Ruby,
        "rethrow.rb",
        "def f()\n g()\nrescue => e\n raise e\nend\n",
    );
    assert!(analysis.parse_error.is_none(), "ruby rethrow parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "ruby raise e is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "ruby raise e is Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let analysis = audit_analyze(
        Lang::Ruby,
        "recovery.rb",
        "def f()\n g()\nrescue\n return 0\nend\n",
    );
    assert!(analysis.parse_error.is_none(), "ruby recovery parses");
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "ruby recovery is a handler"
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Other]),
        "ruby recovery stays Other, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let analysis = audit_analyze(Lang::Ruby, "modifier.rb", "a = b() rescue c()\n");
    assert!(
        analysis.parse_error.is_none(),
        "ruby rescue modifier parses"
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "ruby rescue modifier still emits a handler"
    );
}

#[test]
fn audit_missing_handler_typescript() {
    let empties = [
        (
            "empty",
            "export function f() { try { g(); } catch (e) {} }\n",
        ),
        (
            "comment-only",
            "export function readConfig(p: string) { try { return fs.readFileSync(p, \"utf8\"); } catch (e) {\n // missing config is fine\n } }\n",
        ),
    ];
    for (name, source) in empties {
        let analysis = audit_analyze(Lang::TypeScript, "empty.ts", source);
        assert!(analysis.parse_error.is_none(), "ts {name} catch parses");
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "ts {name} catch is a handler"
        );
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "ts {name} catch stays empty, got {:?}",
            analysis.facts.handlers[0].stmts
        );
    }
    let analysis = audit_analyze(
        Lang::TypeScript,
        "rethrow.ts",
        "export function f() { try { g(); } catch (e) { throw e; } }\n",
    );
    assert!(analysis.parse_error.is_none(), "ts rethrow parses");
    assert_eq!(analysis.facts.handlers.len(), 1, "ts throw e is a handler");
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Rethrow]),
        "ts throw e is Rethrow, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let analysis = audit_analyze(
        Lang::TypeScript,
        "recovery.ts",
        "export function f() { try { g(); } catch (e) { return 0; } }\n",
    );
    assert!(analysis.parse_error.is_none(), "ts recovery parses");
    assert_eq!(analysis.facts.handlers.len(), 1, "ts recovery is a handler");
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Other]),
        "ts recovery stays Other, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

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
        assert_eq!(b.last_write_line, 3, "{write} updates its root binding");
        assert_eq!(b.max_nested_depth, 0, "{write} stays at depth 0");
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
        assert_eq!(b.last_write_line, 3, "{write} updates its root binding");
        assert_eq!(b.max_nested_depth, 0, "{write} stays at depth 0");
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
    assert_eq!(b.last_write_line, 5, "nested += advances span to line 5");
    assert_eq!(b.max_nested_depth, 2, "doubly nested write has depth 2");
    assert!(
        b.nested_write_pos.is_some_and(|p| p.line == 5),
        "nested write pos is line 5"
    );
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
        b.last_write_line, 2,
        "read-only call leaves span at decl line 2"
    );
    assert_eq!(b.max_nested_depth, 0, "read-only call keeps depth 0");
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

#[test]
fn audit_local_function_visibility() {
    let nested = [
        (
            Lang::Kotlin,
            "a.kt",
            "fun total(xs: List<Int>): Int {\n fun twice(x: Int) = x * 2\n return xs.sumOf { twice(it) }\n}\n",
        ),
        (
            Lang::Scala,
            "A.scala",
            "object Calc {\n def total(xs: List[Int]): Int = {\n def twice(x: Int) = x * 2\n xs.map(twice).sum\n }\n}\n",
        ),
    ];
    for (lang, path, source) in nested {
        let a = audit_analyze(lang, path, source);
        assert!(
            a.parse_error.is_none(),
            "{path} local-function repro parses"
        );
        let total = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == "total")
            .unwrap_or_else(|| panic!("{path} has total"));
        assert!(total.public, "{path} total stays public");
        let twice = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == "twice")
            .unwrap_or_else(|| panic!("{path} has twice"));
        assert!(!twice.public, "{path} local twice is not public");
    }
    let members = [
        (
            Lang::Kotlin,
            "C.kt",
            "class C {\n fun total(): Int = 1\n}\n",
            "total",
            true,
        ),
        (
            Lang::Kotlin,
            "P.kt",
            "class C {\n private fun twice(): Int = 1\n}\n",
            "twice",
            false,
        ),
        (
            Lang::Scala,
            "C.scala",
            "object Calc {\n def total(): Int = 1\n}\n",
            "total",
            true,
        ),
        (
            Lang::Scala,
            "P.scala",
            "class C {\n private def twice(): Int = 1\n}\n",
            "twice",
            false,
        ),
    ];
    for (lang, path, source, name, want) in members {
        let a = audit_analyze(lang, path, source);
        assert!(a.parse_error.is_none(), "{path} member parses");
        let f = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{path} has {name}"));
        assert_eq!(f.public, want, "{path} {name} public is {want}");
    }
    let block_locals = [
        (
            Lang::Kotlin,
            "I.kt",
            "class C {\n init {\n fun twice(x: Int) = x * 2\n }\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "V.scala",
            "object C {\n val k = {\n def twice(x: Int) = x * 2\n twice(1)\n }\n}\n",
            "twice",
        ),
        (
            Lang::Kotlin,
            "L.kt",
            "fun total(): Int {\n val f = { fun twice(x: Int) = x * 2\n twice(1) }\n return 1\n}\n",
            "twice",
        ),
        (
            Lang::Kotlin,
            "N.kt",
            "fun total(): Int {\n class Local {\n fun twice(): Int = 1\n }\n return 1\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "W.scala",
            "object C {\n val f = (x: Int) => {\n def twice(y: Int) = y * 2\n twice(x)\n }\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "F.scala",
            "object C {\n def total(): Int = {\n class Local {\n def twice(): Int = 1\n }\n 1\n }\n}\n",
            "twice",
        ),
        (
            Lang::Kotlin,
            "O.kt",
            "fun total(): Int {\n object Local {\n fun twice(): Int = 1\n }\n return 1\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "G.scala",
            "object C {\n def total(): Int = {\n object Local {\n def twice(): Int = 1\n }\n 1\n }\n}\n",
            "twice",
        ),
    ];
    for (lang, path, source, name) in block_locals {
        let a = audit_analyze(lang, path, source);
        assert!(a.parse_error.is_none(), "{path} block-local repro parses");
        let f = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{path} has {name}"));
        assert!(!f.public, "{path} block-local {name} is not public");
    }
}
