use super::*;
use proptest::prelude::*;

fn plain(pattern: &str, callee: &str) -> bool {
    plain_in(pattern, callee, Lang::Rust)
}

fn plain_in(pattern: &str, callee: &str, lang: Lang) -> bool {
    CalleePattern::parse(pattern)
        .unwrap()
        .matches(callee, CallKind::Plain, lang)
}

#[test]
fn matches_plan_examples() {
    assert!(
        CalleePattern::parse("info!").unwrap().matches(
            "tracing::info!",
            CallKind::Macro,
            Lang::Rust
        ),
        "info! should match tracing::info! macro"
    );
    assert!(
        CalleePattern::parse("info!").unwrap().matches(
            "tracing::info",
            CallKind::Macro,
            Lang::Rust
        ),
        "info! should match tracing::info without bang under Macro kind"
    );
    assert!(
        plain("logger.info", "self.logger.info"),
        "logger.info should match self.logger.info"
    );
    assert!(
        plain("*Error", "ValueError"),
        "*Error should match ValueError"
    );
    assert!(
        CalleePattern::parse("*Error")
            .unwrap()
            .matches("TypeError", CallKind::New, Lang::Rust),
        "*Error should match TypeError under New kind"
    );
    assert!(
        CalleePattern::parse("new *Error")
            .unwrap()
            .matches("TypeError", CallKind::New, Lang::Rust),
        "new *Error should match TypeError under New kind"
    );
    assert!(
        !CalleePattern::parse("new *Error").unwrap().matches(
            "ValueError",
            CallKind::Plain,
            Lang::Rust
        ),
        "new *Error must not match ValueError under Plain kind"
    );
    assert!(!plain("a.b", "a.bc"), "a.b must not match a.bc");
    assert!(!plain("a.b", "b"), "a.b must not match b");
    assert!(
        plain("console.log", "console . log"),
        "console.log should match whitespace-normalized callee"
    );
    assert!(plain("a.b", "x?.a.b"), "a.b should match x?.a.b");
    assert!(
        plain("std::env::var", "std::env::var"),
        "std::env::var should match itself"
    );
    assert!(
        !CalleePattern::parse("log")
            .unwrap()
            .matches("log", CallKind::Macro, Lang::Rust),
        "plain log must not match log under Macro kind without bang"
    );
}

#[test]
fn long_callee_without_matching_tail_rejects() {
    let pattern = CalleePattern::parse("logger.info").unwrap();
    let deep = format!("echo {}{}", "$(".repeat(20_000), ")".repeat(20_000));
    assert!(
        !pattern.matches(&deep, CallKind::Plain, Lang::Rust),
        "a callee whose tail is not the pattern's last segment cannot match"
    );
    let chain = format!("{}info", "x.".repeat(1_000));
    assert!(
        CalleePattern::parse("x.info")
            .unwrap()
            .matches(&chain, CallKind::Plain, Lang::Rust),
        "a callee ending in the pattern's segments still matches"
    );
    let bang = CalleePattern::parse("info!").unwrap();
    for callee in ["tracing::info!", "tracing::info"] {
        assert!(
            bang.matches(callee, CallKind::Macro, Lang::Rust),
            "info! still matches {callee} under Macro"
        );
    }
    assert!(
        !bang.matches("tracing::debug!", CallKind::Macro, Lang::Rust),
        "info! still rejects a non-matching macro callee"
    );
}

#[test]
fn nested_callees_reject_every_pattern_in_linear_time() {
    let patterns = [
        "log",
        "info",
        "warn",
        "error",
        "debug",
        "trace",
        "logger.info",
        "console.log",
        "console.error",
        "tracing::info",
        "tracing::warn",
        "std::env::var",
        "*Error",
        "new *Error",
    ];
    let patterns: Vec<CalleePattern> = patterns
        .into_iter()
        .map(|p| CalleePattern::parse(p).unwrap())
        .collect();
    let small = nested_callee(1_000);
    let large = nested_callee(4_000);
    VISIBLE_CHECKS.with(|c| c.set(0));
    STRIP_STEPS.with(|c| c.set(0));
    for pattern in &patterns {
        assert!(
            !pattern.matches(&small, CallKind::Plain, Lang::Rust),
            "a nested callee matches no log pattern"
        );
    }
    let small_steps = VISIBLE_CHECKS.with(|c| c.get());
    let small_strip = STRIP_STEPS.with(|c| c.get());
    VISIBLE_CHECKS.with(|c| c.set(0));
    STRIP_STEPS.with(|c| c.set(0));
    for pattern in &patterns {
        assert!(
            !pattern.matches(&large, CallKind::Plain, Lang::Rust),
            "a nested callee matches no log pattern"
        );
    }
    let large_steps = VISIBLE_CHECKS.with(|c| c.get());
    let large_strip = STRIP_STEPS.with(|c| c.get());
    assert!(
        large_steps <= small_steps.max(1) * 2,
        "matching cost must stay flat across depths: {small_steps} vs {large_steps}"
    );
    assert!(
        large_strip <= small_strip.max(1) * 6,
        "the comment-strip scan must stay linear in depth: {small_strip} vs {large_strip}"
    );
    let multi = [
        CalleePattern::parse("logger.info").unwrap(),
        CalleePattern::parse("tracing::info").unwrap(),
    ];
    let dot_small = "a.".repeat(16) + "info";
    let dot_large = "a.".repeat(64) + "info";
    let colon_small = "a::".repeat(16) + "info";
    let colon_large = "a::".repeat(64) + "info";
    VISIBLE_CHECKS.with(|c| c.set(0));
    for callee in [&dot_small, &colon_small] {
        for pattern in &multi {
            assert!(
                !pattern.matches(callee, CallKind::Plain, Lang::Rust),
                "a dotted callee matches no log pattern"
            );
        }
    }
    let dot_small_steps = VISIBLE_CHECKS.with(|c| c.get());
    assert!(
        dot_small_steps >= 16,
        "segment walk must exercise the counter: {dot_small_steps}"
    );
    VISIBLE_CHECKS.with(|c| c.set(0));
    for callee in [&dot_large, &colon_large] {
        for pattern in &multi {
            assert!(
                !pattern.matches(callee, CallKind::Plain, Lang::Rust),
                "a dotted callee matches no log pattern"
            );
        }
    }
    let dot_large_steps = VISIBLE_CHECKS.with(|c| c.get());
    assert!(
        dot_large_steps <= dot_small_steps.max(1) * 6,
        "segment walk must stay linear in depth: {dot_small_steps} vs {dot_large_steps}"
    );
}

fn nested_callee(depth: usize) -> String {
    let mut out = String::from("f(");
    for _ in 0..depth {
        out.push_str("$(");
    }
    out.push('x');
    for _ in 0..depth {
        out.push(')');
    }
    out
}

#[test]
fn rejects_malformed_patterns() {
    assert!(
        CalleePattern::parse("").is_err(),
        "empty pattern should be rejected"
    );
    assert!(
        CalleePattern::parse("   ").is_err(),
        "whitespace-only pattern should be rejected"
    );
    assert!(
        CalleePattern::parse("a..b").is_err(),
        "pattern a..b with empty segment should be rejected"
    );
    assert!(
        CalleePattern::parse(".lead").is_err(),
        "pattern .lead with leading dot should be rejected"
    );
    assert!(
        CalleePattern::parse("trail.").is_err(),
        "pattern trail. with trailing dot should be rejected"
    );
    assert!(
        CalleePattern::parse("*").is_err(),
        "lone star pattern should be rejected"
    );
    assert!(
        CalleePattern::parse("new ").is_err(),
        "dangling new prefix should be rejected"
    );
}

fn segment_vec(min: usize, max: usize) -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec("[a-z][a-z0-9]{0,5}", min..max)
}

proptest! {
    #[test]
    fn suffix_pattern_matches_prefixed_callee(
        p in segment_vec(1, 4),
        c in segment_vec(0, 4),
    ) {
        let mut pattern_segments = vec!["x".to_string()];
        pattern_segments.extend(p.iter().cloned());
        let pattern = CalleePattern::parse(&pattern_segments.join(".")).unwrap();
        let mut callee_segments = c;
        callee_segments.push("x".to_string());
        callee_segments.extend(p.iter().cloned());
        prop_assert!(pattern.matches(&callee_segments.join("."), CallKind::Plain, Lang::Rust));
        prop_assert!(pattern.matches(&callee_segments.join("."), CallKind::New, Lang::Rust));
    }

    #[test]
    fn exact_pattern_never_matches_changed_last_segment(p in segment_vec(1, 4)) {
        let pattern = CalleePattern::parse(&p.join(".")).unwrap();
        let mut changed = p.clone();
        let last = changed.len() - 1;
        changed[last].push('z');
        prop_assert!(!pattern.matches(&changed.join("."), CallKind::Plain, Lang::Rust));
    }

    #[test]
    fn segments_rev_reverses_forward_split(callee in "[a-z0-9:.>\\-?$ ]{0,60}") {
        let forward = forward_segments(&callee);
        let reverse: Vec<&str> = SegmentsRev::new(&callee).collect();
        prop_assert_eq!(
            reverse,
            forward.into_iter().rev().collect::<Vec<&str>>(),
            "backward walk must yield the forward split reversed"
        );
    }
}
/// The reference forward segment walk, kept only as a test oracle for
/// [`SegmentsRev`]: production matching consumes segments back to front.
fn forward_segments(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut cursor = start;
    loop {
        match next_visible(text, cursor) {
            Some((index, ch)) => match separator_end(text, index, ch) {
                Some(end) => {
                    out.push(&text[start..index]);
                    start = end;
                    cursor = end;
                }
                None => cursor = index + ch.len_utf8(),
            },
            None => {
                out.push(&text[start..]);
                return out;
            }
        }
    }
}

fn next_visible(text: &str, from: usize) -> Option<(usize, char)> {
    text[from..]
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(offset, ch)| (from + offset, ch))
}

fn eat(text: &str, from: usize, want: char) -> Option<usize> {
    match next_visible(text, from) {
        Some((index, ch)) if ch == want => Some(index + ch.len_utf8()),
        _ => None,
    }
}

fn separator_end(text: &str, index: usize, ch: char) -> Option<usize> {
    let after = index + ch.len_utf8();
    match ch {
        '.' | '\\' => Some(after),
        ':' => eat(text, after, ':'),
        '-' => eat(text, after, '>'),
        '?' => match next_visible(text, after) {
            Some((dot, '.')) => Some(dot + 1),
            Some((dash, '-')) => eat(text, dash + 1, '>'),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn segments_rev_pairs_colons_left_to_right() {
    for (callee, expected) in [
        ("a::b", vec!["a", "b"]),
        ("a:::b", vec!["a", ":b"]),
        ("a::::b", vec!["a", "", "b"]),
        ("a:::", vec!["a", ":"]),
        ("::a", vec!["", "a"]),
        ("x ?. a.b", vec!["x ", " a", "b"]),
        ("a->b", vec!["a", "b"]),
        ("a?->b", vec!["a", "b"]),
        ("a\\b", vec!["a", "b"]),
        ("a:b", vec!["a:b"]),
        ("a-b", vec!["a-b"]),
        ("a?b", vec!["a?b"]),
        ("a. b . c", vec!["a", " b ", " c"]),
        ("", vec![""]),
        ("a.", vec!["a", ""]),
        (".a", vec!["", "a"]),
    ] {
        let forward = forward_segments(callee);
        assert_eq!(
            forward, expected,
            "forward oracle and pinned split disagree for {callee:?}"
        );
        let got: Vec<&str> = SegmentsRev::new(callee).collect::<Vec<_>>();
        assert_eq!(
            got,
            expected.into_iter().rev().collect::<Vec<_>>(),
            "wrong backward split for {callee:?}"
        );
    }
}

#[test]
fn commented_callee_segments_still_match() {
    assert!(
        plain("Logger.new", "Logger /* reason */ .new"),
        "a comment between the base and the suffix must not hide the call"
    );
    assert!(
        plain("Logger.new", "Logger.new"),
        "the uncommented spelling still matches"
    );
    assert!(
        !plain("Logger.new", "Logger /* reason */ .other"),
        "a different suffix still misses"
    );
    assert_eq!(
        strip_comments("Logger /* reason */ .new", Lang::Rust),
        Some("Logger  .new".to_string()),
        "block comments strip out"
    );
    assert_eq!(
        strip_comments("foo // reason\n.bar", Lang::Rust),
        Some("foo \n.bar".to_string()),
        "line comments strip to the newline"
    );
    assert_eq!(
        strip_comments("obj[\"/*\"]", Lang::Rust),
        Some("obj[\"/*\"]".to_string()),
        "comment markers inside a string member are code, not a comment"
    );
    assert_eq!(
        strip_comments("Logger.new", Lang::Rust),
        None,
        "comment-free callees take the zero-alloc path"
    );
}

#[test]
fn comment_markers_follow_the_language() {
    // Bash `//` is a path separator, never a comment: `log//rotate`
    // must not match the bare `log` pattern.
    assert!(
        !plain_in("log", "log//rotate", Lang::Bash),
        "Bash log//rotate is a path, not a log call"
    );
    // The same text in Rust strips the `//` comment and matches.
    assert!(
        plain_in("log", "log//rotate", Lang::Rust),
        "Rust log//rotate strips the trailing comment and matches log"
    );
    // Zig has no block comments: `/*` dereferences.
    assert!(
        !plain_in("a", "a/*b", Lang::Zig),
        "Zig a/*b is a dereference, not a comment"
    );
    // Dart nests block comments: the whole comment strips as one.
    assert!(
        plain_in(
            "Logger.new",
            "Logger /* outer /* inner */ note */ .new",
            Lang::Dart
        ),
        "Dart nested comments strip as one comment"
    );
    // C does not nest: the outer remainder stays and hides the call.
    assert!(
        !plain_in(
            "Logger.new",
            "Logger /* outer /* inner */ note */ .new",
            Lang::C
        ),
        "C stops at the first close, so the remainder hides the call"
    );
}
