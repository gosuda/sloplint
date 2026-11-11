//! PHP test detection: a method is a test case when its enclosing class extends `TestCase`.
//!
//! `base_clause` is a child of `class_declaration`, a sibling of the class body, so
//! the lookup must start from the enclosing class and read its base names; the
//! clause text itself starts with the `extends` keyword and is never a class name.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[test]
fn methods_of_a_testclass_subclass_are_marked() {
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
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.php parses");
    let flags: Vec<(String, bool)> = analysis
        .facts
        .functions
        .iter()
        .map(|f| {
            let marked = analysis.facts.regions.iter().any(|region| {
                region.role == CodeRole::Test
                    && region.start_line <= f.pos.line
                    && f.pos.line <= region.end_line
            });
            (f.name.to_owned(), marked)
        })
        .collect();
    let expected: Vec<(String, bool)> = [
        ("testFlag", true),
        ("flag", false),
        ("check", true),
        ("helper", false),
    ]
    .into_iter()
    .map(|(name, marked)| (name.to_owned(), marked))
    .collect();
    assert_eq!(flags, expected);
}
