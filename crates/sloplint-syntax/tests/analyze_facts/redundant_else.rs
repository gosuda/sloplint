use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn else_lines(lang: Lang, path: &str, source: &str) -> Vec<u32> {
    let analysis = run_clean(lang, path, source);
    analysis
        .facts
        .redundant_elses
        .iter()
        .map(|pos| {
            assert!(
                source[pos.byte as usize..].starts_with("else"),
                "{path}: diagnostic must point at the else keyword: {pos:?}"
            );
            pos.line
        })
        .collect()
}

#[test]
fn go_records_direct_exits_and_keyword_positions() {
    let sources = [
        "package p\nfunc stop(ready bool) { if ready { return } else { proceed() } }\n",
        "package p\nfunc stop(ready bool) { for { if ready { break } else { proceed() } } }\n",
        "package p\nfunc stop(ready bool) { for { if ready { continue } else { proceed() } } }\n",
    ];
    for source in sources {
        assert_eq!(else_lines(Lang::Go, "a.go", source), [2], "{source}");
    }

    let source = "package p\nfunc stop(ready bool) {\n    if ready {\n        println(\"é😊\")\n        return\n        // Completion needs no further work.\n    } else {\n        proceed()\n    }\n}\n";
    assert_eq!(else_lines(Lang::Go, "a.go", source), [7]);
}

#[test]
fn go_keeps_scoped_branches_chains_and_named_calls() {
    let cases = [
        "if value := read(); value == 0 { return } else { consume(value) }",
        "if ready { return } else if other { return } else { proceed() }",
        "if ready { return } else { value := read(); consume(value) }",
        "if ready { return } else { var value = read(); consume(value) }",
        "if ready { return } else { const value = 1; consume(value) }",
        "if ready { return } else { type Value int; consume(Value(1)) }",
        "if ready { panic(\"recoverable\") } else { proceed() }",
        "if ready { fatal() } else { proceed() }",
        "if ready { if other { return } } else { proceed() }",
        "if ready { for other { return } } else { proceed() }",
        "if ready { func() { return }() } else { proceed() }",
        "if ready { return } else { }",
        "if ready { return } else { ; }",
        "again: if ready { return } else { proceed() }; goto again",
        "if ready { goto done } else { proceed() }; done: return",
    ];
    for case in cases {
        let source = format!("package p\nfunc stop(ready, other bool) {{ {case} }}\n");
        assert!(
            else_lines(Lang::Go, "a.go", &source).is_empty(),
            "must preserve the branch: {case}"
        );
    }
}

#[test]
fn go_preserves_nested_block_scopes() {
    let source = "package p\nfunc stop(ready bool) { if ready { return } else { { value := read(); consume(value) } } }\n";
    assert_eq!(else_lines(Lang::Go, "a.go", source), [2]);
}

#[test]
fn python_records_direct_exits_without_descending_into_statements() {
    let source = r#"def stop(ready, values):
    if ready:
        return
        # Completion needs no further work.
    else:
        proceed()
    if ready:
        raise ValueError("invalid state")
    else:
        proceed()
    for value in values:
        if value < 0:
            continue
        else:
            consume(value)
        if value == 0:
            break
        else:
            consume(value)
"#;
    assert_eq!(else_lines(Lang::Python, "a.py", source), [5, 9, 14, 18]);
}

#[test]
fn python_keeps_decision_chains_and_conditional_exits() {
    let source = r#"def stop(ready, other, values):
    if ready:
        return
    elif other:
        return
    else:
        proceed()
    if ready:
        if other:
            return
    else:
        proceed()
    if ready:
        for value in values:
            return value
    else:
        proceed()
    if ready:
        def deferred():
            return
    else:
        proceed()
    if ready:
        fatal("recoverable")
    else:
        proceed()
    try:
        work()
    except ValueError:
        raise
    else:
        proceed()
"#;
    assert!(else_lines(Lang::Python, "a.py", source).is_empty());
}

const WEB_LANGUAGES: [(Lang, &str); 3] = [
    (Lang::JavaScript, "a.js"),
    (Lang::TypeScript, "a.ts"),
    (Lang::Tsx, "a.tsx"),
];

#[test]
fn web_records_direct_exits_and_bare_branches() {
    let source = r#"function stop(ready, values) {
    if (ready) {
        consume("é😊");
        return;
        // Completion needs no further work.
    } else {
        proceed();
    }
    if (ready) throw new Error("invalid state");
    else proceed();
    for (const value of values) {
        if (value < 0) {
            continue;
        } else {
            consume(value);
        }
        if (value === 0) {
            break;
        } else {
            consume(value);
        }
    }
}
"#;
    for (lang, path) in WEB_LANGUAGES {
        assert_eq!(else_lines(lang, path, source), [6, 10, 14, 19], "{path}");
    }
}

#[test]
fn web_keeps_scope_lifetimes_and_outer_control_boundaries() {
    let cases = [
        "if (ready) { return; } else { let value = read(); consume(value); }",
        "if (ready) { return; } else { const value = read(); consume(value); }",
        "if (ready) { return; } else { var value = read(); consume(value); }",
        "if (ready) { return; } else { using value = acquire(); consume(value); }",
        "if (ready) { return; } else { class Value {} consume(Value); }",
        "if (ready) { return; } else { function value() {} consume(value); }",
        "if (ready) { return; } else if (other) { return; } else { proceed(); }",
        "if (ready) { if (other) return; } else { proceed(); }",
        "if (ready) { while (other) return; } else { proceed(); }",
        "if (ready) { (() => { return; })(); } else { proceed(); }",
        "if (ready) { fatal('recoverable'); } else { proceed(); }",
        "if (outer) if (ready) return; else proceed();",
        "while (outer) if (ready) return; else proceed();",
        "again: if (ready) return; else proceed();",
        "if (ready) { return; } else { }",
        "if (ready) { return; } else { ; }",
    ];
    for (lang, path) in WEB_LANGUAGES {
        for case in cases {
            let source = format!("function stop(ready, other, outer) {{ {case} }}\n");
            assert!(
                else_lines(lang, path, &source).is_empty(),
                "{path} must preserve the branch: {case}"
            );
        }
    }
}

#[test]
fn web_preserves_nested_block_scopes() {
    let source = "function stop(ready) { if (ready) { return; } else { { let value = read(); consume(value); } } }\n";
    for (lang, path) in WEB_LANGUAGES {
        assert_eq!(else_lines(lang, path, source), [1], "{path}");
    }
}

#[test]
fn typescript_keeps_branch_local_type_declarations() {
    let cases = [
        "type Value = number; consume(1);",
        "interface Value { size: number } consume(1);",
        "enum Value { Ready } consume(Value.Ready);",
    ];
    for (lang, path) in &WEB_LANGUAGES[1..] {
        for case in cases {
            let source = format!(
                "function stop(ready: boolean): void {{ if (ready) {{ return; }} else {{ {case} }} }}\n"
            );
            assert!(
                else_lines(*lang, path, &source).is_empty(),
                "{path} must retain the declaration scope: {case}"
            );
        }
    }
}

#[test]
fn unsupported_expression_and_go_like_languages_stay_silent() {
    let rust =
        "fn choose(stop: bool) -> u32 { let value = if stop { return 0; } else { 1 }; value }\n";
    assert!(else_lines(Lang::Rust, "a.rs", rust).is_empty());
    let logos = "package p\nfunc stop(ready bool) { if ready { return } else { proceed() } }\n";
    assert!(else_lines(Lang::Logos, "a.lgo", logos).is_empty());
}
