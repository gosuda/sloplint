use sloplint_core::facts::{
    ArgShape, AssertShape, CallKind, ImportKind, ImportScope, Stmt, Visibility,
};
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

const NESTED: &str = r#"outer <- function(x) {
  helper <- function(verbose = TRUE, quiet = FALSE) x
  helper(TRUE)
}
"#;

const NAMESPACED: &str = r#"stats::median(x)
"#;

const LOCAL_IMPORT: &str = r#"f <- function() {
  library(dplyr)
}
"#;

const DOTS: &str = r#"g <- function(..., verbose = FALSE) NULL
h <- function(x, y, ...) NULL
"#;

const DEFAULTS: &str = r#"f <- function(x, verbose = TRUE, debug = FALSE, quiet = TRUE) NULL
"#;

fn named_function<'s>(
    analysis: &'s Analysis<'s>,
    name: &str,
) -> &'s sloplint_core::facts::Function<'s> {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("has function {name}"))
}

#[test]
fn nested_bound_functions_report_private() {
    with_facts(NESTED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses nested closure");
        assert_eq!(
            named_function(analysis, "helper").visibility,
            Visibility::Private,
            "helper nested in outer is not visible outside it"
        );
        assert_eq!(
            named_function(analysis, "outer").visibility,
            Visibility::Exported,
            "top-level binding stays exported"
        );
    });
}

#[test]
fn namespaced_calls_report_simple_paths() {
    with_facts(NAMESPACED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses namespaced call");
        assert_eq!(analysis.facts.calls.len(), 1, "finds the call");
        let call = &analysis.facts.calls[0];
        assert_eq!(
            call.callee, "stats::median",
            "keeps the qualified callee text"
        );
        assert_eq!(call.callee_root, Some("stats"), "root is the package name");
        assert!(call.is_simple_path, "qualified name is a simple path");
    });
    with_facts(CALL_INIT, |analysis| {
        let call = &analysis.facts.calls[0];
        assert_eq!(call.callee_root, Some("f"), "bare callee keeps its root");
        assert!(call.is_simple_path, "bare callee stays a simple path");
    });
}

#[test]
fn function_local_imports_report_function_scope() {
    with_facts(LOCAL_IMPORT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses local library call");
        assert_eq!(analysis.facts.imports.len(), 1, "finds the import");
        assert_eq!(
            analysis.facts.imports[0].scope,
            ImportScope::FunctionLocal,
            "library in a function body is function-local"
        );
    });
    with_facts(IMPORTS, |analysis| {
        assert_eq!(
            analysis.facts.imports[0].scope,
            ImportScope::Module,
            "top-level library keeps module scope"
        );
    });
}

#[test]
fn defaulted_params_report_has_default() {
    with_facts(DEFAULTS, |analysis| {
        assert!(
            analysis.parse_error.is_none(),
            "parses defaulted parameters"
        );
        let f = named_function(analysis, "f");
        assert_eq!(f.params.len(), 4, "keeps every parameter");
        assert!(!f.params[0].has_default, "x stays default-free");
        for param in &f.params[1..] {
            assert!(param.has_default, "{} reports its default", param.name);
        }
        assert!(
            f.params[1].primitive_kind == sloplint_core::facts::PrimitiveKind::Bool,
            "TRUE default keeps the bool kind"
        );
    });
    with_facts(FUNS, |analysis| {
        assert!(
            !named_function(analysis, "helper").params[0].has_default,
            "plain parameter keeps no default"
        );
    });
}

#[test]
fn dots_parameter_reports_variadic_and_keyword_only() {
    with_facts(DOTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses dots parameters");
        let g = named_function(analysis, "g");
        assert!(g.params[0].is_variadic, "dots parameter is variadic");
        assert!(!g.params[0].keyword_only, "dots itself is not keyword-only");
        assert!(
            g.params[1].keyword_only,
            "parameter after dots is keyword-only"
        );
        assert!(!g.params[1].is_variadic, "named parameter is not variadic");
        let h = named_function(analysis, "h");
        assert_eq!(h.params.len(), 3, "keeps every parameter");
        assert!(!h.params[0].is_variadic, "plain parameter is not variadic");
        assert!(
            !h.params[0].keyword_only,
            "parameter before dots stays positional"
        );
        assert!(h.params[2].is_variadic, "trailing dots is variadic");
    });
}

const CHAINED: &str = r#"f <- function(x) f2(x)(x)
g <- function(x) make_client(x)$connect(x)
"#;

#[test]
fn chained_applications_report_names_without_arguments() {
    with_facts(CHAINED, |analysis| {
        assert!(
            analysis.parse_error.is_none(),
            "parses chained applications"
        );
        assert_eq!(analysis.facts.calls.len(), 4, "finds all applications");
        let applied = analysis
            .facts
            .calls
            .iter()
            .find(|call| !call.is_simple_path && call.callee == "f2")
            .unwrap_or_else(|| panic!("has the outer application"));
        assert_eq!(
            applied.callee_root, None,
            "call-result callee has no path root"
        );
        let member = analysis
            .facts
            .calls
            .iter()
            .find(|call| call.callee == "connect")
            .unwrap_or_else(|| panic!("has the member application"));
        assert_eq!(
            member.callee_root, None,
            "member on a call result has no path root"
        );
        assert!(
            !member.is_simple_path,
            "member on a call result is not a simple path"
        );
        let factory = analysis
            .facts
            .calls
            .iter()
            .find(|call| call.is_simple_path)
            .unwrap_or_else(|| panic!("has the inner application"));
        assert_eq!(factory.callee, "f2", "inner application keeps its name");
        assert_eq!(
            factory.callee_root,
            Some("f2"),
            "inner application keeps its root"
        );
    });
}

const CONCATS: &str = r#"log("failed to download archive " + path)
log("ok " + path)
log(path + "")
"#;

const ARGUMENTS: &str = r#"run(c(1, 2))
make(list(key = 1))
take(function(x) x)
hold({
  x
})
keep(path)
"#;

#[test]
fn concatenations_carry_static_run_lengths() {
    with_facts(CONCATS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses concatenations");
        assert_eq!(analysis.facts.calls.len(), 3, "finds all three calls");
        assert!(
            matches!(
                analysis.facts.calls[0].args[0].shape,
                ArgShape::Concat { longest_static: 27 }
            ),
            "twenty-seven static characters measure twenty-seven"
        );
        assert!(
            matches!(
                analysis.facts.calls[1].args[0].shape,
                ArgShape::Concat { longest_static: 3 }
            ),
            "three static characters measure three"
        );
        assert!(
            matches!(
                analysis.facts.calls[2].args[0].shape,
                ArgShape::Concat { longest_static: 0 }
            ),
            "empty static operand measures zero"
        );
    });
}

#[test]
fn constructor_calls_stay_other_and_blocks_are_code_body() {
    with_facts(ARGUMENTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses shaped arguments");
        let shape = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|call| call.callee == callee)
                .unwrap_or_else(|| panic!("has the {callee} call"))
                .args[0]
                .shape
                .clone()
        };
        assert!(
            matches!(shape("run"), ArgShape::Other),
            "nested constructor call stays other"
        );
        assert!(
            matches!(shape("make"), ArgShape::Other),
            "keyed constructor call stays other"
        );
        assert!(
            matches!(shape("take"), ArgShape::CodeBody),
            "closure argument is a code body"
        );
        assert!(
            matches!(shape("hold"), ArgShape::CodeBody),
            "block argument is a code body"
        );
        assert!(
            matches!(shape("keep"), ArgShape::Ident("path")),
            "plain identifier keeps its name"
        );
    });
}
