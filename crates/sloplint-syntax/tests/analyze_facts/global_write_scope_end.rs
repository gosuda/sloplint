//! Statement-scoped bindings: writes after the binding scope ends reach the file-scope mutable.

use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn in_function_writes(lang: Lang, path: &str, source: &str) -> usize {
    run_clean(lang, path, source)
        .facts
        .ambient
        .iter()
        .filter(|site| site.kind == AmbientKind::GlobalWrite && site.in_function.is_some())
        .count()
}

#[test]
fn go_type_switch_alias_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "switch_scope.go",
            "package main\n\nvar counter int\n\nfunc f(v any) {\nswitch counter := v.(type) {\ncase int:\ncounter = 1\n}\ncounter = 5\n}\n",
        ),
        1,
        "switch_scope.go: a write after the type switch reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "switch_scope.go",
            "package main\n\nvar counter int\n\nfunc f(v any) {\nswitch counter := v.(type) {\ncase int:\ncounter = 1\n}\n}\n",
        ),
        0,
        "switch_scope.go: a write inside the type switch stays local to the alias"
    );
}

#[test]
fn csharp_foreach_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::CSharp,
            "foreach_scope.cs",
            "class A {\nstatic int x = 0;\nvoid f(int[] xs) {\nforeach (var x in xs) { }\nx = 1;\n}\n}\n",
        ),
        1,
        "foreach_scope.cs: a write after the loop reaches the static field"
    );
    assert_eq!(
        in_function_writes(
            Lang::CSharp,
            "foreach_scope.cs",
            "class A {\nstatic int x = 0;\nvoid f(int[] xs) {\nforeach (var x in xs) {\nx = 1;\n}\n}\n}\n",
        ),
        0,
        "foreach_scope.cs: a write inside the loop stays local to the iteration variable"
    );
}

#[test]
fn java_enhanced_for_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Java,
            "ForScope.java",
            "class A {\nstatic int x = 0;\nvoid f(int[] xs) {\nfor (int x : xs) { }\nx = 1;\n}\n}\n",
        ),
        1,
        "ForScope.java: a write after the loop reaches the static field"
    );
    assert_eq!(
        in_function_writes(
            Lang::Java,
            "ForScope.java",
            "class A {\nstatic int x = 0;\nvoid f(int[] xs) {\nfor (int x : xs) {\nx = 1;\n}\n}\n}\n",
        ),
        0,
        "ForScope.java: a write inside the loop stays local to the iteration variable"
    );
}

#[test]
fn js_for_in_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "shadow.js",
            "let x = 0;\nfunction f(xs) {\nfor (let x in xs) { }\nx = 1;\n}\n",
        ),
        1,
        "shadow.js: a write after the for-in loop reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "shadow.js",
            "let x = 0;\nfunction f(xs) {\nfor (let x in xs) {\nx = 1;\n}\n}\n",
        ),
        0,
        "shadow.js: a write inside the for-in loop stays local to the loop variable"
    );
}

#[test]
fn ts_for_of_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::TypeScript,
            "shadow.ts",
            "let x = 0;\nfunction f(xs: string[]) {\nfor (const x of xs) { }\nx = 1;\n}\n",
        ),
        1,
        "shadow.ts: a write after the for-of loop reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::TypeScript,
            "shadow.ts",
            "let x = 0;\nfunction f(xs: string[]) {\nfor (const x of xs) {\nx = 1;\n}\n}\n",
        ),
        0,
        "shadow.ts: a write inside the for-of loop stays local to the loop variable"
    );
}

#[test]
fn ocaml_let_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Ocaml,
            "lib.ml",
            "let counter = ref 0\nlet f b = if b then let counter = ref 1 in counter := 1 else counter := 2\n",
        ),
        1,
        "lib.ml: a write after the let reaches the file-scope ref"
    );
    assert_eq!(
        in_function_writes(
            Lang::Ocaml,
            "lib.ml",
            "let counter = ref 0\nlet f b = if b then let counter = ref 1 in counter := 1\n",
        ),
        0,
        "lib.ml: a write inside the let body stays local to the let-bound ref"
    );
}

#[test]
fn ocaml_match_case_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Ocaml,
            "lib.ml",
            "let counter = ref 0\nlet f x = match x with Some counter -> counter := 1 | None -> counter := 2\n",
        ),
        1,
        "lib.ml: a write in a later match arm reaches the file-scope ref"
    );
    assert_eq!(
        in_function_writes(
            Lang::Ocaml,
            "lib.ml",
            "let counter = ref 0\nlet f x = match x with Some counter -> counter := 1 | None -> ()\n",
        ),
        0,
        "lib.ml: a write inside the binding arm stays local to the pattern"
    );
}

#[test]
fn odin_for_in_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Odin,
            "scope.odin",
            "package p\ncounter: int\nxs: []int\nrun :: proc() {\nfor counter in xs {\ncounter = 1\n}\ncounter = 2\n}\n",
        ),
        1,
        "scope.odin: a write after the for-in loop reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::Odin,
            "scope.odin",
            "package p\ncounter: int\nxs: []int\nrun :: proc() {\nfor counter in xs {\ncounter = 1\n}\n}\n",
        ),
        0,
        "scope.odin: a write inside the for-in loop stays local to the loop variable"
    );
}
