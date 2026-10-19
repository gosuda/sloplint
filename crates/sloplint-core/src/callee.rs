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
    pub fn matches(&self, callee: &str, kind: CallKind) -> bool {
        if self.new_only && kind != CallKind::New {
            return false;
        }
        let Some((tail, heads)) = self.segments.split_last() else {
            return false;
        };
        let (count, last) = scan_segments(callee);
        if count < self.segments.len() {
            return false;
        }
        let tail_matches = if kind == CallKind::Macro {
            tail.matches_macro_last(last)
        } else {
            tail.matches_plain(last)
        };
        if !tail_matches {
            return false;
        }
        let mut remaining = Segments::new(callee).skip(count - self.segments.len());
        heads.iter().all(|head| {
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

fn strip_sigil(text: &str, suffix: bool) -> &str {
    match text.strip_prefix('$') {
        Some(rest) if !suffix && !rest.is_empty() => rest,
        _ => text,
    }
}

fn scan_segments(callee: &str) -> (usize, &str) {
    let mut count = 0usize;
    let mut last = "";
    for segment in Segments::new(callee) {
        count += 1;
        last = segment;
    }
    (count, last)
}

struct Segments<'a> {
    text: &'a str,
    pos: usize,
    done: bool,
}

impl<'a> Segments<'a> {
    fn new(text: &'a str) -> Segments<'a> {
        Segments {
            text,
            pos: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for Segments<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        let start = self.pos;
        let mut cursor = start;
        while let Some((index, ch)) = next_visible(self.text, cursor) {
            if let Some(end) = separator_end(self.text, index, ch) {
                self.pos = end;
                return Some(&self.text[start..index]);
            }
            cursor = index + ch.len_utf8();
        }
        self.done = true;
        Some(&self.text[start..])
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
    }
}
