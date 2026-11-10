//! Ruby method visibility follows the nearest preceding marker among a method's siblings.
//!
//! Bare `private`, `protected`, and `public` identifiers and parenthesized `private()`
//! calls set the state; argument-bearing calls and a bare `module_function` do not.
//! Wrapped definitions and singleton methods ignore the state. Every container (class,
//! singleton class, block, top level) keeps its own sibling list, observed through
//! `analyze` only.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// `(name, public)` for every function in `source`, sorted by name.
fn publicity(source: &str) -> Vec<(String, Visibility)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Ruby,
        path: "case.rb",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.rb parses cleanly");
    let mut out: Vec<(String, Visibility)> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.visibility))
        .collect();
    out.sort();
    out
}

/// The expected `(name, public)` pairs, sorted by name.
fn expected(pairs: &[(&str, Visibility)]) -> Vec<(String, Visibility)> {
    let mut out: Vec<(String, Visibility)> = pairs
        .iter()
        .map(|(name, vis)| ((*name).to_owned(), *vis))
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
            ("open_default", Visibility::Exported),
            ("closed_after_private", Visibility::Private),
            ("closed_after_protected", Visibility::Private),
            ("open_after_public", Visibility::Exported),
            ("closed_after_second_private", Visibility::Private),
        ]),
        "bare private, protected, and public markers set later sibling visibility"
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
            ("helper_one", Visibility::Private),
            ("closed_after_argument_call", Visibility::Private),
            ("wrapped_private", Visibility::Private),
            ("closed_after_wrapped", Visibility::Private),
            ("wrapped_module_function", Visibility::Private),
            ("wrapped_public", Visibility::Exported),
        ]),
        "argument calls and wrapped definitions leave the marker state unchanged"
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
            ("bare_module_function_stays_public", Visibility::Exported),
            ("closed_after_empty_call", Visibility::Private),
        ]),
        "bare module_function is not a marker but an empty call closes later siblings"
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
    assert_eq!(
        publicity(source),
        expected(&[("open_singleton", Visibility::Exported)]),
        "singleton methods stay Exported after private"
    );
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
            ("open_in_sclass", Visibility::Exported),
            ("closed_in_sclass", Visibility::Private),
            ("closed_after_sclass", Visibility::Private),
        ]),
        "singleton-class bodies scope markers separately from the outer class"
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
        expected(&[("closed_across_macros", Visibility::Private)]),
        "macro calls between a marker and a method keep the Private state"
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
            ("top_level_closed", Visibility::Private),
            ("outer_open", Visibility::Exported),
            ("inner_open", Visibility::Exported),
            ("nested_open", Visibility::Exported),
            ("host_closed", Visibility::Private),
            ("in_block_closed", Visibility::Private),
        ]),
        "each container and block judges its own methods under its own markers"
    );
}
