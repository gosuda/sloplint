//! PHP field copies: array literals and constructor calls that copy `$source->field`
//! one-to-one.
//!
//! `$row->id` has a `variable_name` object, and `new Dto(...)` carries its
//! `arguments` list as an unnamed child, so both shapes need explicit handling.

use sloplint_core::facts::{CallKind, FileFacts};
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"<?php
function rows($row)
{
    return ['id' => $row->id, 'name' => $row->name, 'email' => $row->email];
}
function build($row)
{
    return new Dto(id: $row->id, name: $row->name, email: $row->email, role: $row->role);
}
function mixed($row, $other)
{
    return ['id' => $row->id, 'name' => $other->name];
}
function renamed($row)
{
    return new Dto(id: $row->identifier, name: $row->name);
}
function positional($row)
{
    return new Dto($row->id, $row->name);
}
"#;

fn observe<R>(read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Php,
        path: "case.php",
        source: SOURCE,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.php parses");
    read(&analysis.facts)
}

#[test]
fn array_and_constructor_copies_name_the_source_variable() {
    let copies = observe(|facts| {
        facts
            .field_copies
            .iter()
            .map(|c| (c.pos.line, c.fields, c.source.map(str::to_owned)))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        copies,
        vec![
            (4, 3, Some("row".to_owned())),
            (8, 4, Some("row".to_owned())),
        ]
    );
}

#[test]
fn constructor_calls_report_their_arguments() {
    let arities = observe(|facts| {
        facts
            .calls
            .iter()
            .filter(|c| c.kind == CallKind::New)
            .map(|c| (c.pos.line, c.callee.to_owned(), c.args.len()))
            .collect::<Vec<_>>()
    });
    let expected: Vec<(u32, String, usize)> = [(8, 4), (16, 2), (20, 2)]
        .into_iter()
        .map(|(line, args)| (line, "Dto".to_owned(), args))
        .collect();
    assert_eq!(arities, expected);
}
