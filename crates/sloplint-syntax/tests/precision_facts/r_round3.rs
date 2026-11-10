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
