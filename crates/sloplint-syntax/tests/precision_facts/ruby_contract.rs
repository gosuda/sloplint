use sloplint_core::facts::{AssertShape, Visibility};

use super::ruby::with_facts;

const PRIVATE_SYM: &str = "def helper(x, y = true)\nend\nprivate :helper\nprotected\ndef score(a, b, c)\nend\nprotected :score\n";

#[test]
fn ruby_private_symbol_marks_earlier_def() {
    with_facts(PRIVATE_SYM, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses markers");
        let visibility_of = |name: &str| {
            analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == name)
                .map_or_else(|| panic!("has fn {name}"), |f| f.visibility)
        };
        assert_eq!(
            visibility_of("helper"),
            Visibility::Private,
            "private symbol marks the earlier def"
        );
        assert_eq!(
            visibility_of("score"),
            Visibility::Exported,
            "protected stays exported"
        );
        assert_eq!(
            visibility_of("score"),
            Visibility::Exported,
            "protected symbol keeps the export"
        );
    });
}
const CHAIN_CALLEE: &str = "logger.tagged('req').info(\"started\")\nlogger.info(\"started\")\n";

#[test]
fn ruby_chained_callee_drops_receiver_call_text() {
    with_facts(CHAIN_CALLEE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses chained logger");
        let chained = analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == "info")
            .expect("has trimmed callee");
        assert!(
            !chained.is_simple_path,
            "receiver call link is not a simple path"
        );
        let direct = analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == "logger.info")
            .expect("has direct callee");
        assert_eq!(
            direct.callee_root,
            Some("logger"),
            "direct receiver keeps its root"
        );
    });
}

const ROOTS: &str =
    "items[0].save(id)\n\"abc\".upcase\n(a + b).to_s\nFoo::Bar.baz(x)\n@repo.find(id)\n";

#[test]
fn ruby_receiver_roots_keep_bare_identifiers() {
    with_facts(ROOTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses receivers");
        let root_of = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .map_or_else(|| panic!("has call {callee}"), |c| c.callee_root)
        };
        assert_eq!(root_of("save"), None, "index root is None");
        assert_eq!(root_of("upcase"), None, "literal root is None");
        assert_eq!(root_of("to_s"), None, "parenthesized root is None");
        assert_eq!(
            root_of("Foo::Bar.baz"),
            Some("Foo"),
            "scope root keeps the leftmost identifier"
        );
        assert_eq!(
            root_of("@repo.find"),
            Some("repo"),
            "sigil is stripped from the root"
        );
    });
}

const PATHS: &str = "def build(x)\n  Foo::Bar.baz(x)\nend\ndef go(x)\n  a.b.c(x)\nend\ndef newed(x)\n  Foo.new(x)\nend\n";

#[test]
fn ruby_bare_chains_and_new_are_simple_paths() {
    with_facts(PATHS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses paths");
        let is_plain = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .map_or_else(|| panic!("has call {callee}"), |c| c.is_simple_path)
        };
        assert!(is_plain("Foo::Bar.baz"), "scope chain is a simple path");
        assert!(is_plain("a.b.c"), "bare member chain is a simple path");
        assert!(is_plain("Foo"), "named construction is a simple path");
    });
}

const NEW_DIRTY: &str = "Registry.lookup(key).new(x)\nFoo.new(x)\n";

#[test]
fn ruby_new_through_call_args_drops_argument_text() {
    with_facts(NEW_DIRTY, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses constructions");
        let callee_of = |want: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == want)
                .map_or_else(|| panic!("has call {want}"), |c| c.callee)
        };
        assert_eq!(
            callee_of("new"),
            "new",
            "argument-carrying receiver trims to the method"
        );
        assert_eq!(
            callee_of("Foo"),
            "Foo",
            "named construction keeps its class"
        );
    });
}

const BARE_FWD: &str =
    "def greet(name)\n  render_greeting(name)\nend\ndef fetch(id)\n  store.fetch(id)\nend\n";

#[test]
fn ruby_bare_call_root_is_the_method_name() {
    with_facts(BARE_FWD, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses forwarders");
        let root_of = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .map_or_else(|| panic!("has call {callee}"), |c| c.callee_root)
        };
        assert_eq!(
            root_of("render_greeting"),
            Some("render_greeting"),
            "bare call roots at its method"
        );
        assert_eq!(
            root_of("store.fetch"),
            Some("store"),
            "receiver call keeps its root"
        );
    });
}

const CASE_MATCH: &str =
    "case status\nin Status::Active then 1\nin Status::Idle then 2\nin _ then 0\nend\n";

#[test]
fn ruby_pattern_wildcard_uses_sibling_variants() {
    with_facts(CASE_MATCH, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses case match");
        assert_eq!(
            analysis.facts.wildcard_arms.len(),
            1,
            "wildcard arm is the only arm"
        );
        let arm = &analysis.facts.wildcard_arms[0];
        assert!(
            arm.enum_like,
            "sibling variants make the wildcard enum-like"
        );
        assert_eq!(arm.variant_count, 2, "counts both sibling variants");
    });
}

const TYPE_DISPATCH: &str = "case value\nwhen Integer then 1\nwhen String then 2\nelse 0\nend\n";

#[test]
fn ruby_type_dispatch_else_is_not_enum_like() {
    with_facts(TYPE_DISPATCH, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses type dispatch");
        assert_eq!(
            analysis.facts.wildcard_arms.len(),
            1,
            "else arm is the only wildcard"
        );
        let arm = &analysis.facts.wildcard_arms[0];
        assert!(!arm.enum_like, "two classes are not enum variants");
        assert_eq!(arm.variant_count, 2, "both classes still count as variants");
    });
}

const MULTI_PATTERN: &str = "case s\nwhen Status::A, Status::B then 1\nelse 0\nend\n";

#[test]
fn ruby_multi_pattern_when_counts_all_variants() {
    with_facts(MULTI_PATTERN, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses multi-pattern when");
        assert_eq!(
            analysis.facts.wildcard_arms.len(),
            1,
            "else arm is the only wildcard"
        );
        let arm = &analysis.facts.wildcard_arms[0];
        assert!(arm.enum_like, "shared scope makes the arm enum-like");
        assert_eq!(
            arm.variant_count, 2,
            "both patterns count, got {}",
            arm.variant_count
        );
    });
}

const RESCUE_NIL: &str =
    "begin\n  work\nrescue\n  nil\nend\nbegin\n  work\nrescue\n  log(e)\nend\n";

#[test]
fn ruby_rescue_nil_is_an_empty_swallow() {
    with_facts(RESCUE_NIL, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses rescues");
        assert_eq!(analysis.facts.handlers.len(), 2, "finds both handlers");
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "nil body is an empty swallow"
        );
        assert!(
            !analysis.facts.handlers[1].stmts.is_empty(),
            "log body is not empty"
        );
    });
}

const RESCUE_MOD_NIL: &str = "value = Integer(s) rescue nil\n";

#[test]
fn ruby_rescue_modifier_nil_is_empty() {
    with_facts(RESCUE_MOD_NIL, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses rescue modifier");
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "finds the modifier handler"
        );
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "nil handler is empty"
        );
    });
}

const TRY_REQUIRE: &str =
    "begin\n  require \"optional_gem\"\nrescue LoadError\nend\nbegin\n  work\nrescue\nend\n";

#[test]
fn ruby_require_only_try_marks_imports_only() {
    with_facts(TRY_REQUIRE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses begin blocks");
        assert_eq!(analysis.facts.handlers.len(), 2, "finds both handlers");
        assert!(
            analysis.facts.handlers[0].try_imports_only,
            "require-only try is imports-only"
        );
        assert!(
            !analysis.facts.handlers[1].try_imports_only,
            "work try is not imports-only"
        );
        assert!(
            !analysis.facts.handlers[0].in_cleanup,
            "top-level handler is not cleanup"
        );
    });
}

const CLEANUP: &str = "def close\n  begin\n    work\n  rescue\n  end\nend\ndef load\n  begin\n    work\n  rescue\n  end\nend\n";

#[test]
fn ruby_rescue_in_close_is_cleanup() {
    with_facts(CLEANUP, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses methods");
        assert_eq!(analysis.facts.handlers.len(), 2, "finds both handlers");
        assert!(
            analysis.facts.handlers[0].in_cleanup,
            "rescue in close is cleanup"
        );
        assert!(
            !analysis.facts.handlers[1].in_cleanup,
            "rescue in load is not cleanup"
        );
    });
}

const DEFAULTS: &str = "def f(x = true, y = false, z = true)\nend\ndef connect(host, secure: false, verify: true)\nend\ndef log(level, *args, **opts, &blk)\nend\n";

#[test]
fn ruby_params_carry_defaults_and_variadics() {
    with_facts(DEFAULTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses params");
        let params_of = |name: &str| {
            analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == name)
                .map_or_else(|| panic!("has fn {name}"), |f| f.params.clone())
        };
        let f = params_of("f");
        assert!(
            f.iter().all(|p| p.has_default && p.bool_default),
            "optional params carry defaults"
        );
        let connect = params_of("connect");
        assert!(!connect[0].keyword_only, "positional host stays positional");
        assert!(
            connect[1..].iter().all(|p| p.keyword_only && p.has_default),
            "keyword params are keyword-only with defaults"
        );
        let log = params_of("log");
        assert_eq!(
            log.len(),
            2,
            "block and hash splat leave the positional list"
        );
        assert!(log[1].is_variadic, "splat is variadic");
    });
}

const PROTOCOL: &str = "class Matrix\n  def size(rows, cols, fill: false)\n  end\nend\n";

#[test]
fn ruby_ordinary_size_is_not_signature_forced() {
    with_facts(PROTOCOL, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses matrix");
        let size = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == "size")
            .expect("has size");
        assert!(!size.signature_forced, "user size signature is actionable");
    });
}

const PROTECTED: &str = "protected\ndef score(a, b, c)\nend\n";

#[test]
fn ruby_protected_method_is_exported() {
    with_facts(PROTECTED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses marker");
        let score = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == "score")
            .expect("has score");
        assert_eq!(
            score.visibility,
            Visibility::Exported,
            "protected is exported like php"
        );
    });
}

const NESTED_ASSERT: &str = "assert(total > 0 && label == \"x\")\nassert(a == b)\n";

#[test]
fn ruby_nested_comparison_is_not_top_eq_ne() {
    with_facts(NESTED_ASSERT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses asserts");
        assert_eq!(analysis.facts.assertions.len(), 2, "finds both asserts");
        assert_eq!(
            analysis.facts.assertions[0].shape,
            AssertShape::Other,
            "top && stays Other"
        );
        assert_eq!(
            analysis.facts.assertions[1].shape,
            AssertShape::TopEqNe,
            "top == is TopEqNe"
        );
    });
}

const KIND_ASSERT: &str = "assert_kind_of String, value\nassert_equal 1, value\n";

#[test]
fn ruby_kind_assertion_is_not_top_eq_ne() {
    with_facts(KIND_ASSERT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses kind asserts");
        assert_eq!(analysis.facts.assertions.len(), 2, "finds both asserts");
        assert_eq!(
            analysis.facts.assertions[0].shape,
            AssertShape::Other,
            "kind check stays Other"
        );
        assert_eq!(
            analysis.facts.assertions[1].shape,
            AssertShape::TopEqNe,
            "equality check is TopEqNe"
        );
    });
}
