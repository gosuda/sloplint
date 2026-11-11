use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

const OPTIONS_SAVE: &str = r#"f <- function(x) {
  old <- options(digits = 3)
  on.exit(options(old))
  x
}
"#;

const ENV_SET: &str = r#"configure <- function() {
  Sys.setenv(APP_MODE = "prod")
}
"#;

const GLOBAL_ASSIGN: &str = r#"counter <- 0
bump <- function() {
  counter <<- counter + 1
}
"#;

const LOCAL_ASSIGN: &str = r#"total <- 0
add <- function(x) {
  total <- total + x
  total
}
"#;

const MATCH_FUN_VALUE: &str = r#"apply_all <- function(FUN, x) {
  FUN <- match.fun(FUN)
  FUN(x)
}
"#;

const MATCH_FUN_COMPUTED: &str = r#"apply_all <- function(prefix, x) {
  FUN <- match.fun(paste0(prefix, "_impl"))
  FUN(x)
}
"#;

const GET_NAMED_COMPUTED: &str = r#"read_one <- function(key) {
  get(mode = "function", x = key)
}
"#;

const GET_NAMED_STATIC: &str = r#"read_one <- function() {
  get(mode = "function", x = "name")
}
"#;

const GET_POSITIONAL_STATIC: &str = r#"read_one <- function() {
  get("name", mode = "function")
}
"#;

const DO_CALL_NAMED_COMPUTED: &str = r#"build <- function(kind, parts) {
  do.call(args = list(parts), what = paste0("make_", kind))
}
"#;

const DO_CALL_STATIC_FORMS: &str = r#"combine <- function(parts) {
  do.call("rbind", list(parts))
  do.call(rbind, list(parts))
}
"#;

const DO_CALL_QUOTE_WITH_NAMED_WHAT: &str = r#"apply_it <- function(fname) {
  do.call(quote = TRUE, what = fname)
}
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
fn session_options_are_not_environment_writes() {
    with_facts(OPTIONS_SAVE, |analysis| {
        assert!(
            analysis.parse_error.is_none(),
            "parses options save and restore"
        );
        assert!(
            analysis.facts.ambient.iter().all(|site| {
                !(site.kind == AmbientKind::GlobalWrite && site.name == "os.environ")
            }),
            "options save and restore is not an environment write"
        );
    });
    with_facts(ENV_SET, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses setenv");
        assert!(
            analysis
                .facts
                .ambient
                .iter()
                .any(|site| site.kind == AmbientKind::GlobalWrite && site.name == "os.environ"),
            "setenv stays an environment write"
        );
    });
}

#[test]
fn superassignment_inside_functions_is_a_global_write() {
    with_facts(GLOBAL_ASSIGN, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses superassignment");
        let writes: Vec<_> = analysis
            .facts
            .ambient
            .iter()
            .filter(|site| site.kind == AmbientKind::GlobalWrite)
            .collect();
        assert_eq!(writes.len(), 1, "finds the superassignment write");
        assert_eq!(
            writes[0].name, "counter",
            "records the assigned global name"
        );
        assert!(
            writes[0].in_function.is_some(),
            "records the write inside a function"
        );
    });
    with_facts(LOCAL_ASSIGN, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses local assignment");
        assert!(
            analysis
                .facts
                .ambient
                .iter()
                .all(|site| site.kind != AmbientKind::GlobalWrite),
            "plain local assignment stays silent"
        );
    });
}

#[test]
fn function_valued_match_fun_is_not_computed() {
    with_facts(MATCH_FUN_VALUE, |analysis| {
        assert!(
            analysis.parse_error.is_none(),
            "parses function passthrough"
        );
        assert!(
            analysis.facts.dynamic_symbols.is_empty(),
            "a function value is not a computed name"
        );
    });
    with_facts(MATCH_FUN_COMPUTED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses a computed lookup");
        assert_eq!(
            analysis.facts.dynamic_symbols.len(),
            1,
            "keeps the genuinely computed lookup"
        );
        assert_eq!(
            analysis.facts.dynamic_symbols[0].what, "match.fun() with computed function name",
            "keeps the match.fun lookup description"
        );
    });
}

#[test]
fn named_get_lookup_argument_is_selected_over_other_formals() {
    with_facts(GET_NAMED_COMPUTED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses named get lookup");
        assert_eq!(
            analysis.facts.dynamic_symbols.len(),
            1,
            "x = key is the lookup argument even behind mode"
        );
        assert_eq!(
            analysis.facts.dynamic_symbols[0].what, "get() with computed name",
            "keeps the get lookup description"
        );
    });
    with_facts(GET_NAMED_STATIC, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses static named get");
        assert!(
            analysis.facts.dynamic_symbols.is_empty(),
            "a static x argument stays silent"
        );
    });
    with_facts(GET_POSITIONAL_STATIC, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses positional get");
        assert!(
            analysis.facts.dynamic_symbols.is_empty(),
            "the plain static form stays silent"
        );
    });
}

#[test]
fn named_do_call_what_is_selected_over_other_formals() {
    with_facts(DO_CALL_NAMED_COMPUTED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses named do.call");
        assert_eq!(
            analysis.facts.dynamic_symbols.len(),
            1,
            "a computed what is reported behind args"
        );
        assert_eq!(
            analysis.facts.dynamic_symbols[0].what, "do.call() with computed function name",
            "keeps the do.call lookup description"
        );
    });
    with_facts(DO_CALL_STATIC_FORMS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses static do.call");
        assert!(
            analysis.facts.dynamic_symbols.is_empty(),
            "string and identifier function names stay exempt"
        );
    });
    with_facts(DO_CALL_QUOTE_WITH_NAMED_WHAT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses quote and what");
        assert!(
            analysis.facts.dynamic_symbols.is_empty(),
            "quote is no longer mistaken for the function name"
        );
    });
}
