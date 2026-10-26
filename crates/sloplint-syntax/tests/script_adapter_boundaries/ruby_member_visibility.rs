//! Ruby method visibility follows the nearest preceding marker among a method's siblings.
//!
//! Bare `private`, `protected`, and `public` identifiers and parenthesized `private()`
//! calls set the state; argument-bearing calls and a bare `module_function` do not.
//! Wrapped definitions and singleton methods ignore the state. Every container (class,
//! singleton class, block, top level) keeps its own sibling list, observed through
//! `analyze` only.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// `(name, public)` for every function in `source`, sorted by name.
fn publicity(source: &str) -> Vec<(String, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Ruby,
        path: "case.rb",
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.rb parses cleanly");
    let mut out: Vec<(String, bool)> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.public))
        .collect();
    out.sort();
    out
}

/// The expected `(name, public)` pairs, sorted by name.
fn expected(pairs: &[(&str, bool)]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = pairs
        .iter()
        .map(|(name, public)| ((*name).to_owned(), *public))
        .collect();
    out.sort();
    out
}

#[test]
fn bare_markers_set_visibility_for_the_methods_after_them() {
    let source = r#"class Gate
  def open_default
  end

  private

  def closed_after_private
  end

  protected

  def closed_after_protected
  end

  public

  def open_after_public
  end

  private

  def closed_after_second_private
  end
end
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("open_default", true),
            ("closed_after_private", false),
            ("closed_after_protected", false),
            ("open_after_public", true),
            ("closed_after_second_private", false),
        ])
    );
}

#[test]
fn argument_bearing_calls_and_wrapped_definitions_do_not_change_visibility() {
    let source = r#"class Gate
  private

  def helper_one
  end

  public :helper_one

  def closed_after_argument_call
  end

  private def wrapped_private
  end

  def closed_after_wrapped
  end

  module_function def wrapped_module_function
  end
end

class Shout
  private

  public def wrapped_public
  end
end
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("helper_one", false),
            ("closed_after_argument_call", false),
            ("wrapped_private", false),
            ("closed_after_wrapped", false),
            ("wrapped_module_function", false),
            ("wrapped_public", true),
        ])
    );
}

#[test]
fn bare_module_function_is_not_a_marker_but_an_empty_call_is() {
    let source = r#"module Util
  module_function

  def bare_module_function_stays_public
  end

  module_function()

  def closed_after_empty_call
  end
end
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("bare_module_function_stays_public", true),
            ("closed_after_empty_call", false),
        ])
    );
}

#[test]
fn singleton_methods_are_public_even_after_private() {
    let source = r#"class Gate
  private

  def self.open_singleton
  end
end
"#;
    assert_eq!(publicity(source), expected(&[("open_singleton", true)]));
}

#[test]
fn singleton_class_body_has_its_own_marker_scope() {
    let source = r#"class Gate
  private

  class << self
    def open_in_sclass
    end

    private

    def closed_in_sclass
    end
  end

  def closed_after_sclass
  end
end
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("open_in_sclass", true),
            ("closed_in_sclass", false),
            ("closed_after_sclass", false),
        ])
    );
}

#[test]
fn macro_calls_between_marker_and_method_keep_the_state() {
    let source = r#"class Gate
  private
  attr_reader :tracked
  include Comparable

  def closed_across_macros
  end
end
"#;
    assert_eq!(
        publicity(source),
        expected(&[("closed_across_macros", false)])
    );
}

#[test]
fn nested_containers_and_blocks_judge_methods_in_their_own_body() {
    let source = r#"private

def top_level_closed
end

class Outer
  def outer_open
  end

  class Inner
    def inner_open
    end
  end
end

class Host
  private

  class Nested
    def nested_open
    end
  end

  def host_closed
  end

  included do
    private

    def in_block_closed
    end
  end
end
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("top_level_closed", false),
            ("outer_open", true),
            ("inner_open", true),
            ("nested_open", true),
            ("host_closed", false),
            ("in_block_closed", false),
        ])
    );
}
