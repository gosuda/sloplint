//! PHP call sites keep their callee as a borrowed source slice (`$this->logger->info`,
//! `\App\Log::info`, `\trigger_error`); the callee matcher in `sloplint-core` is what
//! makes those slices comparable with configured `log_callees` patterns.

use sloplint_core::callee::CalleePattern;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"<?php
final class Orders
{
    public function place(string $id): void
    {
        $this->logger->info($id);
        $this->logger?->warning($id);
        \trigger_error($id);
        \App\Log::info($id);
        error_log($id);
    }
}
function boom($id)
{
    throw new \RuntimeException("order failed");
}
"#;

fn matches_on_line(line: u32, pattern: &str) -> (bool, String) {
    let pattern = CalleePattern::parse(pattern).expect("pattern parses");
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Php,
        path: "case.php",
        source: SOURCE,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.php parses");
    let at_line: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.pos.line == line)
        .collect();
    assert_eq!(at_line.len(), 1, "one call on line {line}");
    let matched = at_line
        .iter()
        .any(|call| pattern.matches(call.callee, call.kind));
    let observed = at_line.iter().map(|call| call.callee).collect::<Vec<_>>();
    (matched, observed.join(" | "))
}

#[test]
fn property_chain_callee_matches_dotted_pattern() {
    let (matched, callee) = matches_on_line(6, "logger.info");
    assert!(
        matched,
        "pattern `logger.info` must match the property-chain callee `{callee}` on line 6"
    );
}

#[test]
fn nullsafe_callee_matches_dotted_pattern() {
    let (matched, callee) = matches_on_line(7, "logger.warning");
    assert!(
        matched,
        "pattern `logger.warning` must match the nullsafe callee `{callee}` on line 7"
    );
}

#[test]
fn leading_backslash_function_matches_bare_pattern() {
    let (matched, callee) = matches_on_line(8, "trigger_error");
    assert!(
        matched,
        "pattern `trigger_error` must match the root-namespace callee `{callee}` on line 8"
    );
}

#[test]
fn namespaced_static_callee_matches_class_pattern() {
    let (matched, callee) = matches_on_line(9, "Log::info");
    assert!(
        matched,
        "pattern `Log::info` must match the namespaced static callee `{callee}` on line 9"
    );
}

#[test]
fn namespaced_constructor_matches_new_pattern() {
    let (matched, callee) = matches_on_line(15, "new *Exception");
    assert!(
        matched,
        "pattern `new *Exception` must match the namespaced constructor `{callee}` on line 15"
    );
}

#[test]
fn unrelated_pattern_does_not_match_property_chain() {
    let (matched, callee) = matches_on_line(6, "logger.warning");
    assert!(
        !matched,
        "pattern `logger.warning` must not match the callee `{callee}` on line 6"
    );
}
