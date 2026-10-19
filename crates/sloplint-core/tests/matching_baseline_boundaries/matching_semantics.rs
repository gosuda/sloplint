use proptest::prelude::*;
use sloplint_core::CalleePattern;
use sloplint_core::facts::CallKind;

const SPACES: [&str; 6] = [" ", "\t", "\n", "\r\n", "\u{a0}", "\u{3000}"];
const SEPARATORS: [&str; 3] = [".", "::", "?."];
const NOISE_PERIOD: u8 = 5;

#[derive(Debug, Clone)]
struct Shape {
    new_only: bool,
    heads: Vec<(String, bool)>,
    last: String,
    suffix: bool,
}

impl Shape {
    fn pattern(&self) -> String {
        let mut out = String::new();
        if self.new_only {
            out.push_str("new ");
        }
        for (head, scoped) in &self.heads {
            out.push_str(head);
            out.push_str(if *scoped { "::" } else { "." });
        }
        if self.suffix {
            out.push('*');
        }
        out.push_str(&self.last);
        out
    }

    fn model_matches(&self, callee: &str, kind: CallKind) -> bool {
        if self.new_only && kind != CallKind::New {
            return false;
        }
        let flat: String = callee.chars().filter(|c| !c.is_whitespace()).collect();
        let mut parts = split_flat(&flat);
        if kind == CallKind::Macro {
            let last = parts.last_mut().expect("split always yields a segment");
            if !last.ends_with('!') {
                last.push('!');
            }
        }
        let wanted = self.heads.len() + 1;
        if parts.len() < wanted {
            return false;
        }
        let tail = &parts[parts.len() - wanted..];
        let heads_match = self
            .heads
            .iter()
            .zip(tail)
            .all(|((head, _), part)| head == part);
        let Some(last) = tail.last() else {
            return false;
        };
        let last_matches = if self.suffix {
            last.ends_with(self.last.as_str())
        } else {
            *last == self.last
        };
        heads_match && last_matches
    }
}

fn split_flat(flat: &str) -> Vec<String> {
    let bytes = flat.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let width = if bytes[i] == b'.' {
            1
        } else if matches!(
            (bytes[i], bytes.get(i + 1)),
            (b':', Some(b':')) | (b'?', Some(b'.'))
        ) {
            2
        } else {
            0
        };
        if width == 0 {
            i += 1;
        } else {
            parts.push(flat[start..i].to_string());
            i += width;
            start = i;
        }
    }
    parts.push(flat[start..].to_string());
    parts
}

fn with_noise(text: &str, noise: &[u8]) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if let Some(n) = noise.get(index).copied()
            && n % NOISE_PERIOD == 0
        {
            out.push_str(SPACES[usize::from(n / NOISE_PERIOD) % SPACES.len()]);
        }
        out.push(ch);
    }
    out
}

fn word() -> impl Strategy<Value = String> {
    "[A-Za-z0-9_]{1,4}"
}

fn shape() -> impl Strategy<Value = Shape> {
    (
        any::<bool>(),
        proptest::collection::vec((word(), any::<bool>()), 0..3),
        word(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_filter(
            "a lone `new` is not a pattern",
            |(new_only, heads, last, suffix, _)| {
                *new_only || !heads.is_empty() || *suffix || last != "new"
            },
        )
        .prop_map(|(new_only, heads, mut last, suffix, bang)| {
            if bang {
                last.push('!');
            }
            Shape {
                new_only,
                heads,
                last,
                suffix,
            }
        })
}

fn callee_for(
    shape: &Shape,
    prefix: &[String],
    lead: &str,
    mutate: Option<usize>,
    strip_bang: bool,
    separators: &[usize],
) -> String {
    let mut parts: Vec<String> = prefix.to_vec();
    parts.extend(shape.heads.iter().map(|(head, _)| head.clone()));
    let mut last = shape.last.clone();
    if strip_bang {
        last = last.trim_end_matches('!').to_string();
    }
    parts.push(format!("{lead}{last}"));
    if let Some(index) = mutate {
        let index = index % parts.len();
        parts[index].push('z');
    }
    let mut out = String::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            let choice = separators.get(index).copied().unwrap_or(0);
            out.push_str(SEPARATORS[choice % SEPARATORS.len()]);
        }
        out.push_str(part);
    }
    out
}

fn case() -> impl Strategy<Value = (Shape, String)> {
    let derived = (
        shape(),
        proptest::collection::vec(word(), 0..3),
        "[A-Za-z]{0,3}",
        proptest::option::of(0usize..6),
        any::<bool>(),
        proptest::collection::vec(0usize..3, 0..8),
        proptest::collection::vec(any::<u8>(), 0..40),
    )
        .prop_map(
            |(shape, prefix, lead, mutate, strip_bang, separators, noise)| {
                let callee = callee_for(&shape, &prefix, &lead, mutate, strip_bang, &separators);
                (shape, with_noise(&callee, &noise))
            },
        );
    let random = (
        shape(),
        proptest::collection::vec(
            prop_oneof![
                word(),
                Just(".".to_string()),
                Just("::".to_string()),
                Just("?.".to_string()),
                Just("!".to_string()),
                prop::sample::select(SPACES.to_vec()).prop_map(str::to_string),
            ],
            0..12,
        ),
    )
        .prop_map(|(shape, pieces)| (shape, pieces.concat()));
    prop_oneof![3 => derived, 1 => random]
}

proptest! {
    #[test]
    fn matching_agrees_with_the_documented_normalization_model((shape, callee) in case()) {
        let pattern = CalleePattern::parse(&shape.pattern()).expect("generated patterns are valid");
        for kind in [CallKind::Plain, CallKind::Macro, CallKind::New] {
            prop_assert_eq!(
                pattern.matches(&callee, kind),
                shape.model_matches(&callee, kind),
                "pattern {:?} callee {:?} kind {:?}",
                shape.pattern(),
                callee,
                kind
            );
        }
    }
}

fn observed(pattern: &str, callee: &str, kind: CallKind) -> bool {
    CalleePattern::parse(pattern)
        .unwrap_or_else(|error| panic!("pattern {pattern:?} must parse: {error}"))
        .matches(callee, kind)
}

fn hit_as(pattern: &str, callee: &str, kind: CallKind) {
    assert!(
        observed(pattern, callee, kind),
        "pattern {pattern:?} must match callee {callee:?} as {kind:?}"
    );
}

fn miss_as(pattern: &str, callee: &str, kind: CallKind) {
    assert!(
        !observed(pattern, callee, kind),
        "pattern {pattern:?} must not match callee {callee:?} as {kind:?}"
    );
}

fn hit(pattern: &str, callee: &str) {
    hit_as(pattern, callee, CallKind::Plain);
}

fn miss(pattern: &str, callee: &str) {
    miss_as(pattern, callee, CallKind::Plain);
}

#[test]
fn unicode_whitespace_is_ignored_inside_segments_and_separators() {
    hit("std::env::var", "std\u{a0}:\u{3000}:env . va\nr");
    hit("a.b", "x ? . a\t.\u{a0}b");
    hit("console.log", "console\r\n  .log");
    miss("a.b", "a\u{a0}x.b");
}

#[test]
fn macro_kind_appends_bang_to_the_last_segment_only() {
    hit_as("info!", "tracing::info", CallKind::Macro);
    hit_as("info!", "tracing::info!", CallKind::Macro);
    hit_as("info!", "tracing::info !", CallKind::Macro);
    miss_as("info!", "tracing::info", CallKind::Plain);
    miss_as("info!!", "info!", CallKind::Macro);
    hit_as("tracing.info!", "tracing::info", CallKind::Macro);
    miss_as("log", "log", CallKind::Macro);
    hit_as("*rror!", "Error", CallKind::Macro);
    miss_as("*rror!", "Error", CallKind::Plain);
}

#[test]
fn suffix_wildcard_applies_to_the_last_segment_only() {
    hit("*Error", "a.Error");
    hit("*Error", "TypeError");
    miss("*Error", "Error.a");
    miss("*Error", "Errors");
    hit("x.*Error", "y.x.TypeError");
    miss("x.*Error", "x.y.TypeError");
}

#[test]
fn new_prefix_restricts_the_call_kind() {
    hit_as("new *Error", "TypeError", CallKind::New);
    miss_as("new *Error", "TypeError", CallKind::Plain);
    miss_as("new *Error", "TypeError", CallKind::Macro);
    hit_as("*Error", "TypeError", CallKind::New);
    hit_as("*Error", "TypeError", CallKind::Plain);
}

#[test]
fn short_and_empty_segments_follow_the_normalization_contract() {
    miss("a.b.c", "b.c");
    miss("a", "");
    miss("a", "a.");
    hit("a.b", ".a.b");
    hit("a.b", "x::a.b");
    miss("a.b", "a:b");
    hit("a:b", "a:b");
}
