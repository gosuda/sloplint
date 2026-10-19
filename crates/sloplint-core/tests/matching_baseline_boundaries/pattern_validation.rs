use sloplint_core::CalleePattern;
use sloplint_core::facts::CallKind;

fn rejection(pattern: &str) -> String {
    match CalleePattern::parse(pattern) {
        Ok(_) => panic!("pattern {pattern:?} must be rejected"),
        Err(message) => message,
    }
}

fn matches_plain(pattern: &str, callee: &str) -> bool {
    CalleePattern::parse(pattern)
        .unwrap_or_else(|error| panic!("pattern {pattern:?} must parse: {error}"))
        .matches(callee, CallKind::Plain)
}

#[test]
fn star_is_valid_only_as_first_character_of_last_segment() {
    for pattern in [
        "*Foo.bar",
        "a*b",
        "a*b.log",
        "foo.b*",
        "foo.*bar*",
        "**Error",
        "*.log",
        "a.*b.c",
        "new *Foo.bar",
    ] {
        let message = rejection(pattern);
        assert!(
            message.contains(pattern),
            "error for {pattern:?} must name the pattern, got {message:?}"
        );
    }
}

#[test]
fn interior_whitespace_is_rejected_and_names_the_pattern() {
    for pattern in [
        "console . log",
        "console.\tlog",
        "console.lo\u{a0}g",
        "console\u{3000}.log",
        "new *Er ror",
        "a b",
    ] {
        let message = rejection(pattern);
        assert!(
            message.contains(pattern),
            "error for {pattern:?} must name the pattern, got {message:?}"
        );
        assert!(
            message.contains("whitespace"),
            "error for {pattern:?} must explain the whitespace rule, got {message:?}"
        );
    }
}

#[test]
fn question_dot_cannot_appear_inside_a_pattern() {
    for pattern in ["a?.b", "console?.log", "x.empty?.size"] {
        let message = rejection(pattern);
        assert!(
            message.contains(pattern),
            "error for {pattern:?} must name the pattern, got {message:?}"
        );
    }
    assert!(
        matches_plain("a.b", "a?.b"),
        "the separator form is written `.` in the pattern and `?.` in the callee"
    );
}

#[test]
fn structural_errors_still_name_the_pattern() {
    for pattern in [
        "", "   ", "new", "new ", "a..b", ".lead", "trail.", "*", "a.*",
    ] {
        let message = rejection(pattern);
        assert!(
            message.contains(pattern) || pattern.trim().is_empty(),
            "error for {pattern:?} must name the pattern, got {message:?}"
        );
    }
}

#[test]
fn rejected_shapes_have_working_equivalents() {
    assert!(
        matches_plain("console.log", "console . log"),
        "callee whitespace must not change segment matching"
    );
    assert!(
        matches_plain("*Error", "TypeError"),
        "a final suffix pattern must match TypeError"
    );
    assert!(
        matches_plain("x.*Error", "x.TypeError"),
        "a qualified suffix must match in the same namespace"
    );
    assert!(
        !matches_plain("x.*Error", "TypeError"),
        "a required namespace must not disappear"
    );
}

#[test]
fn valid_shapes_parse_and_match() {
    let macro_pattern = CalleePattern::parse("info!").expect("info! is valid");
    assert!(
        macro_pattern.matches("tracing::info", CallKind::Macro),
        "macro calls must receive their implicit exclamation mark"
    );
    assert!(
        macro_pattern.matches("tracing::info!", CallKind::Macro),
        "an explicit macro mark must not be duplicated"
    );
    assert!(
        !macro_pattern.matches("tracing::info", CallKind::Plain),
        "a plain function must not match a macro pattern"
    );

    assert!(
        matches_plain("logger.info", "self.logger.info"),
        "leading receiver segments may precede the pattern"
    );
    assert!(
        matches_plain("std::env::var", "std::env::var"),
        "namespace separators must match their own spelling"
    );
    assert!(
        matches_plain("foo.empty?", "foo.empty?"),
        "a terminal question mark is part of the method name"
    );
    assert!(
        !matches_plain("a.b", "a.bc"),
        "plain segments require exact names rather than prefixes"
    );
    assert!(
        !matches_plain("a.b", "b"),
        "the callee must contain every pattern segment"
    );

    let constructor = CalleePattern::parse("new *Error").expect("new *Error is valid");
    assert!(
        constructor.matches("TypeError", CallKind::New),
        "a constructor pattern must match a new expression"
    );
    assert!(
        !constructor.matches("TypeError", CallKind::Plain),
        "a constructor pattern must reject plain calls"
    );
}

#[test]
fn surrounding_whitespace_and_new_prefix_spacing_are_accepted() {
    for pattern in [
        "  new   *Error  ",
        "new\t*Error",
        "new\u{a0}*Error",
        "\u{3000}new *Error",
    ] {
        let parsed = CalleePattern::parse(pattern)
            .unwrap_or_else(|error| panic!("pattern {pattern:?} must parse: {error}"));
        assert!(parsed.matches("TypeError", CallKind::New), "{pattern:?}");
        assert!(!parsed.matches("TypeError", CallKind::Plain), "{pattern:?}");
    }
    assert!(
        matches_plain("  console.log ", "console.log"),
        "surrounding pattern whitespace must be ignored"
    );
    let newer = CalleePattern::parse("newer.make").expect("`newer` is an ordinary segment");
    assert!(
        newer.matches("newer.make", CallKind::Plain),
        "the constructor keyword must not consume a longer identifier"
    );
}
