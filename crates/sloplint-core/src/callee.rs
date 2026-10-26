//! Callee-pattern parsing and matching for log callees and allow lists.

use std::iter::Filter;
use std::str::Chars;

use crate::facts::CallKind;

type Visible<'a> = Filter<Chars<'a>, fn(&char) -> bool>;

fn is_visible(ch: &char) -> bool {
    !ch.is_whitespace()
}

#[derive(Debug, Clone)]
struct Segment {
    text: Box<str>,
    suffix: bool,
}

/// A parsed callee pattern such as `info!`, `logger.info`, `*Error`, or `new *Error`.
#[derive(Debug, Clone)]
pub struct CalleePattern {
    new_only: bool,
    segments: Vec<Segment>,
}

impl CalleePattern {
    /// Parses a pattern: segments split on `.` and `::`, a leading `*` on the last segment matches any suffix, and a `new ` prefix restricts the match to `CallKind::New`.
    ///
    /// Shapes that cannot match a normalized callee are errors that name the pattern: whitespace inside the pattern (callees lose all whitespace before matching), a `*` anywhere but the first character of the last segment, and `?.`, `\`, or `->`, which callee normalization consumes as segment separators (write `.` or `::` instead).
    pub fn parse(s: &str) -> Result<CalleePattern, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "new" {
            return Err(format!("empty callee pattern `{s}`"));
        }
        let (new_only, body) = match trimmed.strip_prefix("new") {
            Some(rest) if rest.chars().next().is_some_and(char::is_whitespace) => {
                (true, rest.trim_start())
            }
            _ => (false, trimmed),
        };
        if body.is_empty() {
            return Err(format!("empty callee pattern `{s}`"));
        }
        if body.chars().any(char::is_whitespace) {
            return Err(format!(
                "whitespace inside callee pattern `{s}`: callees are matched with all whitespace removed, so write the pattern without it"
            ));
        }
        if body.contains("?.") {
            return Err(format!(
                "`?.` in callee pattern `{s}` is a segment separator in callees and never appears inside a segment; write `.` instead"
            ));
        }
        if body.contains('\\') || body.contains("->") {
            return Err(format!(
                "`\\` and `->` in callee pattern `{s}` are segment separators in callees and never appear inside a segment; separate segments with `.` or `::`"
            ));
        }
        let pieces = split_pattern(body);
        let last_index = pieces.len() - 1;
        let mut segments: Vec<Segment> = Vec::with_capacity(pieces.len());
        for (index, piece) in pieces.into_iter().enumerate() {
            if piece.is_empty() {
                return Err(format!("empty segment in callee pattern `{s}`"));
            }
            let is_last = index == last_index;
            let (text, suffix) = match piece.strip_prefix('*') {
                Some("") => return Err(format!("`*` needs a suffix in callee pattern `{s}`")),
                Some(rest) if is_last => (rest, true),
                Some(_) => {
                    return Err(format!(
                        "`*` may only start the last segment of callee pattern `{s}`"
                    ));
                }
                None => (piece, false),
            };
            if text.contains('*') {
                return Err(format!(
                    "`*` may only start the last segment of callee pattern `{s}`"
                ));
            }
            segments.push(Segment {
                text: strip_sigil(text, suffix).into(),
                suffix,
            });
        }
        Ok(CalleePattern { new_only, segments })
    }

    /// Reports whether the pattern matches the callee under this call kind.
    ///
    /// All whitespace is ignored, segments split on `::`, `?.`, `->`, `?->`, `\`, and `.`, a `$` that starts a longer segment is ignored (`$this`, `$logger`), and a `Macro` callee gains a trailing `!` on its last segment. The callee is scanned in place and nothing is allocated.
    ///
    /// Segments are consumed back to front, so the last segment rejects first: a
    /// pattern that cannot match costs the length of one tail segment, not a
    /// scan of the whole callee.
    pub fn matches(&self, callee: &str, kind: CallKind) -> bool {
        if self.new_only && kind != CallKind::New {
            return false;
        }
        let Some((tail, heads)) = self.segments.split_last() else {
            return false;
        };
        if !tail_can_match(callee, &tail.text, kind) {
            return false;
        }
        let mut remaining = SegmentsRev::new(callee);
        let Some(last) = remaining.next() else {
            return false;
        };
        let tail_matches = if kind == CallKind::Macro {
            tail.matches_macro_last(last)
        } else {
            tail.matches_plain(last)
        };
        if !tail_matches {
            return false;
        }
        heads.iter().rev().all(|head| {
            remaining
                .next()
                .is_some_and(|candidate| head.matches_plain(candidate))
        })
    }
}

impl Segment {
    fn matches_plain(&self, raw: &str) -> bool {
        self.accepts(self.candidate(raw))
    }

    fn matches_macro_last(&self, raw: &str) -> bool {
        let candidate = self.candidate(raw);
        let bang = (candidate.clone().next_back() != Some('!')).then_some('!');
        self.accepts(candidate.chain(bang))
    }

    fn candidate<'a>(&self, raw: &'a str) -> Visible<'a> {
        let mut visible = raw.chars().filter(is_visible as fn(&char) -> bool);
        let sigiled = visible.clone().next() == Some('$') && visible.clone().nth(1).is_some();
        if sigiled && !self.suffix {
            visible.next();
        }
        visible
    }

    fn accepts<I>(&self, candidate: I) -> bool
    where
        I: DoubleEndedIterator<Item = char>,
    {
        if self.suffix {
            let mut tail = candidate.rev();
            return self
                .text
                .chars()
                .rev()
                .all(|expected| tail.next() == Some(expected));
        }
        candidate.eq(self.text.chars())
    }
}

/// Whether the callee's visible tail can still match the pattern's last segment.
///
/// A matching last segment always ends with the pattern's tail text: a leading
/// `$` is only ever stripped from the front and whitespace is ignored, so a
/// callee whose visible characters do not end that way cannot match. (A `Macro`
/// callee gains a `!` the text may lack, so a callee ending with the tail minus
/// one trailing `!` can also match.) This reads only as many trailing characters
/// as the tail text is long, so a pattern that cannot match costs the tail's
/// length instead of a scan of the whole callee.
///
/// When the tail does match, the reverse scan below still costs only the last
/// few segments: it yields segments back to front and stops after the
/// pattern's own segments, so a deep chain ending in a configured pattern
/// stays linear too.
fn tail_can_match(callee: &str, tail: &str, kind: CallKind) -> bool {
    if ends_with_visible(callee, tail) {
        return true;
    }
    match kind {
        CallKind::Macro => ends_with_visible(callee, tail.strip_suffix('!').unwrap_or(tail)),
        _ => false,
    }
}

/// Whether the callee's non-whitespace characters end with `tail`.
fn ends_with_visible(callee: &str, tail: &str) -> bool {
    let mut visible = callee.chars().rev().filter(is_visible as fn(&char) -> bool);
    tail.chars()
        .rev()
        .all(|expected| visible.next() == Some(expected))
}

fn strip_sigil(text: &str, suffix: bool) -> &str {
    match text.strip_prefix('$') {
        Some(rest) if !suffix && !rest.is_empty() => rest,
        _ => text,
    }
}

/// Yields the raw segments of `callee` from last to first, the exact reverse of a
/// left-to-right split. Each yielded range keeps its interior whitespace, like the
/// forward walk: callers normalize with [`Segment::candidate`].
struct SegmentsRev<'a> {
    text: &'a str,
    /// End (exclusive) of the current rightmost un-yielded segment.
    end: usize,
    done: bool,
    /// `(run length, `:` consumed so far)` inside the current visible run of
    /// `:`.
    ///
    /// `::` pairs left to right, so whether a `:` ends a pair depends on its
    /// position `h` counted back from the run's last `:`: it pairs exactly when
    /// `h + len` is odd.
    colon_run: Option<(usize, usize)>,
}

impl<'a> SegmentsRev<'a> {
    fn new(text: &'a str) -> SegmentsRev<'a> {
        SegmentsRev {
            text,
            end: text.len(),
            done: false,
            colon_run: None,
        }
    }

    /// Whether the `:` at `index` ends a `::` separator, and where that separator
    /// starts. `seen` counts the `:` consumed so far in this run — scanned ones,
    /// plus the partner inside each yielded separator — so it equals the
    /// position of `index` counted back from the run's last `:`. The run length
    /// is measured once, on that last `:`.
    fn colon_pair_start(&mut self, index: usize) -> Option<usize> {
        let (len, seen) = match self.colon_run {
            Some((len, seen)) => (len, seen + 1),
            None => {
                let mut len = 1usize;
                let mut cursor = index;
                while let Some((i, ':')) = prev_visible(self.text, cursor) {
                    len += 1;
                    cursor = i;
                }
                (len, 1)
            }
        };
        if (seen + len) % 2 == 1 {
            self.colon_run = Some((len, seen + 1));
            prev_visible(self.text, index).map(|(left, _)| left)
        } else {
            self.colon_run = Some((len, seen));
            None
        }
    }
}

impl<'a> Iterator for SegmentsRev<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        let mut cursor = self.end;
        while let Some((index, ch)) = prev_visible(self.text, cursor) {
            let start = if ch == ':' {
                self.colon_pair_start(index)
            } else {
                self.colon_run = None;
                separator_start(self.text, index, ch)
            };
            if let Some(start) = start {
                let segment = &self.text[index + ch.len_utf8()..self.end];
                self.end = start;
                return Some(segment);
            }
            cursor = index;
        }
        self.done = true;
        Some(&self.text[..self.end])
    }
}

fn prev_visible(text: &str, before: usize) -> Option<(usize, char)> {
    text[..before]
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
}

/// The byte index where the separator ending at `(index, ch)` starts, or `None`
/// when `ch` is not the end of a segment separator. `?` and `-` are separators
/// only inside `?.`, `?->`, or `->`, so they never end one: the scan reaches
/// them only through `.` or `>`. `:` pairs are decided by [`SegmentsRev`]
/// because their direction depends on the run they sit in.
fn separator_start(text: &str, index: usize, ch: char) -> Option<usize> {
    match ch {
        '.' | '\\' => match prev_visible(text, index) {
            Some((q, '?')) if ch == '.' => Some(q),
            _ => Some(index),
        },
        '>' => match prev_visible(text, index) {
            Some((dash, '-')) => match prev_visible(text, dash) {
                Some((q, '?')) => Some(q),
                _ => Some(dash),
            },
            _ => None,
        },
        _ => None,
    }
}

fn split_pattern(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            parts.push(&text[start..i]);
            i += 1;
            start = i;
        } else if bytes[i] == b':' && i + 1 < bytes.len() && bytes[i + 1] == b':' {
            parts.push(&text[start..i]);
            i += 2;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(&text[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn plain(pattern: &str, callee: &str) -> bool {
        CalleePattern::parse(pattern)
            .unwrap()
            .matches(callee, CallKind::Plain)
    }

    #[test]
    fn matches_plan_examples() {
        assert!(
            CalleePattern::parse("info!")
                .unwrap()
                .matches("tracing::info!", CallKind::Macro),
            "info! should match tracing::info! macro"
        );
        assert!(
            CalleePattern::parse("info!")
                .unwrap()
                .matches("tracing::info", CallKind::Macro),
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
                .matches("TypeError", CallKind::New),
            "*Error should match TypeError under New kind"
        );
        assert!(
            CalleePattern::parse("new *Error")
                .unwrap()
                .matches("TypeError", CallKind::New),
            "new *Error should match TypeError under New kind"
        );
        assert!(
            !CalleePattern::parse("new *Error")
                .unwrap()
                .matches("ValueError", CallKind::Plain),
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
                .matches("log", CallKind::Macro),
            "plain log must not match log under Macro kind without bang"
        );
    }

    #[test]
    fn long_callee_without_matching_tail_rejects() {
        let pattern = CalleePattern::parse("logger.info").unwrap();
        let deep = format!("echo {}{}", "$(".repeat(20_000), ")".repeat(20_000));
        assert!(
            !pattern.matches(&deep, CallKind::Plain),
            "a callee whose tail is not the pattern's last segment cannot match"
        );
        let chain = format!("{}info", "x.".repeat(1_000));
        assert!(
            CalleePattern::parse("x.info")
                .unwrap()
                .matches(&chain, CallKind::Plain),
            "a callee ending in the pattern's segments still matches"
        );
        let bang = CalleePattern::parse("info!").unwrap();
        for callee in ["tracing::info!", "tracing::info"] {
            assert!(
                bang.matches(callee, CallKind::Macro),
                "info! still matches {callee} under Macro"
            );
        }
        assert!(
            !bang.matches("tracing::debug!", CallKind::Macro),
            "info! still rejects a non-matching macro callee"
        );
    }

    #[test]
    fn nested_callees_reject_every_pattern_in_linear_time() {
        use std::time::{Duration, Instant};
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
        // Prebuilt outside the timed region; each callee nests one level deeper.
        // One format! per callee keeps construction at one copy of its own text.
        let mut callees = Vec::with_capacity(4_000);
        let mut open = String::from("f(");
        let mut close = String::from(")");
        for _ in 0..4_000 {
            open.push_str("$(");
            close.push(')');
            callees.push(format!("{open}x{close}"));
        }
        let reject_all = |callees: &[String]| {
            for callee in callees {
                for pattern in &patterns {
                    assert!(
                        !pattern.matches(callee, CallKind::Plain),
                        "a nested callee matches no log pattern"
                    );
                }
            }
        };
        let bytes = |callees: &[String]| callees.iter().map(String::len).sum::<usize>();
        let bytes_small = bytes(&callees[..1_000]);
        let bytes_total = bytes(&callees[..]);
        let start = Instant::now();
        reject_all(&callees[..1_000]);
        let small = start.elapsed();
        reject_all(&callees[1_000..]);
        let total = start.elapsed();
        // A matcher linear in callee text keeps a constant cost per byte; the
        // old whole-callee rescan made the per-byte cost grow with depth. Four
        // times the per-byte floor separates the two without timing the input
        // size itself (which is inherently quadratic across 4,000 depths). The
        // comparison is cross-multiplied so it stays in integer nanoseconds.
        assert!(
            total.as_nanos() * (bytes_small as u128)
                < 4 * small.as_nanos().max(1) * (bytes_total as u128),
            "matching cost per callee byte must stay flat, not grow with depth"
        );
        assert!(
            total < Duration::from_secs(90),
            "rejecting every pattern at every depth must stay linear in depth"
        );
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
            prop_assert!(pattern.matches(&callee_segments.join("."), CallKind::Plain));
            prop_assert!(pattern.matches(&callee_segments.join("."), CallKind::New));
        }

        #[test]
        fn exact_pattern_never_matches_changed_last_segment(p in segment_vec(1, 4)) {
            let pattern = CalleePattern::parse(&p.join(".")).unwrap();
            let mut changed = p.clone();
            let last = changed.len() - 1;
            changed[last].push('z');
            prop_assert!(!pattern.matches(&changed.join("."), CallKind::Plain));
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
            ("a:.b", vec!["a:", "b"]),
            ("a::.b", vec!["a", "", "b"]),
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
}
