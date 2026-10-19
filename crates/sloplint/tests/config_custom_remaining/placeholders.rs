//! Custom rule messages: malformed `{name}` placeholders are configuration errors,
//! and `{{` / `}}` escapes survive to the rendered finding.

use sloplint_core::config::parse_config;
use sloplint_core::lang::Lang;
use sloplint_core::rules::custom::{PlaceholderError, message_placeholders};
use sloplint_core::{ConfigError, CustomRuleSpec, RuleFilter, RuleSet, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const QUERY: &str = "(call_expression function: (identifier) @m) @match";

fn rule_src(message: &str) -> String {
    format!(
        "[[custom_rules]]\nid = \"custom/x\"\nlanguage = \"rust\"\nseverity = \"warn\"\nmessage = {message:?}\nwhy = \"w\"\nfix = \"f\"\nquery = {QUERY:?}\n"
    )
}

fn spec(message: &str) -> CustomRuleSpec {
    CustomRuleSpec {
        id: "custom/x".to_string(),
        language: "rust".to_string(),
        severity: Severity::Warn,
        message: message.to_string(),
        why: "w".to_string(),
        fix: "f".to_string(),
        query: QUERY.to_string(),
        include: Vec::new(),
        exclude: Vec::new(),
    }
}

fn custom_rule_error(error: &ConfigError, fragment: &str, context: &str) {
    let ConfigError::CustomRule { id, message } = error else {
        panic!("{context}: expected a custom rule error, got: {error}");
    };
    assert_eq!(id, "custom/x", "{context}: error names the wrong rule");
    assert!(
        message.contains(fragment),
        "{context}: `{message}` lacks `{fragment}`"
    );
}

const MALFORMED: [(&str, &str); 6] = [
    ("x {name", "unclosed `{`"),
    ("{m} then {oops", "unclosed `{`"),
    ("{", "unclosed `{`"),
    ("{{{m", "unclosed `{`"),
    ("x {}", "empty placeholder"),
    ("{m}{}", "empty placeholder"),
];

#[test]
fn malformed_placeholders_are_rejected_when_the_config_loads() {
    for (message, fragment) in MALFORMED {
        let Err(error) = parse_config(&rule_src(message), "sloplint.toml") else {
            panic!("message {message:?} must be rejected");
        };
        custom_rule_error(&error, fragment, message);
    }
}

#[test]
fn malformed_placeholders_are_rejected_when_queries_compile() {
    for (message, fragment) in MALFORMED {
        let Err(error) = CustomRules::compile(&[spec(message)]) else {
            panic!("message {message:?} must be rejected by the query compiler");
        };
        custom_rule_error(&error, fragment, message);
    }
}

#[test]
fn the_error_names_the_offset_of_the_bad_brace() {
    let Err(error) = parse_config(&rule_src("ab {m} cd {oops"), "sloplint.toml") else {
        panic!("an unclosed brace must be rejected");
    };
    custom_rule_error(&error, "at byte 10", "offset");
}

#[test]
fn placeholder_names_skip_escaped_braces() {
    assert_eq!(
        message_placeholders("{a} {{x}} {b}"),
        Ok(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(message_placeholders("{{m}}"), Ok(Vec::new()));
}

#[test]
fn placeholder_errors_are_typed_with_the_brace_offset() {
    assert_eq!(
        message_placeholders("x {name"),
        Err(PlaceholderError::Unclosed { offset: 2 })
    );
    assert_eq!(
        message_placeholders("{a} {{ {b"),
        Err(PlaceholderError::Unclosed { offset: 7 })
    );
    assert_eq!(
        message_placeholders("x {}"),
        Err(PlaceholderError::Empty { offset: 2 })
    );
}

fn render(message: &str, source: &str) -> Vec<String> {
    let config = parse_config(&rule_src(message), "sloplint.toml")
        .unwrap_or_else(|e| panic!("valid config rejected: {e}"));
    let custom = CustomRules::compile(&config.custom_rules)
        .unwrap_or_else(|e| panic!("valid rule failed to compile: {e}"));
    let rules = RuleSet::new(&config, &RuleFilter::default())
        .unwrap_or_else(|e| panic!("valid rules rejected: {e}"));
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/a.rs",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut parsers, &input, &custom);
    rules
        .custom_findings(&analysis.facts, &analysis.custom_hits)
        .into_iter()
        .map(|finding| finding.message)
        .collect()
}

#[test]
fn escaped_braces_and_captures_render_together() {
    let source = "fn main() { first(1); }\n";
    assert_eq!(render("lit {{braces}} {m}", source), ["lit {braces} first"]);
    assert_eq!(render("{{m}} is {m}", source), ["{m} is first"]);
    assert_eq!(
        render("a } lone close brace", source),
        ["a } lone close brace"]
    );
}
