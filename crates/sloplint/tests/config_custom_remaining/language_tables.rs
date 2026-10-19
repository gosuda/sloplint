//! Shared config keys route one custom rule to every file spelling of a language.

use sloplint_core::lang::Lang;
use sloplint_core::{ConfigError, CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn rule(language: &str, query: &str) -> CustomRuleSpec {
    CustomRuleSpec {
        id: "custom/probe".to_string(),
        language: language.to_string(),
        severity: Severity::Warn,
        message: "m".to_string(),
        why: "w".to_string(),
        fix: "f".to_string(),
        query: query.to_string(),
        include: Vec::new(),
        exclude: Vec::new(),
    }
}

type ObservedHit<'s> = (u32, u32, Vec<(&'s str, &'s str)>);

fn observed<'s>(
    custom: &'s CustomRules,
    lang: Lang,
    path: &'s str,
    source: &'s str,
) -> Vec<ObservedHit<'s>> {
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut parsers, &input, custom);
    assert!(analysis.parse_error.is_none(), "{path} must parse");
    let mut hits: Vec<_> = analysis
        .custom_hits
        .iter()
        .map(|hit| {
            let mut captures = hit.captures.to_vec();
            captures.sort_unstable();
            (hit.pos.line, hit.pos.byte, captures)
        })
        .collect();
    hits.sort_unstable();
    hits
}

#[test]
fn an_unknown_config_key_is_a_typed_error() {
    let Err(error) = CustomRules::compile(&[rule("cobol", "(_) @match")]) else {
        panic!("an unknown language must be rejected");
    };
    let ConfigError::UnknownLanguage { value } = &error else {
        panic!("expected an unknown language error, got: {error}");
    };
    assert_eq!(value, "cobol");
}

#[test]
fn shared_config_keys_run_against_every_file_spelling() {
    let typescript = CustomRules::compile(&[rule(
        "typescript",
        "(lexical_declaration (variable_declarator name: (identifier) @name) @match)",
    )])
    .unwrap_or_else(|e| panic!("typescript rule failed to compile: {e}"));
    let ocaml = CustomRules::compile(&[rule(
        "ocaml",
        "(value_definition (let_binding pattern: (value_name) @name)) @match
(value_specification (value_name) @name) @match",
    )])
    .unwrap_or_else(|e| panic!("ocaml rule failed to compile: {e}"));
    assert_eq!(
        observed(
            &typescript,
            Lang::TypeScript,
            "a.ts",
            "// header\nconst x: number = 1;\n"
        ),
        [(2, 16, vec![("match", "x: number = 1"), ("name", "x")])]
    );
    assert_eq!(
        observed(
            &typescript,
            Lang::Tsx,
            "a.tsx",
            "// header\nconst x = <div />;\n"
        ),
        [(2, 16, vec![("match", "x = <div />"), ("name", "x")])]
    );
    assert_eq!(
        observed(&ocaml, Lang::Ocaml, "a.ml", "(* header *)\nlet x = 1\n"),
        [(2, 13, vec![("match", "let x = 1"), ("name", "x")])]
    );
    assert_eq!(
        observed(
            &ocaml,
            Lang::OcamlInterface,
            "a.mli",
            "(* header *)\nval x : int\n"
        ),
        [(2, 13, vec![("match", "val x : int"), ("name", "x")])]
    );
    let rust = CustomRules::compile(&[rule("rust", "(_) @match")])
        .unwrap_or_else(|e| panic!("rust rule failed to compile: {e}"));
    assert_eq!(
        observed(&rust, Lang::Ocaml, "a.ml", "let x = 1\n"),
        Vec::new()
    );
}
