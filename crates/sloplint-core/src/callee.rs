//! Callee-pattern parsing and matching for log callees and allow lists.

use crate::facts::CallKind;

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
    pub fn parse(s: &str) -> Result<CalleePattern, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "new" {
            return Err(format!("empty callee pattern `{s}`"));
        }
        let (new_only, body) = match trimmed.strip_prefix("new ") {
            Some(rest) => (true, rest.trim()),
            None => (false, trimmed),
        };
        if body.is_empty() {
            return Err(format!("empty callee pattern `{s}`"));
        }
        let mut heads: Vec<&str> = Vec::new();
        for piece in split_pattern(body) {
            if piece.is_empty() {
                return Err(format!("empty segment in callee pattern `{s}`"));
            }
            heads.push(piece);
        }
        let Some(last) = heads.pop() else {
            return Err(format!("empty callee pattern `{s}`"));
        };
        let (text, suffix) = match last.strip_prefix('*') {
            Some(rest) if !rest.is_empty() => (rest, true),
            Some(_) => return Err(format!("`*` needs a suffix in callee pattern `{s}`")),
            None => (last, false),
        };
        let mut segments: Vec<Segment> = heads
            .into_iter()
            .map(|head| Segment {
                text: head.into(),
                suffix: false,
            })
            .collect();
        segments.push(Segment {
            text: text.into(),
            suffix,
        });
        Ok(CalleePattern { new_only, segments })
    }

    /// Reports whether the pattern matches the normalized callee under this call kind: whitespace is deleted, segments split on `::`, `?.`, and `.`, and a `Macro` callee gains a trailing `!`.
    pub fn matches(&self, callee: &str, kind: CallKind) -> bool {
        if self.new_only && kind != CallKind::New {
            return false;
        }
        let segments = normalize(callee, kind);
        if segments.len() < self.segments.len() {
            return false;
        }
        self.segments.iter().enumerate().all(|(index, pattern)| {
            let candidate = &segments[segments.len() - self.segments.len() + index];
            if pattern.suffix {
                candidate.ends_with(pattern.text.as_ref())
            } else {
                candidate.as_str() == pattern.text.as_ref()
            }
        })
    }
}

fn normalize(callee: &str, kind: CallKind) -> Vec<String> {
    let flat: String = callee.chars().filter(|c| !c.is_whitespace()).collect();
    let mut segments: Vec<String> = split_callee(&flat).into_iter().map(str::to_owned).collect();
    if kind == CallKind::Macro
        && let Some(last) = segments.last_mut()
        && !last.ends_with('!')
    {
        last.push('!');
    }
    segments
}

fn split_callee(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let (width, hit) = callee_separator_at(bytes, i);
        if hit {
            parts.push(&text[start..i]);
            i += width;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(&text[start..]);
    parts
}

fn callee_separator_at(bytes: &[u8], i: usize) -> (usize, bool) {
    if bytes[i] == b'.' {
        return (1, true);
    }
    if bytes[i] == b':' && i + 1 < bytes.len() && bytes[i + 1] == b':' {
        return (2, true);
    }
    if bytes[i] == b'?' && i + 1 < bytes.len() && bytes[i + 1] == b'.' {
        return (2, true);
    }
    (0, false)
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
                .matches("tracing::info!", CallKind::Macro)
        );
        assert!(
            CalleePattern::parse("info!")
                .unwrap()
                .matches("tracing::info", CallKind::Macro)
        );
        assert!(plain("logger.info", "self.logger.info"));
        assert!(plain("*Error", "ValueError"));
        assert!(
            CalleePattern::parse("*Error")
                .unwrap()
                .matches("TypeError", CallKind::New)
        );
        assert!(
            CalleePattern::parse("new *Error")
                .unwrap()
                .matches("TypeError", CallKind::New)
        );
        assert!(
            !CalleePattern::parse("new *Error")
                .unwrap()
                .matches("ValueError", CallKind::Plain)
        );
        assert!(!plain("a.b", "a.bc"));
        assert!(!plain("a.b", "b"));
        assert!(plain("console.log", "console . log"));
        assert!(plain("a.b", "x?.a.b"));
        assert!(plain("std::env::var", "std::env::var"));
        assert!(
            !CalleePattern::parse("log")
                .unwrap()
                .matches("log", CallKind::Macro)
        );
    }

    #[test]
    fn rejects_malformed_patterns() {
        assert!(CalleePattern::parse("").is_err());
        assert!(CalleePattern::parse("   ").is_err());
        assert!(CalleePattern::parse("a..b").is_err());
        assert!(CalleePattern::parse(".lead").is_err());
        assert!(CalleePattern::parse("trail.").is_err());
        assert!(CalleePattern::parse("*").is_err());
        assert!(CalleePattern::parse("new ").is_err());
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
