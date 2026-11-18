//! Type-syntax gaps in the vendored TypeScript/TSX grammars (upstream
//! tree-sitter-typescript 0.23.2): `import("m")` inside type arguments,
//! generic tagged templates, and `import("m").T[]`/`import("m").T[K]`.

use super::javascript_typescript::{TS_LANGS, captures, run_clean};

#[test]
fn import_calls_inside_call_type_arguments_parse() {
    // upstream 0.23.2 bug: in `foo<T>()`, `typeof import("m")` and a bare
    // `import("m")` type argument died in the merged type/expression state;
    // vitest's `importOriginal<typeof import("...")>` hits it.
    let sources = [
        "const a = foo<typeof import(\"m\")>();\n",
        "const a = foo<import(\"m\")>();\n",
        "const a = foo<typeof import(\"m\").T>();\n",
        "const a = foo<import(\"m\").T>();\n",
        "const a = foo<typeof import(\"m\"), number>();\n",
        "const a = await foo<typeof import(\"m\")>();\n",
        "const a: Foo<Bar<typeof bar>> = x;\n",
    ];
    for (lang, path) in TS_LANGS {
        for source in sources {
            run_clean(lang, path, source);
        }
    }
}

#[test]
fn import_call_type_argument_keeps_call_shape() {
    let query = "(call_expression function: (import) @name) @match";
    for (lang, path) in TS_LANGS {
        assert_eq!(
            captures(lang, path, "const a = foo<import(\"m\")>();\n", query),
            ["import"],
            "{lang:?}: the bare import() type argument is a call"
        );
    }
}

#[test]
fn instantiation_expression_tagged_template_parses() {
    // upstream 0.23.2 bug: `foo<T>\`tpl\`` demanded a non-null `!` before the
    // template; styled-components `styled(Link)<P>\`...\`` and drizzle
    // `sql<T>\`...\`` hit it.
    let sources = [
        "const n = styled(Link)<{ active: boolean }>`color: red;`;\n",
        "const r = sql<typeof projects.$inferSelect.meta>`null`;\n",
        "const n = f<T>!`x`;\n",
    ];
    for (lang, path) in TS_LANGS {
        for source in sources {
            run_clean(lang, path, source);
        }
    }
}

#[test]
fn import_type_array_suffix_parses() {
    // upstream 0.23.2 bug: `import("m").T[]` — the `[]` suffix could not attach
    // to the in-annotation import member chain.
    let sources = [
        "let e: import(\"node:fs\").Dirent[];\n",
        "function f(d: string): import(\"node:fs\").Dirent[] { return []; }\n",
        "let e: import(\"m\").T.U[];\n",
    ];
    for (lang, path) in TS_LANGS {
        for source in sources {
            run_clean(lang, path, source);
        }
    }
}

#[test]
fn instantiation_tagged_template_keeps_call_shape() {
    let query = "(call_expression function: (instantiation_expression) @name arguments: (template_string)) @match";
    for (lang, path) in TS_LANGS {
        let got = captures(lang, path, "const n = f<T>`x`;\n", query);
        assert_eq!(
            got,
            ["f<T>"],
            "{lang:?}: instantiation tag wraps the template call"
        );
    }
}
