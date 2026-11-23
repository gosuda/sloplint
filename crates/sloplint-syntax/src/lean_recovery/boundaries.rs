//! Lexical declaration boundaries for a Lean tree whose root is an ERROR.
//!
//! Only unquoted column-zero declaration heads can split malformed fragments.
//! Delimited terms, nested comments, and every string form stay opaque even
//! when the failed parse has lost their original token boundaries.

use std::ops::Range;

pub(super) struct Boundaries {
    pub(super) declarations: Vec<usize>,
    opaque: Vec<Range<usize>>,
}

impl Boundaries {
    pub(super) fn is_top_level(&self, at: usize) -> bool {
        let next = self.opaque.partition_point(|range| range.end <= at);
        self.opaque.get(next).is_none_or(|range| at < range.start)
    }
}

#[derive(Clone, Copy)]
enum Context {
    Delimited(u8),
    String,
    RawString(usize),
    InterpolatedString,
    LineComment,
    BlockComment(usize),
    QuotedIdentifier,
}

pub(super) fn scan(source: &str) -> Boundaries {
    let bytes = source.as_bytes();
    let mut contexts = Vec::new();
    let mut starts = Vec::new();
    let mut opaque = Vec::new();
    let mut opaque_start = None;
    let mut at = 0;
    while at < bytes.len() {
        let was_top_level = contexts.is_empty();
        let token_start = at;
        let tail = &bytes[at..];
        match contexts.last().copied() {
            Some(Context::LineComment) => {
                at += tail
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .map_or(tail.len(), |newline| newline + 1);
                contexts.pop();
            }
            Some(Context::BlockComment(depth)) => {
                if tail.starts_with(b"/-") {
                    *contexts.last_mut().expect("comment context") =
                        Context::BlockComment(depth + 1);
                    at += 2;
                } else if tail.starts_with(b"-/") {
                    contexts.pop();
                    if depth > 1 {
                        contexts.push(Context::BlockComment(depth - 1));
                    }
                    at += 2;
                } else {
                    at += 1;
                }
            }
            Some(Context::RawString(hashes)) => {
                if tail[0] == b'"'
                    && tail
                        .get(1..hashes + 1)
                        .is_some_and(|end| end.iter().all(|&byte| byte == b'#'))
                {
                    contexts.pop();
                    at += hashes + 1;
                } else {
                    at += 1;
                }
            }
            Some(Context::String | Context::InterpolatedString) => {
                if tail[0] == b'\\' {
                    at += 2.min(tail.len());
                } else if tail[0] == b'"' {
                    contexts.pop();
                    at += 1;
                } else if tail[0] == b'{'
                    && matches!(contexts.last(), Some(Context::InterpolatedString))
                {
                    contexts.push(Context::Delimited(b'}'));
                    at += 1;
                } else {
                    at += 1;
                }
            }
            Some(Context::QuotedIdentifier) => {
                if tail.starts_with("»".as_bytes()) {
                    contexts.pop();
                    at += "»".len();
                } else {
                    at += 1;
                }
            }
            Some(Context::Delimited(_)) | None => {
                if contexts.is_empty()
                    && (at == 0 || bytes[at - 1] == b'\n')
                    && is_declaration_head(tail)
                {
                    starts.push(at);
                }
                if tail.starts_with(b"--") {
                    contexts.push(Context::LineComment);
                    at += 2;
                } else if tail.starts_with(b"/-") {
                    contexts.push(Context::BlockComment(1));
                    at += 2;
                } else if tail.starts_with(b"s!\"") || tail.starts_with(b"m!\"") {
                    contexts.push(Context::InterpolatedString);
                    at += 3;
                } else if let Some(hashes) = raw_string_hashes(tail) {
                    contexts.push(Context::RawString(hashes));
                    at += hashes + 2;
                } else if tail[0] == b'"' {
                    contexts.push(Context::String);
                    at += 1;
                } else if tail.starts_with("«".as_bytes()) {
                    contexts.push(Context::QuotedIdentifier);
                    at += "«".len();
                } else if let Some(length) = char_literal_len(source, at) {
                    at += length;
                } else {
                    match tail[0] {
                        b'(' => contexts.push(Context::Delimited(b')')),
                        b'[' => contexts.push(Context::Delimited(b']')),
                        b'{' => contexts.push(Context::Delimited(b'}')),
                        byte if matches!(contexts.last(), Some(Context::Delimited(close)) if *close == byte) =>
                        {
                            contexts.pop();
                        }
                        _ => {}
                    }
                    at += 1;
                }
            }
        }
        if was_top_level && !contexts.is_empty() {
            opaque_start = Some(token_start);
        } else if !was_top_level && contexts.is_empty() {
            opaque.push(opaque_start.take().expect("an opaque context has a start")..at);
        }
    }
    if let Some(start) = opaque_start {
        opaque.push(start..bytes.len());
    }
    Boundaries {
        declarations: starts,
        opaque,
    }
}

fn raw_string_hashes(bytes: &[u8]) -> Option<usize> {
    if bytes[0] != b'r' {
        return None;
    }
    let hashes = bytes[1..].iter().take_while(|&&byte| byte == b'#').count();
    (bytes.get(hashes + 1) == Some(&b'"')).then_some(hashes)
}

/// Lean character literals cannot contain a newline. Apostrophes in names
/// therefore never hide a declaration on the following row.
fn char_literal_len(source: &str, at: usize) -> Option<usize> {
    let bytes = &source.as_bytes()[at..];
    if bytes[0] != b'\'' {
        return None;
    }
    let first = source[at + 1..].chars().next()?;
    let end = if first == '\\' {
        bytes
            .get(3..)?
            .iter()
            .position(|&byte| matches!(byte, b'\'' | b'\n'))?
            + 3
    } else if first != '\n' {
        first.len_utf8() + 1
    } else {
        return None;
    };
    (bytes.get(end) == Some(&b'\'')).then_some(end + 1)
}

/// Attribute and modifier rows belong to the declaration that follows them.
/// Terms and custom-command heads are deliberately excluded: orphaned terms
/// must not become apparent commands after a failed declaration is removed.
fn is_declaration_head(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"@[") {
        return true;
    }
    let end = bytes
        .iter()
        .position(|&byte| {
            byte.is_ascii() && !byte.is_ascii_alphanumeric() && !b"_'!?.".contains(&byte)
        })
        .unwrap_or(bytes.len());
    matches!(
        &bytes[..end],
        b"def"
            | b"theorem"
            | b"lemma"
            | b"example"
            | b"abbrev"
            | b"instance"
            | b"axiom"
            | b"opaque"
            | b"constant"
            | b"structure"
            | b"class"
            | b"inductive"
            | b"public"
            | b"meta"
            | b"noncomputable"
            | b"partial"
            | b"nonrec"
            | b"private"
            | b"protected"
            | b"unsafe"
            | b"scoped"
            | b"local"
    )
}
