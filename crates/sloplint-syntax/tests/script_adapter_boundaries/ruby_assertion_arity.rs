//! Minitest assertions with two required operands: `assert_kind_of cls, obj`,
//! `assert_instance_of cls, obj`, `assert_respond_to obj, meth`, and
//! `assert_predicate obj, pred`. A trailing string in the second operand slot is
//! an operand, not a failure message.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"class VerifyTest < Minitest::Test
  def verify(value)
    assert_kind_of String, "abc"
    assert_instance_of Hash, "abc"
    assert_respond_to "abc", "upcase"
    assert_predicate "abc", "empty?"
    refute_kind_of Integer, "abc"
    assert_kind_of String, value, "must be a string"
    assert_nil value, "must be nil"
    assert_equal 1, value, "must equal"
  end
end
"#;

#[test]
fn trailing_string_is_a_message_only_after_both_operands() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Ruby,
        path: "case.rb",
        source: SOURCE,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.rb parses");
    let messages: Vec<(u32, bool)> = analysis
        .facts
        .assertions
        .iter()
        .map(|a| (a.pos.line, a.has_message))
        .collect();
    assert_eq!(
        messages,
        vec![
            (3, false),
            (4, false),
            (5, false),
            (6, false),
            (7, false),
            (8, true),
            (9, true),
            (10, true),
        ]
    );
}
