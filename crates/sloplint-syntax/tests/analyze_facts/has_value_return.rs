//! Value-return facts observed through `analyze` only.
//!
//! A return statement carrying a value, an expression body, and any `yield`
//! all mark `has_value_return`; a bare return or a print-only body does not.

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

#[test]
fn a_yield_marks_the_enclosing_function_as_returning() {
    assert!(
        has_value_return(
            Lang::Python,
            "gen.py",
            "def values():\n    yield 1\n",
            "values"
        ),
        "a generator body is a value return"
    );
    assert!(
        has_value_return(
            Lang::Python,
            "bare.py",
            "def values():\n    yield\n",
            "values"
        ),
        "a bare yield still returns a generator"
    );
    assert!(
        has_value_return(
            Lang::Python,
            "from.py",
            "def values():\n    yield from xs\n",
            "values"
        ),
        "yield from still returns a generator"
    );
    assert!(
        has_value_return(
            Lang::TypeScript,
            "gen.ts",
            "export function* values() {\n    yield 1;\n}\n",
            "values"
        ),
        "a TS generator body is a value return"
    );
    assert!(
        has_value_return(
            Lang::TypeScript,
            "bare.ts",
            "function* values() {\n    yield;\n}\n",
            "values"
        ),
        "a bare JS yield still returns a generator"
    );
    assert!(
        has_value_return(
            Lang::Dart,
            "sync.dart",
            "values() sync* {\n  yield 1;\n}\n",
            "values"
        ),
        "a Dart sync* body is a value return"
    );
    assert!(
        has_value_return(
            Lang::Dart,
            "each.dart",
            "values() sync* {\n  yield* xs;\n}\n",
            "values"
        ),
        "a Dart yield* body is a value return"
    );
    assert!(
        has_value_return(
            Lang::CSharp,
            "gen.cs",
            "class C {\n    IEnumerable<int> Values() {\n        yield return 1;\n    }\n}\n",
            "Values"
        ),
        "a C# yield return marks the iterator contract"
    );
    assert!(
        has_value_return(
            Lang::CSharp,
            "stop.cs",
            "class C {\n    IEnumerable<int> Values() {\n        yield break;\n    }\n}\n",
            "Values"
        ),
        "a C# yield break still returns an iterator"
    );
    assert!(
        has_value_return(
            Lang::Php,
            "gen.php",
            "<?php\nfunction values() {\n    yield 1;\n}\n",
            "values"
        ),
        "a PHP yield body is a value return"
    );
}

#[test]
fn generator_near_misses_keep_the_valueless_fact() {
    assert!(
        !has_value_return(
            Lang::Python,
            "log.py",
            "def log(msg):\n    print(msg)\n",
            "log"
        ),
        "a print-only body is no value return"
    );
    assert!(
        !has_value_return(
            Lang::Python,
            "fetch.py",
            "async def fetch():\n    await g()\n",
            "fetch"
        ),
        "an await-only coroutine body is no value return"
    );
    assert!(
        !has_value_return(
            Lang::TypeScript,
            "void.ts",
            "function log(msg: string): void {\n    console.log(msg);\n}\n",
            "log"
        ),
        "a void TS body is no value return"
    );
    assert!(
        !has_value_return(
            Lang::Dart,
            "void.dart",
            "deliver(String address) {\n  print(address);\n}\n",
            "deliver"
        ),
        "a print-only Dart body is no value return"
    );
    assert!(
        !has_value_return(
            Lang::CSharp,
            "void.cs",
            "class C {\n    void Log(string msg) {\n        System.Console.WriteLine(msg);\n    }\n}\n",
            "Log"
        ),
        "a void C# body is no value return"
    );
    assert!(
        !has_value_return(
            Lang::Php,
            "log.php",
            "<?php\nfunction log($msg) {\n    echo $msg;\n}\n",
            "log"
        ),
        "an echo-only PHP body is no value return"
    );
}

#[test]
fn a_yield_inside_a_nested_function_marks_only_the_inner_function() {
    let source = "def outer():\n    def inner():\n        yield 1\n    inner()\n";
    assert!(
        !has_value_return(Lang::Python, "nest.py", source, "outer"),
        "the inner generator's yield does not mark the outer function"
    );
    assert!(
        has_value_return(Lang::Python, "nest.py", source, "inner"),
        "the yield marks the function that contains it"
    );
}
