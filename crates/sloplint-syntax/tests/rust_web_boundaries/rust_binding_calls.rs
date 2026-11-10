//! Rust call initializer and argument shapes, observed through `analyze` only.
//!
//! A call that is the whole initializer of a `let` or `const` binding flags
//! `initializes_binding`. Bracket-delimited macro bodies read as collections, closures and
//! blocks as code bodies, so a long `vec![..]` argument shapes differently from a long
//! callback.

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn calls(source: &str) -> Vec<(String, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "call source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .calls
        .iter()
        .map(|c| (c.callee.to_string(), c.initializes_binding))
        .collect()
}

#[test]
fn whole_initializers_flag_and_plain_statements_do_not() {
    let source = "fn f() {\nlet x = compute(a);\ncompute(b);\nlet y: u32 = compute();\n}\nconst C: u32 = compute();\n";
    assert_eq!(
        calls(source),
        vec![
            ("compute".to_string(), true),
            ("compute".to_string(), false),
            ("compute".to_string(), true),
            ("compute".to_string(), true),
        ]
    );
}

#[test]
fn wrapped_calls_are_not_whole_initializers() {
    let source = "fn f() {\nlet x = compute(a)?;\n}\n";
    assert_eq!(calls(source), vec![("compute".to_string(), false)]);
}

fn first_arg_tag(source: &str) -> (&'static str, u32, u32) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "shape source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    let arg = &analysis.facts.calls[0].args[0];
    let tag = match &arg.shape {
        ArgShape::Str { .. } => "str",
        ArgShape::Concat => "concat",
        ArgShape::Ident(_) => "ident",
        ArgShape::Collection => "collection",
        ArgShape::CodeBody => "code",
        ArgShape::Other => "other",
    };
    (tag, arg.start_line, arg.end_line)
}

#[test]
fn long_vec_body_is_a_collection() {
    let mut source = String::from("fn f() {\ncompute(vec![\n");
    for i in 0..15 {
        source.push_str(&format!("    {i},\n"));
    }
    source.push_str("]);\n}\n");
    let (tag, start, end) = first_arg_tag(&source);
    assert_eq!(tag, "collection", "vec![..] reads as a collection");
    assert!(
        end - start + 1 >= 15,
        "the argument spans the whole literal"
    );
}

#[test]
fn closures_and_blocks_are_code_bodies() {
    let (tag, _, _) = first_arg_tag("fn f() {\nrun(|x| x + 1);\n}\n");
    assert_eq!(tag, "code", "a closure reads as a code body");
    let (tag, _, _) = first_arg_tag("fn f() {\nrun({ compute(); });\n}\n");
    assert_eq!(tag, "code", "a block reads as a code body");
}
