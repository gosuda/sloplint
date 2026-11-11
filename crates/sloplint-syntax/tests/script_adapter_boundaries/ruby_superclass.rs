//! Ruby superclass resolution. A `superclass` node is a child of `class` whose text
//! starts with `<`; the class name is its expression child. Test detection and
//! implementation references both read that name, not the clause text.

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"class WidgetTest < TestCase
  def check(flag = true)
    flag
  end
end

class QualifiedTest < Minitest::Test
  def check(flag = true)
    flag
  end
end

class RailsTest < ActiveSupport::TestCase
  def check(flag = true)
    flag
  end
end

class Plain < Base
  def check(flag = true)
    flag
  end
end

class Child < Outer::Parent
  def go; end
end

def free(flag = true)
  flag
end
"#;

fn observe<R>(read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Ruby,
        path: "case.rb",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.rb parses");
    read(&analysis.facts)
}

#[test]
fn methods_of_testclass_subclasses_are_marked() {
    let flags: Vec<(u32, bool)> = observe(|facts| {
        facts
            .functions
            .iter()
            .map(|f| {
                let marked = facts.regions.iter().any(|region| {
                    region.role == CodeRole::Test
                        && region.start_line <= f.pos.line
                        && f.pos.line <= region.end_line
                });
                (f.pos.line, marked)
            })
            .collect()
    });
    assert_eq!(
        flags,
        vec![
            (2, true),
            (8, true),
            (14, true),
            (20, false),
            (26, false),
            (29, false),
        ]
    );
}

#[test]
fn implementation_references_carry_the_bare_superclass_name() {
    let bases: Vec<String> =
        observe(|facts| facts.impls.iter().map(|i| i.name.to_owned()).collect());
    assert_eq!(bases, ["TestCase", "Test", "TestCase", "Base", "Parent"]);
}
