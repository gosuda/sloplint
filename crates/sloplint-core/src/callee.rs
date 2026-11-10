use std::iter::Filter;
use std::str::Chars;

use crate::facts::CallKind;

type Visible<'a> = Filter<Chars<'a>, fn(&char) -> bool>;

#[cfg(test)]
std::thread_local! { static VISIBLE_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
fn is_visible(ch: &char) -> bool {
    #[cfg(test)]
    VISIBLE_CHECKS.with(|c| c.set(c.get() + 1));
    !ch.is_whitespace()
}

#[derive(Debug, Clone)]
struct Segment {
    text: Box<str>,
    suffix: bool,
}

#[derive(Debug, Clone)]
pub struct CalleePattern {
    new_only: bool,
    segments: Vec<Segment>,
}

impl CalleePattern {
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

fn tail_can_match(callee: &str, tail: &str, kind: CallKind) -> bool {
    if ends_with_visible(callee, tail) {
        return true;
    }
    match kind {
        CallKind::Macro => ends_with_visible(callee, tail.strip_suffix('!').unwrap_or(tail)),
        _ => false,
    }
}

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

struct SegmentsRev<'a> {
    text: &'a str,
    end: usize,
    done: bool,
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
        #[cfg(test)]
        VISIBLE_CHECKS.with(|c| c.set(c.get() + 1));
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
    for (index, ch) in text[..before].char_indices().rev() {
        #[cfg(test)]
        VISIBLE_CHECKS.with(|c| c.set(c.get() + 1));
        if !ch.is_whitespace() {
            return Some((index, ch));
        }
    }
    None
}

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
#[path = "callee_tests.rs"]
mod tests;
