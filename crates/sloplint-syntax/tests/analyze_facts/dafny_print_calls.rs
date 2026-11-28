//! Dafny `print` is program output, not a log call.
//!
//! `print` statements still surface a `print` call fact (they are calls),
//! but the default log-callee set for Dafny must not match it: every
//! pretty-printer and test-oracle `print` would otherwise report as
//! unsearchable logging. The test pins both sides, so neither a lost call
//! fact nor a re-added `print` callee passes silently.

use sloplint_core::config::Config;
use sloplint_core::lang::Lang;
use sloplint_core::rules::Settings;

use crate::support::run_clean;

const PRINTER: &str = "method Greet() {\n  print \"hi\";\n  print prefix, name, \"\\n\";\n}\n";

#[test]
fn dafny_print_calls_match_no_default_log_callee() {
    let analysis = run_clean(Lang::Dafny, "greet.dfy", PRINTER);
    let prints: Vec<&str> = analysis
        .facts
        .calls
        .iter()
        .filter(|call| call.callee == "print")
        .map(|call| call.callee)
        .collect();
    assert_eq!(
        prints.len(),
        2,
        "both print statements must surface print call facts, found {:?}",
        analysis
            .facts
            .calls
            .iter()
            .map(|call| call.callee)
            .collect::<Vec<_>>()
    );
    let settings =
        Settings::from_config(&Config::default()).expect("default config builds settings");
    let patterns = settings
        .log_callees
        .get(Lang::Dafny.config_key())
        .expect("default config names Dafny log callees");
    for call in &analysis.facts.calls {
        assert!(
            patterns
                .iter()
                .all(|pattern| !pattern.matches(call.callee, call.kind, Lang::Dafny)),
            "dafny call `{}` must match no default log callee",
            call.callee
        );
    }
}
