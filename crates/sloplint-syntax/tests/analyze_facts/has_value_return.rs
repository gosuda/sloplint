//! Value-return facts observed through `analyze` only.
//!
//! A return statement carrying a value and an expression body both mark
//! `has_value_return`; a bare return or a print-only body does not.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn has_value_return(lang: Lang, path: &str, source: &str, name: &str) -> bool {
    run_clean(lang, path, source)
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{path} has function {name}"))
        .has_value_return
}

#[test]
fn return_values_and_expression_bodies_mark_the_fact() {
    assert!(
        !has_value_return(
            Lang::Python,
            "main.py",
            "def main():\n    print(1)\n",
            "main"
        ),
        "a print-only body is no value return"
    );
    assert!(
        has_value_return(Lang::Python, "f.py", "def f():\n    return 1\n", "f"),
        "return 1 is a value return"
    );
    assert!(
        !has_value_return(Lang::Python, "bare.py", "def bare():\n    return\n", "bare"),
        "a bare return is no value return"
    );
    assert!(
        has_value_return(Lang::TypeScript, "arrow.ts", "const f = () => 1;\n", "f"),
        "an arrow expression body is a value return"
    );
    assert!(
        !has_value_return(
            Lang::TypeScript,
            "void_fn.ts",
            "function g(): void {\n    return;\n}\n",
            "g"
        ),
        "a void bare return is no value return"
    );
    assert!(
        has_value_return(Lang::Kotlin, "expr.kt", "fun f() = 1\n", "f"),
        "a Kotlin `= expr` body is a value return"
    );
    assert!(
        !has_value_return(Lang::Kotlin, "block.kt", "fun g() {\n    return\n}\n", "g"),
        "a Kotlin block with a bare return is no value return"
    );
    assert!(
        has_value_return(Lang::Scala, "expr.scala", "def f = 1\n", "f"),
        "a Scala `= expr` body is a value return"
    );
    assert!(
        has_value_return(Lang::Dart, "expr.dart", "int f() => 1;\n", "f"),
        "a Dart `=> expr` body is a value return"
    );
    assert!(
        has_value_return(
            Lang::Php,
            "expr.php",
            "<?php\nfunction f(): int { return 1; }\n",
            "f"
        ),
        "a PHP return 1 is a value return"
    );
    assert!(
        has_value_return(Lang::JavaScript, "expr.js", "const f = () => 1;\n", "f"),
        "a JS arrow expression body is a value return"
    );
}

#[test]
fn return_inside_a_frameless_closure_marks_no_value() {
    assert!(
        !has_value_return(
            Lang::Dart,
            "loop.dart",
            "f() {\n  [1].forEach((x) { return x; });\n}\n",
            "f"
        ),
        "a return inside a frameless closure is no value return for the outer function"
    );
    assert!(
        !has_value_return(
            Lang::Php,
            "map.php",
            "<?php\nfunction f() { array_map(function ($x) { return $x; }, []); }\n",
            "f"
        ),
        "a return inside an anonymous function is no value return for the outer function"
    );
    assert!(
        has_value_return(Lang::Dart, "plain.dart", "f() { return 1; }\n", "f"),
        "a plain return 1 is a value return"
    );
}
