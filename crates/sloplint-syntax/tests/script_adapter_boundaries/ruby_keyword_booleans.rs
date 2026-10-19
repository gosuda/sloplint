//! Ruby keyword parameters with a `true` or `false` default are boolean flags, exactly
//! like positional optional parameters with the same default.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"class Exporter
  def export(rows, verbose: false, compress: true)
    rows
  end

  def labelled(rows, label: "x", limit:, flag: nil)
    rows
  end

  def plain(rows, opt = true)
    rows
  end
end
"#;

fn boolean_defaults(function: &str) -> Vec<(String, bool)> {
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
    let found = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == function)
        .unwrap_or_else(|| panic!("case.rb has function {function}"));
    found
        .params
        .iter()
        .map(|p| (p.name.to_owned(), p.bool_default))
        .collect()
}

fn flags(items: &[(&str, bool)]) -> Vec<(String, bool)> {
    items
        .iter()
        .map(|(name, flag)| ((*name).to_owned(), *flag))
        .collect()
}

#[test]
fn keyword_parameters_with_boolean_defaults_are_flags() {
    assert_eq!(
        boolean_defaults("export"),
        flags(&[("rows", false), ("verbose", true), ("compress", true)])
    );
}

#[test]
fn keyword_parameters_without_a_boolean_default_are_not_flags() {
    assert_eq!(
        boolean_defaults("labelled"),
        flags(&[
            ("rows", false),
            ("label", false),
            ("limit", false),
            ("flag", false)
        ])
    );
}

#[test]
fn optional_positional_parameter_still_counts() {
    assert_eq!(
        boolean_defaults("plain"),
        flags(&[("rows", false), ("opt", true)])
    );
}
