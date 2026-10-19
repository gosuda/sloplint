//! PHP test detection: a method is a test when its enclosing class extends `TestCase`.
//!
//! `base_clause` is a child of `class_declaration`, a sibling of the class body, so
//! the lookup must start from the enclosing class and read its base names; the
//! clause text itself starts with the `extends` keyword and is never a class name.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[test]
fn methods_of_a_test_case_subclass_are_in_test() {
    let source = r#"<?php
class WidgetSupport extends TestCase
{
    public function testFlag(bool $on): void
    {
        $this->assertTrue($on);
    }
}
class Plain extends Base
{
    public function flag(bool $on): void
    {
        $this->use($on);
    }
}
class Qualified extends \PHPUnit\Framework\TestCase
{
    public function check(bool $on): void
    {
    }
}
function helper(bool $on): void
{
}
"#;
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Php,
        path: "case.php",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.php parses");
    let flags: Vec<(String, bool)> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.in_test))
        .collect();
    let expected: Vec<(String, bool)> = [
        ("testFlag", true),
        ("flag", false),
        ("check", true),
        ("helper", false),
    ]
    .into_iter()
    .map(|(name, in_test)| (name.to_owned(), in_test))
    .collect();
    assert_eq!(flags, expected);
}
