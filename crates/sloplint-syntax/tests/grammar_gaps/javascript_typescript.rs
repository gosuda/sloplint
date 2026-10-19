//! Valid JavaScript and TypeScript that the published tree-sitter grammars rejected.
//!
//! Each source here is valid code. The vendored grammars (`vendor/tree-sitter-javascript`,
//! `vendor/tree-sitter-typescript`) must parse it with no error node, keep the tree shapes
//! the language adapters read, and still reject code that is invalid.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::{CustomRuleSpec, Severity};
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

/// JavaScript, TypeScript and TSX: every grammar that parses a script.
const SCRIPT_LANGS: [(Lang, &str); 3] = [
    (Lang::JavaScript, "a.js"),
    (Lang::TypeScript, "a.ts"),
    (Lang::Tsx, "a.tsx"),
];

/// The TypeScript grammars only.
const TS_LANGS: [(Lang, &str); 2] = [(Lang::TypeScript, "a.ts"), (Lang::Tsx, "a.tsx")];

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn analyze_with<'s>(
    custom: &'s CustomRules,
    lang: Lang,
    path: &'s str,
    source: &'s str,
) -> Analysis<'s> {
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, custom)
}

fn run<'s>(lang: Lang, path: &'s str, source: &'s str) -> Analysis<'s> {
    analyze_with(&NO_CUSTOM, lang, path, source)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(lang: Lang, source: &str) {
    let language: tree_sitter::Language = match lang {
        Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
        Lang::JavaScript => tree_sitter_javascript::LANGUAGE,
        other => panic!("assert_raw_clean: unexpected {other:?}"),
    }
    .into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("js/ts language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean<'s>(lang: Lang, path: &'s str, source: &'s str) -> Analysis<'s> {
    assert_raw_clean(lang, source);
    let analysis = run(lang, path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{lang:?} must parse {source:?} cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn assert_parse_error(lang: Lang, path: &str, source: &str) {
    let analysis = run(lang, path, source);
    assert!(
        analysis.parse_error.is_some(),
        "{lang:?} must reject the invalid source {source:?}"
    );
}

fn query_spec(language: &str, query: &str) -> CustomRuleSpec {
    CustomRuleSpec {
        id: "custom/probe".to_string(),
        language: language.to_string(),
        severity: Severity::Warn,
        message: "probe".to_string(),
        why: "probe".to_string(),
        fix: "probe".to_string(),
        query: query.to_string(),
        include: Vec::new(),
        exclude: Vec::new(),
    }
}

/// Texts captured as `@name` by `query` on a clean parse of `source`, in match order.
fn captures(lang: Lang, path: &str, source: &str, query: &str) -> Vec<String> {
    let key = if lang == Lang::JavaScript {
        "javascript"
    } else {
        "typescript"
    };
    let custom = CustomRules::compile(&[query_spec(key, query)]).expect("probe query compiles");
    let analysis = analyze_with(&custom, lang, path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{lang:?} must parse {source:?} cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
        .custom_hits
        .iter()
        .flat_map(|hit| {
            hit.captures
                .iter()
                .filter(|(name, _)| *name == "name")
                .map(|(_, text)| (*text).to_string())
        })
        .collect()
}

#[test]
fn tagged_template_invalid_escapes_parse_in_every_script_grammar() {
    let sources = [
        "const a = String.raw`[0]\\x`;\n",
        "const a = String.raw`.\\undefined`;\n",
        "const a = String.raw`\\u`;\n",
        "const a = String.raw`\\u{`;\n",
        "const a = String.raw`\\u{12`;\n",
        "const a = String.raw`\\x4`;\n",
        "const a = String.raw`\\u12`;\n",
        "const a = String.raw`\\xZ and \\u{ and \\u1`;\n",
    ];
    for (lang, path) in SCRIPT_LANGS {
        for source in sources {
            run_clean(lang, path, source);
        }
    }
}

#[test]
fn template_escapes_keep_the_escape_sequence_node() {
    let query = "(template_string (escape_sequence) @name) @match";
    for (lang, path) in SCRIPT_LANGS {
        let invalid = captures(lang, path, "String.raw`[0]\\x`;\n", query);
        assert_eq!(invalid, ["\\x"], "{lang:?}: invalid hex escape");
        let word = captures(lang, path, "String.raw`.\\undefined`;\n", query);
        assert_eq!(word, ["\\u"], "{lang:?}: \\u before letters");
        let valid = captures(
            lang,
            path,
            "const t = `a\\x41\\u0041\\u{1F600}\\101\\n${x}`;\n",
            query,
        );
        assert_eq!(
            valid,
            ["\\x41", "\\u0041", "\\u{1F600}", "\\101", "\\n"],
            "{lang:?}: valid escapes are unchanged"
        );
    }
}

#[test]
fn tagged_template_argument_shape_ignores_escape_validity() {
    for (lang, path) in SCRIPT_LANGS {
        let invalid = run_clean(lang, path, "tag`aaaa\\x bbbbbb`;\n");
        let valid = run_clean(lang, path, "tag`aaaa\\n bbbbbb`;\n");
        assert_eq!(invalid.facts.calls.len(), 1, "{lang:?}: one tagged call");
        assert_eq!(valid.facts.calls.len(), 1, "{lang:?}: one tagged call");
        assert_eq!(invalid.facts.calls[0].callee, "tag");
        assert_eq!(
            invalid.facts.calls[0].args, valid.facts.calls[0].args,
            "{lang:?}: the template argument shape depends on the escape only by validity"
        );
    }
}

#[test]
fn string_literals_keep_strict_escapes() {
    for (lang, path) in SCRIPT_LANGS {
        assert_parse_error(lang, path, "const s = '\\x';\n");
        assert_parse_error(lang, path, "const s = \"\\u\";\n");
    }
}

#[test]
fn using_declarations_are_declarations() {
    let source = "using file = getFile();\nawait using conn = open();\nusing a = 1, b = 2;\n";
    let decl = "(using_declaration (variable_declarator name: (identifier) @name)) @match";
    for (lang, path) in TS_LANGS {
        run_clean(lang, path, source);
        assert_eq!(
            captures(lang, path, source, decl),
            ["file", "conn", "a", "b"],
            "{lang:?}: every binding is a using_declaration declarator"
        );
        assert!(
            captures(lang, path, source, "(assignment_expression) @name @match").is_empty(),
            "{lang:?}: `using x = f()` is not an assignment"
        );
    }
}

#[test]
fn using_declarations_with_type_annotations_parse() {
    let source =
        "using file: Disposable = getFile();\nawait using conn: AsyncDisposable = open();\n";
    for (lang, path) in TS_LANGS {
        run_clean(lang, path, source);
    }
}

#[test]
fn using_in_for_heads_parses() {
    let heads = [
        "for (using r of xs) { r.go(); }\n",
        "for (await using r of xs) { r.go(); }\n",
        "for await (await using r of xs) { r.go(); }\n",
        "for (using d1 = open(), d2 = null;;) { break; }\n",
    ];
    for (lang, path) in TS_LANGS {
        for head in heads {
            run_clean(lang, path, head);
        }
    }
    let of_heads = "for (using r of xs) {}\nfor (await using s of ys) {}\n";
    for (lang, path) in TS_LANGS {
        assert_eq!(
            captures(
                lang,
                path,
                of_heads,
                "(for_in_statement \"using\" left: (identifier) @name) @match"
            ),
            ["r", "s"],
            "{lang:?}: the loop variable keeps the left field"
        );
    }
}

#[test]
fn using_declarations_do_not_disturb_function_and_binding_facts() {
    let source = "let top = 1;\nfunction f() {\n  let n = 0;\n  using h = open();\n  n += 1;\n  return h;\n}\n";
    for (lang, path) in TS_LANGS {
        let analysis = run_clean(lang, path, source);
        let names: Vec<_> = analysis.facts.functions.iter().map(|f| f.name).collect();
        assert_eq!(names, ["f"], "{lang:?}: function facts");
        let bound: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
        assert_eq!(
            bound,
            ["n"],
            "{lang:?}: using is immutable, only `let` is tracked"
        );
        let globals: Vec<_> = analysis
            .facts
            .ambient
            .iter()
            .filter(|a| a.kind == sloplint_core::facts::AmbientKind::GlobalMutable)
            .map(|a| a.name)
            .collect();
        assert_eq!(
            globals,
            ["top"],
            "{lang:?}: only the top-level `let` is a mutable global"
        );
    }
}

#[test]
fn using_still_works_as_an_identifier_in_expressions() {
    let source = "const z = using + 1;\n";
    for (lang, path) in SCRIPT_LANGS {
        run_clean(lang, path, source);
    }
}

#[test]
fn reserved_words_after_optional_chaining_parse() {
    let source = "a?.delete(b);\nm?.class;\nm?.a.delete(1);\nm?.[0];\nm?.(x);\nx.delete(y);\n";
    for (lang, path) in SCRIPT_LANGS {
        run_clean(lang, path, source);
    }
    for (lang, path) in TS_LANGS {
        run_clean(lang, path, "A?.<B>();\n");
    }
}

#[test]
fn reserved_words_as_type_query_properties_parse() {
    let source = "type A = import('x').default;\ntype B = typeof Map.prototype.delete;\nlet c: typeof Promise.prototype.finally;\n";
    for (lang, path) in TS_LANGS {
        run_clean(lang, path, source);
    }
}

#[test]
fn reserved_words_as_export_and_import_names_parse() {
    let source = "export { a as null, b as function, c as instanceof };\nexport { function, default } from './x';\nimport { instanceof as x } from './y';\n";
    for (lang, path) in SCRIPT_LANGS {
        run_clean(lang, path, source);
    }
}

#[test]
fn import_facts_keep_keyword_names() {
    let source = "import { default as d, instanceof as i, plain } from './m';\n";
    for (lang, path) in SCRIPT_LANGS {
        let analysis = run_clean(lang, path, source);
        let named: Vec<_> = analysis
            .facts
            .imports
            .iter()
            .map(|import| (import.name, import.alias))
            .collect();
        assert_eq!(
            named,
            [
                ("default", Some("d")),
                ("instanceof", Some("i")),
                ("plain", None)
            ],
            "{lang:?}: import facts"
        );
    }
}

#[test]
fn exported_names_skip_anonymous_keyword_exports() {
    let source = "export function shown() {}\nfunction hidden() {}\nexport { shown, default };\n";
    for (lang, path) in TS_LANGS {
        let analysis = run_clean(lang, path, source);
        let names: Vec<_> = analysis
            .facts
            .functions
            .iter()
            .map(|f| (f.name, f.public))
            .collect();
        assert_eq!(
            names,
            [("shown", true), ("hidden", false)],
            "{lang:?}: the anonymous `default` export must not mark `hidden` public"
        );
    }
}

#[test]
fn of_is_a_valid_variable_name() {
    let source = "for (var of;;) {}\nvar of = 1;\nlet of2 = of;\n";
    for (lang, path) in SCRIPT_LANGS {
        run_clean(lang, path, source);
    }
}

#[test]
fn await_with_a_parenthesized_callee_is_not_a_call_to_await() {
    let source = "async function g() {\n  const a = await (f ?? h)(x);\n  const b = await (await fetch(u)).json();\n}\n";
    for (lang, path) in SCRIPT_LANGS {
        let analysis = run_clean(lang, path, source);
        let callees: Vec<_> = analysis.facts.calls.iter().map(|c| c.callee).collect();
        assert!(
            !callees.contains(&"await"),
            "{lang:?}: `await (..)` must build an await expression, calls: {callees:?}"
        );
        assert_eq!(
            callees,
            ["(f ?? h)", "(await fetch(u)).json", "fetch"],
            "{lang:?}: call facts"
        );
    }
}

#[test]
fn reserved_words_as_jsx_names_parse() {
    let source = "const a = <div class=\"x\" for=\"y\" default=\"z\" />;\nconst b = <Foo.default.return />;\n";
    for (lang, path) in [(Lang::JavaScript, "a.js"), (Lang::Tsx, "a.tsx")] {
        run_clean(lang, path, source);
    }
}

#[test]
fn parenthesized_decorator_with_super_parses() {
    let source = "class C extends B {\n  @(super.decorate)\n  method() {}\n}\n";
    for (lang, path) in TS_LANGS {
        run_clean(lang, path, source);
    }
}

#[test]
fn plain_declarations_keep_their_shapes() {
    let source = "export const mode = 1;\nlet total = 0;\nfunction add(a: number): number {\n  total += a;\n  return total;\n}\n";
    for (lang, path) in TS_LANGS {
        let analysis = run_clean(lang, path, source);
        let names: Vec<_> = analysis.facts.functions.iter().map(|f| f.name).collect();
        assert_eq!(names, ["add"], "{lang:?}: function facts");
        let globals: Vec<_> = analysis.facts.ambient.iter().map(|a| a.name).collect();
        assert_eq!(
            globals,
            ["total"],
            "{lang:?}: `let` is the only mutable global"
        );
    }
}

#[test]
fn invalid_code_still_reports_a_parse_error() {
    for (lang, path) in SCRIPT_LANGS {
        assert_parse_error(lang, path, "function (\n");
        assert_parse_error(lang, path, "const = 1;\n");
        assert_parse_error(lang, path, "for (using of xs) {\n");
    }
    for (lang, path) in TS_LANGS {
        assert_parse_error(lang, path, "using x f();\n");
        assert_parse_error(lang, path, "await using = 1 2;\n");
    }
}
