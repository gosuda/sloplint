//! Ruby field copies through keyword-style pairs. `customer_id: order.customer_id`
//! carries the key as a bare `hash_key_symbol` (the colon is a separate token), the
//! same way `:a =>` and `"b" =>` keys carry theirs.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"class Builder
  def build(order)
    Invoice.new(customer_id: order.customer_id, total: order.total, currency: order.currency, issued_on: order.issued_on)
  end

  def rocket(order)
    { :a => order.a, "b" => order.b, c: order.c, "d": order.d }
  end

  def mixed(order, other)
    Invoice.new(customer_id: order.customer_id, total: other.total)
  end

  def renamed(order)
    Invoice.new(customer_id: order.id, total: order.total)
  end
end
"#;

#[test]
fn keyword_and_rocket_pairs_report_one_to_one_copies() {
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
    let copies: Vec<(u32, u32, Option<String>)> = analysis
        .facts
        .field_copies
        .iter()
        .map(|c| (c.pos.line, c.fields, c.source.map(str::to_owned)))
        .collect();
    assert_eq!(
        copies,
        vec![
            (3, 4, Some("order".to_owned())),
            (7, 4, Some("order".to_owned())),
        ]
    );
}
