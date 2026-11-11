use sloplint_core::facts::{AssertShape, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

const VIS: &str = r#"def open_m
  1
end

private

def secret
  2
end

def self.build
  3
end

def render
  4
end

def to_s
  "x"
end
"#;

const TEST_CLASS: &str = r#"class CalcTest < TestCase
  def test_add
    1
  end
end

class Plain
  def helper
    2
  end
end
"#;

const DESCRIBE: &str = r#"RSpec.describe "calc" do
  xit "adds" do
    expect(1).to eq(1)
  end
end
"#;

const XIT_ALONE: &str = r#"xit "adds" do
  1
end
"#;

const ASSERTS: &str = r#"assert_equal expected, actual
assert ready
"#;

const WILDCARD: &str = r#"case status
when :ok
  1
else
  2
end
"#;

const NO_ELSE: &str = r#"case status
when :ok
  1
end
"#;

const CALL_INIT: &str = r#"x = foo(1)
"#;

const NO_COND: &str = r#"case
when ready
  1
else
  2
end
"#;

const CHAINED_INIT: &str = r#"x = foo(1).bar
"#;

pub(crate) fn with_facts(source: &str, mut check: impl FnMut(&Analysis<'_>)) {
    let custom = match CustomRules::compile(&[]) {
        Ok(custom) => custom,
        Err(err) => panic!("empty custom rules compile: {err:?}"),
    };
    let input = FileInput {
        lang: Lang::Ruby,
        path: "case.rb",
        source,
        role: CodeRole::Library,
    };
    check(&analyze(&mut Parsers::new(), &input, &custom));
}

fn visibility_of(source: &str, name: &str) -> Visibility {
    let mut found = None;
    with_facts(source, |analysis| {
        if analysis.parse_error.is_some() {
            panic!("parses {name}");
        }
        for f in &analysis.facts.functions {
            if f.name == name {
                found = Some(f.visibility);
            }
        }
    });
    match found {
        Some(visibility) => visibility,
        None => panic!("has function {name}"),
    }
}

#[test]
fn private_marker_makes_method_private() {
    assert_eq!(
        visibility_of(VIS, "open_m"),
        Visibility::Exported,
        "method before marker stays exported"
    );
    assert_eq!(
        visibility_of(VIS, "secret"),
        Visibility::Private,
        "method after private marker is private"
    );
}

#[test]
fn singleton_method_is_exported() {
    assert_eq!(
        visibility_of(VIS, "build"),
        Visibility::Exported,
        "singleton method is exported"
    );
}

#[test]
fn protocol_name_keeps_signature_forced() {
    with_facts(VIS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses protocol sample");
        let forced = |name: &str| {
            analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == name)
                .map_or_else(|| panic!("has function {name}"), |f| f.signature_forced)
        };
        assert!(forced("to_s"), "to_s keeps signature_forced");
        assert!(!forced("render"), "ordinary method is not forced");
    });
}

#[test]
fn minitest_class_marks_in_test() {
    with_facts(TEST_CLASS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses test class");
        let flagged = |name: &str| {
            analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == name)
                .map_or_else(|| panic!("has function {name}"), |f| f.in_test)
        };
        assert!(
            flagged("test_add"),
            "method in TestCase subclass is in test"
        );
        assert!(!flagged("helper"), "method in plain class is not in test");
    });
}

#[test]
fn describe_block_emits_one_test_region() {
    with_facts(DESCRIBE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses describe block");
        assert_eq!(
            analysis.facts.regions.len(),
            1,
            "describe block emits exactly one region"
        );
        assert_eq!(
            analysis.facts.regions[0].role,
            CodeRole::Test,
            "describe region is Test"
        );
        assert_eq!(
            analysis.facts.regions[0].start_line, 1,
            "describe region starts at block"
        );
        assert_eq!(
            analysis.facts.regions[0].end_line, 5,
            "describe region covers whole block"
        );
    });
    with_facts(XIT_ALONE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses lone xit");
        assert!(
            analysis.facts.regions.is_empty(),
            "xit alone emits no test region"
        );
    });
}

#[test]
fn minitest_comparison_assertion_is_top_eq_ne() {
    with_facts(ASSERTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses assertions");
        assert_eq!(analysis.facts.assertions.len(), 2, "finds both assertions");
        let equal = &analysis.facts.assertions[0];
        assert!(equal.comparison, "assert_equal compares");
        assert_eq!(
            equal.shape,
            AssertShape::TopEqNe,
            "assert_equal compares at the top"
        );
        assert!(
            !equal.in_typecheck,
            "assert_equal is not under type checking"
        );
        assert!(
            !equal.value_is_bool_literal,
            "assert_equal value is not a bool literal"
        );
        let bare = &analysis.facts.assertions[1];
        assert!(!bare.comparison, "bare assert does not compare");
        assert_eq!(
            bare.shape,
            AssertShape::BareName,
            "bare identifier assert is a bare name"
        );
    });
}

#[test]
fn ruby_else_arm_reports_ident_scrutinee() {
    with_facts(WILDCARD, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses case with else");
        assert_eq!(
            analysis.facts.wildcard_arms.len(),
            1,
            "else arm is the only wildcard arm"
        );
        let arm = &analysis.facts.wildcard_arms[0];
        assert_eq!(
            arm.scrutinee,
            ScrutineeKind::Ident,
            "identifier scrutinee is reported"
        );
        assert_eq!(arm.variant_count, 0, "symbol patterns count no variants");
        assert!(
            !arm.has_nullable_ctor,
            "ruby arm has no nullable constructor"
        );
    });
    with_facts(NO_COND, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses conditionless case");
        assert_eq!(
            analysis.facts.wildcard_arms.len(),
            1,
            "else arm is still the only wildcard arm"
        );
        assert_eq!(
            analysis.facts.wildcard_arms[0].scrutinee,
            ScrutineeKind::Other,
            "missing scrutinee stays Other"
        );
    });
    with_facts(NO_ELSE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses case without else");
        assert!(
            analysis.facts.wildcard_arms.is_empty(),
            "case without else has no wildcard arm"
        );
    });
}

#[test]
fn ruby_bare_calls_report_simple_paths() {
    with_facts(CALL_INIT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses call binding");
        assert_eq!(analysis.facts.calls.len(), 1, "finds the call");
        let call = &analysis.facts.calls[0];
        assert_eq!(call.callee, "foo", "callee is the bare name");
        assert!(call.is_simple_path, "bare call is a simple path");
        assert!(
            call.initializes_binding,
            "call on the right of an assignment initializes the binding"
        );
    });
    with_facts(CHAINED_INIT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses chained call");
        let chained = analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == "bar")
            .expect("has chained call");
        assert!(
            !chained.is_simple_path,
            "call through an argument list is not a simple path"
        );
    });
}

#[test]
fn ruby_receiver_root_is_none_through_an_argument_list() {
    with_facts("foo(1).bar(x)\nsvc.run(x)\n", |analysis| {
        assert!(analysis.parse_error.is_none(), "parses chained calls");
        let chained = analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == "bar")
            .expect("has chained call");
        assert!(
            chained.callee_root.is_none(),
            "receiver-less call in the chain gives no bare root, got {:?}",
            chained.callee_root
        );
        let direct = analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == "svc.run")
            .expect("has direct receiver call");
        assert_eq!(
            direct.callee_root,
            Some("svc"),
            "direct receiver keeps its bare root"
        );
    });
}

#[test]
fn ruby_named_targets_initialize_a_binding() {
    for (source, expected) in [
        ("total = build(1)\n", true),
        ("TABLE = build(1)\n", true),
        ("@repo = build(1)\n", true),
        ("cfg[:opts] = build(1)\n", false),
        ("obj.attr = build(1)\n", false),
    ] {
        with_facts(source, |analysis| {
            assert!(analysis.parse_error.is_none(), "parses {source}");
            let build = analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == "build")
                .expect("has build call");
            assert_eq!(
                build.initializes_binding, expected,
                "{source} initializes {expected}"
            );
        });
    }
}
