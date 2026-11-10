use sloplint_core::facts::{AssertShape, CallKind, ImportKind, ImportScope, Stmt, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

const FUNS: &str = r#"helper <- function(x) {
  x
}

.cache <- function(x) {
  x
}
"#;

const CALL_INIT: &str = r#"y <- f(1, "a")
"#;

const IMPORTS: &str = r#"library("ggplot2")
"#;

const NO_IMPORT: &str = r#"y <- base::list(1)
"#;

const STDLIB_IMPORT: &str = r#"library("stats")
"#;

const HANDLER: &str = r#"tryCatch(stop("bad"), error = function(e) {
  print("x")
})
"#;

const NO_HANDLER: &str = r#"tryCatch(x)
"#;

const ASSERTS: &str = r#"expect_equal(a, b)
stopifnot(ok)
stopifnot(x == y)
"#;

fn with_facts(source: &str, mut check: impl FnMut(&Analysis<'_>)) {
    let custom = match CustomRules::compile(&[]) {
        Ok(custom) => custom,
        Err(err) => panic!("empty custom rules compile: {err:?}"),
    };
    let input = FileInput {
        lang: Lang::R,
        path: "case.R",
        source,
        role: CodeRole::Library,
    };
    check(&analyze(&mut Parsers::new(), &input, &custom));
}

#[test]
fn dot_prefix_makes_function_private() {
    with_facts(FUNS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses bound functions");
        assert_eq!(
            analysis.facts.functions.len(),
            2,
            "finds both bound functions"
        );
        let visibility = |name: &str| {
            analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == name)
                .map_or_else(|| panic!("has function {name}"), |f| f.visibility)
        };
        assert_eq!(
            visibility("helper"),
            Visibility::Exported,
            "plain name is exported"
        );
        assert_eq!(
            visibility(".cache"),
            Visibility::Private,
            "dot-prefixed name is private"
        );
    });
}

#[test]
fn function_head_facts_stay_default() {
    with_facts(FUNS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses bound functions");
        let helper = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == "helper")
            .unwrap_or_else(|| panic!("has function helper"));
        assert!(!helper.signature_forced, "r signature stays unforced");
        assert!(!helper.entry_point, "r entry point stays false");
        assert!(!helper.in_test, "r test marking stays false");
        assert!(!helper.is_overload, "r overload stays false");
        assert!(!helper.is_deprecated, "r deprecation stays false");
        assert!(!helper.is_decorated, "r decoration stays false");
        assert!(!helper.is_trait_item, "r trait item stays false");
        assert_eq!(helper.params.len(), 1, "keeps the single parameter");
        assert_eq!(helper.params[0].name, "x", "keeps the parameter name");
        assert!(
            helper.params[0].primitive_kind == sloplint_core::facts::PrimitiveKind::Other,
            "r primitive kind stays Other"
        );
        assert!(!helper.params[0].has_default, "r default flag stays false");
        assert!(!helper.params[0].keyword_only, "r keyword flag stays false");
        assert!(!helper.params[0].is_variadic, "r variadic flag stays false");
    });
}

#[test]
fn r_bare_calls_report_simple_paths() {
    with_facts(CALL_INIT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses call binding");
        assert_eq!(analysis.facts.calls.len(), 1, "finds the call");
        let call = &analysis.facts.calls[0];
        assert_eq!(call.callee, "f", "callee is the bare name");
        assert_eq!(call.kind, CallKind::Plain, "call kind stays plain");
        assert!(call.is_simple_path, "bare name is a simple path");
        assert!(
            !call.initializes_binding,
            "r initializer fact stays default"
        );
    });
    with_facts("(function(x) x)(1)\n", |analysis| {
        assert!(
            analysis.parse_error.is_none(),
            "parses immediate invocation"
        );
        assert_eq!(analysis.facts.calls.len(), 1, "finds the call");
        assert!(
            !analysis.facts.calls[0].is_simple_path,
            "computed callee is not a simple path"
        );
    });
}

#[test]
fn r_library_calls_report_nonstdlib_roots() {
    with_facts(IMPORTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses library call");
        assert_eq!(analysis.facts.imports.len(), 1, "finds the import");
        let import = &analysis.facts.imports[0];
        assert_eq!(&*import.module, "ggplot2", "keeps full module path");
        assert_eq!(import.kind, ImportKind::Wildcard, "library is a wildcard");
        assert_eq!(
            import.scope,
            ImportScope::Module,
            "top-level import has module scope"
        );
        assert!(!import.in_test, "r test scope stays false");
        assert!(!import.is_reexport, "r reexport stays false");
        assert_eq!(
            import.external_root,
            Some("ggplot2"),
            "third-party library names its root"
        );
    });
    with_facts(STDLIB_IMPORT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses stdlib library call");
        assert_eq!(analysis.facts.imports.len(), 1, "finds the import");
        assert!(
            analysis.facts.imports[0].external_root.is_none(),
            "stdlib library names no external root"
        );
    });
    with_facts(NO_IMPORT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses namespaced call");
        assert!(
            analysis.facts.imports.is_empty(),
            "namespaced call emits no import"
        );
    });
}

#[test]
fn trycatch_handler_flags_stay_default() {
    with_facts(HANDLER, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses tryCatch");
        assert_eq!(analysis.facts.handlers.len(), 1, "finds the handler");
        let handler = &analysis.facts.handlers[0];
        assert_eq!(
            handler.stmts,
            vec![Stmt::Call {
                callee: "print",
                kind: CallKind::Plain,
            }],
            "handler keeps the call statement"
        );
        assert!(!handler.has_else, "r else flag stays false");
        assert!(!handler.try_imports_only, "r imports-only flag stays false");
        assert!(!handler.in_cleanup, "r cleanup flag stays false");
    });
    with_facts(NO_HANDLER, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses bare tryCatch");
        assert!(
            analysis.facts.handlers.is_empty(),
            "tryCatch without error arg has no handler"
        );
    });
}

#[test]
fn r_stopifnot_shapes_split_bare_name_and_top_eq_ne() {
    with_facts(ASSERTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses assertions");
        assert_eq!(analysis.facts.assertions.len(), 3, "finds all assertions");
        let equal = &analysis.facts.assertions[0];
        assert!(equal.comparison, "expect_equal compares");
        assert_eq!(
            equal.shape,
            AssertShape::Other,
            "expect_equal keeps the default shape"
        );
        assert!(
            !equal.in_typecheck,
            "expect_equal is not under type checking"
        );
        assert!(
            !equal.value_is_bool_literal,
            "expect_equal value is not a bool literal"
        );
        let bare = &analysis.facts.assertions[1];
        assert!(!bare.comparison, "stopifnot does not compare");
        assert_eq!(
            bare.shape,
            AssertShape::BareName,
            "bare identifier is a bare name"
        );
        let compared = &analysis.facts.assertions[2];
        assert!(!compared.comparison, "stopifnot still does not compare");
        assert_eq!(
            compared.shape,
            AssertShape::TopEqNe,
            "top-level equality is top eq-ne"
        );
    });
}
