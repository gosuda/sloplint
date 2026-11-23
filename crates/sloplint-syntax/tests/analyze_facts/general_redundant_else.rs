use sloplint_core::lang::Lang;

use crate::redundant_else::else_lines;

const C_FAMILY: [(Lang, &str); 4] = [
    (Lang::C, "a.c"),
    (Lang::Cpp, "a.cpp"),
    (Lang::Java, "Sample.java"),
    (Lang::CSharp, "Sample.cs"),
];

fn function(lang: Lang, body: &str) -> String {
    match lang {
        Lang::C => format!("void check(int stop) {{ {body} }}\n"),
        Lang::Cpp => format!("void check(bool stop) {{ {body} }}\n"),
        Lang::Java => format!("class Sample {{ void check(boolean stop) {{ {body} }} }}\n"),
        Lang::CSharp => format!("class Sample {{ void Check(bool stop) {{ {body} }} }}\n"),
        _ => panic!("only C-family test languages have a wrapper"),
    }
}

#[test]
fn c_family_records_direct_exits_and_bare_branches() {
    let cases = [
        "if (stop) { return; } else { proceed(); }",
        "while (stop) { if (stop) { break; } else { proceed(); } }",
        "while (stop) { if (stop) { continue; } else { proceed(); } }",
        "if (stop) return; else proceed();",
        "if (stop) { consume(\"é😊\"); return; /* finished */ } else { proceed(); }",
    ];
    for (lang, path) in C_FAMILY {
        for case in cases {
            assert_eq!(
                else_lines(lang, path, &function(lang, case)),
                [1],
                "{path}: {case}"
            );
        }
    }
}

#[test]
fn c_family_keeps_chains_outer_controls_and_conditional_exits() {
    let cases = [
        "if (stop) { if (stop) return; } else proceed();",
        "if (stop) { while (stop) return; } else proceed();",
        "if (stop) { fatal(); } else proceed();",
        "if (stop) return; else if (stop) return; else proceed();",
        "while (stop) if (stop) return; else proceed();",
        "if (stop) { return; } else { }",
        "if (stop) { return; } else { ; }",
        "if (stop) { return; } else { label: proceed(); }",
    ];
    for (lang, path) in C_FAMILY {
        for case in cases {
            assert!(
                else_lines(lang, path, &function(lang, case)).is_empty(),
                "{path} must preserve the branch: {case}"
            );
        }
    }
}

#[test]
fn c_family_preserves_local_declarations_but_keeps_nested_blocks() {
    let scoped = "if (stop) { return; } else { int value = 1; consume(value); }";
    let nested = "if (stop) { return; } else { { int value = 1; consume(value); } }";
    for (lang, path) in C_FAMILY {
        assert!(
            else_lines(lang, path, &function(lang, scoped)).is_empty(),
            "{path}"
        );
        assert_eq!(
            else_lines(lang, path, &function(lang, nested)),
            [1],
            "{path}"
        );
    }
}

#[test]
fn throw_statements_are_exits() {
    for (lang, path, expression) in [
        (Lang::Cpp, "a.cpp", "Failure()"),
        (Lang::Java, "Sample.java", "new IllegalStateException()"),
        (
            Lang::CSharp,
            "Sample.cs",
            "new System.InvalidOperationException()",
        ),
    ] {
        let body = format!("if (stop) {{ throw {expression}; }} else {{ proceed(); }}");
        assert_eq!(
            else_lines(lang, path, &function(lang, &body)),
            [1],
            "{path}"
        );
    }
}

#[test]
fn cpp_keeps_condition_and_cleanup_scopes_and_constexpr_branches() {
    let cases = [
        "if (Guard guard = acquire()) return; else consume(guard);",
        "if (Guard guard = acquire(); stop) return; else consume(guard);",
        "if constexpr (true) return; else proceed();",
        "if (stop) { return; } else { Guard guard; consume(guard); } after_cleanup();",
        "if (stop) { return; } else { using Value = int; consume(Value(1)); }",
    ];
    for case in cases {
        assert!(
            else_lines(Lang::Cpp, "a.cpp", &function(Lang::Cpp, case)).is_empty(),
            "must preserve the scope or compile-time branch: {case}"
        );
    }
    let nested = "if (stop) { return; } else { { Guard guard; consume(guard); } } after_cleanup();";
    assert_eq!(
        else_lines(Lang::Cpp, "a.cpp", &function(Lang::Cpp, nested)),
        [1]
    );
}

#[test]
fn preprocessor_regions_neither_hide_scopes_nor_prove_exits() {
    let cases = [
        "\nif (stop) { return; } else {\n#if FEATURE\nint value = 1;\n#endif\nproceed();\n}\n",
        "\nif (stop) {\n#if FEATURE\nreturn;\n#endif\n} else { proceed(); }\n",
    ];
    for (lang, path) in [C_FAMILY[0], C_FAMILY[1], C_FAMILY[3]] {
        for case in cases {
            assert!(
                else_lines(lang, path, &function(lang, case)).is_empty(),
                "{path} must preserve preprocessor ownership: {case}"
            );
        }
    }
    let chained = "\nif (stop) { return; }\n#if FEATURE\nelse if (stop) { return; }\n#endif\nelse { proceed(); }\n";
    assert!(else_lines(Lang::CSharp, "Sample.cs", &function(Lang::CSharp, chained)).is_empty());
}

#[test]
fn java_constructor_bodies_record_direct_guard_exits() {
    for body in [
        "if (stop) return; else proceed();",
        "super(); if (stop) { return; } else { proceed(); }",
        "if (stop) throw new IllegalArgumentException(); else proceed();",
        "if (stop) return; else { { int value = 1; consume(value); } }",
    ] {
        let source = format!("class Sample {{ Sample(boolean stop) {{ {body} }} }}");
        assert_eq!(
            else_lines(Lang::Java, "Sample.java", &source),
            [1],
            "{body}"
        );
    }
}

#[test]
fn java_constructor_guards_preserve_scopes_and_control_ownership() {
    for body in [
        "if (stop) return; else { int local = 1; consume(local); }",
        "if (!(value instanceof String text)) return; else consume(text);",
        "if (stop) return; else { if (!(value instanceof String text)) return; consume(text); }",
        "if (stop) return; else { class Local {} proceed(); }",
        "while (stop) if (stop) return; else proceed();",
        "if (stop) { if (stop) return; } else proceed();",
        "if (stop) return; else if (stop) return; else proceed();",
        "if (stop) return; else { label: proceed(); }",
    ] {
        let source = format!("class Sample {{ Sample(boolean stop, Object value) {{ {body} }} }}");
        assert!(
            else_lines(Lang::Java, "Sample.java", &source).is_empty(),
            "must preserve the constructor branch: {body}"
        );
    }
}

#[test]
fn java_keeps_pattern_bindings_but_accepts_unbound_type_checks() {
    let cases = [
        "if (!(value instanceof String text)) return; else consume(text);",
        "if (stop) { return; } else { if (!(value instanceof String text)) return; consume(text); }",
        "if (stop) { return; } else { class Local {} proceed(); }",
    ];
    for case in cases {
        assert!(
            else_lines(Lang::Java, "Sample.java", &function(Lang::Java, case)).is_empty(),
            "must preserve the binding: {case}"
        );
    }
    let unbound = "if (value instanceof String) return; else proceed();";
    assert_eq!(
        else_lines(Lang::Java, "Sample.java", &function(Lang::Java, unbound)),
        [1]
    );
}

#[test]
fn csharp_keeps_out_vars_patterns_resources_and_local_functions() {
    let cases = [
        "if (value is string text) return; else proceed();",
        "if (TryRead(out var value)) return; else consume(value);",
        "if (stop) { return; } else { TryRead(out var value); consume(value); }",
        "if (stop) { return; } else { void Local() { proceed(); } Local(); }",
        "if (stop) { return; } else { using var resource = acquire(); consume(resource); }",
        "if (stop) { return; } else { if (value is string text) consume(text); }",
    ];
    for case in cases {
        assert!(
            else_lines(Lang::CSharp, "Sample.cs", &function(Lang::CSharp, case)).is_empty(),
            "must preserve the binding or lifetime: {case}"
        );
    }
    let null_guard = "if (value is null) return; else proceed();";
    assert_eq!(
        else_lines(
            Lang::CSharp,
            "Sample.cs",
            &function(Lang::CSharp, null_guard)
        ),
        [1]
    );
}

#[test]
fn rust_records_only_discarded_statements() {
    let sources = [
        "fn check(stop: bool) { if stop { return; } else { work(); } finish(); }",
        "fn check(stop: bool) { if stop { return; } else { work(); }; }",
        "fn check(stop: bool) { loop { if stop { break; } else { work(); }; } }",
        "fn check(stop: bool) { loop { if stop { continue; } else { work(); }; } }",
    ];
    for source in sources {
        assert_eq!(else_lines(Lang::Rust, "a.rs", source), [1], "{source}");
    }
}

#[test]
fn rust_preserves_values_bindings_temporary_scopes_and_attributes() {
    let sources = [
        "fn choose(stop: bool) -> u32 { if stop { return 0; } else { 1 } }",
        "fn choose(stop: bool) -> u32 { if stop { return 0; } else { 1 }; 2 }",
        "fn check(stop: bool) { if stop { return; } else { work(); } }",
        "fn check(stop: bool) { let value = if stop { return; } else { work(); }; consume(value); }",
        "fn check() { if acquire().ready() { return; } else { work(); }; }",
        "fn check(stop: bool) { if !stop { return; } else { work(); }; }",
        "fn check(value: Option<u32>) { if let Some(found) = value { return; } else { work(); }; }",
        "fn check(value: Option<u32>, stop: bool) { if let Some(found) = value && stop { return; } else { work(); }; }",
        "fn check(stop: bool) { if stop { return; } else { let guard = acquire(); consume(&guard); }; }",
        "fn check(stop: bool) { if stop { return; } else { scoped_work!(); }; }",
        "fn check(stop: bool) { if stop { return; } else { fn local() {} local(); }; }",
        "fn check(stop: bool) { #[cfg(any())] if stop { return; } else { work(); } finish(); }",
        "fn check(stop: bool) { if stop { #[cfg(any())] return; } else { work(); } finish(); }",
        "fn check(stop: bool) { if stop { panic!(); } else { work(); } finish(); }",
        "fn check(stop: bool) { if stop { if stop { return; } } else { work(); } finish(); }",
    ];
    for source in sources {
        assert!(
            else_lines(Lang::Rust, "a.rs", source).is_empty(),
            "must preserve the expression or scope: {source}"
        );
    }
}
