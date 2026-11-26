//! Literal spans and variable boundaries in PowerShell command words.

use std::sync::LazyLock;

use regex::Regex;

use crate::hooks::narrow;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PsWordQuote {
    Bare,
    Expandable,
    Verbatim,
}

// PowerShell scans variable names as UTF-16 chars in the Letter and
// Decimal_Number categories. Other numeric and alphabetic Unicode properties
// are broader, so Rust's is_alphanumeric is not the same predicate.
fn ps_word_name_char(ch: char) -> bool {
    if ch.is_ascii() {
        return ch.is_ascii_alphanumeric();
    }
    static NAME_CHAR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[\p{L}\p{Nd}]$").expect("constant character class"));
    let mut utf8 = [0; 4];
    ch.len_utf16() == 1 && NAME_CHAR.is_match(ch.encode_utf8(&mut utf8))
}

// The grammar hides a variable-path word's leading variable in an anonymous
// token. Scan that name without including its following path separator.
pub(super) fn ps_word_variable_end(raw: &str) -> Option<usize> {
    let tail = raw.strip_prefix('$')?;
    let first = tail.chars().next()?;
    if first == '{' {
        let mut escaped = false;
        for (offset, ch) in tail.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if ch == '`' {
                escaped = true;
            } else if ch == '}' {
                return Some(1 + offset + ch.len_utf8());
            }
        }
        return None;
    }
    if matches!(first, '$' | '?' | '^') {
        return Some(1 + first.len_utf8());
    }
    if !matches!(first, '_' | ':') && !ps_word_name_char(first) {
        return None;
    }
    let mut end = 1 + first.len_utf8();
    for (offset, ch) in tail.char_indices().skip(1) {
        if ch == ':' && tail[offset..].starts_with("::") {
            break;
        }
        if !ps_word_name_char(ch) && !matches!(ch, '_' | '?' | ':') {
            break;
        }
        end = 1 + offset + ch.len_utf8();
    }
    Some(end)
}

// Named AST holes are disjoint and sorted, and their byte offsets are relative
// to this UTF-8 word. Quotes change lexical mode without breaking a literal run.
pub(super) fn ps_word_static_run(raw: &str, dynamics: &[(usize, usize)]) -> u32 {
    let mut quote = PsWordQuote::Bare;
    let mut offset = 0;
    let mut dynamic = 0;
    let mut run = 0;
    let mut best = 0;
    while offset < raw.len() {
        if let Some(&(start, end)) = dynamics.get(dynamic)
            && offset == start
        {
            best = best.max(run);
            run = 0;
            offset = end;
            dynamic += 1;
            continue;
        }
        let ch = raw[offset..]
            .chars()
            .next()
            .expect("offset is before the end");
        offset += ch.len_utf8();
        if quote == PsWordQuote::Verbatim {
            if ch == '\'' {
                if raw[offset..].starts_with('\'') {
                    run += 1;
                    offset += 1;
                } else {
                    quote = PsWordQuote::Bare;
                }
            } else {
                run += 1;
            }
            continue;
        }
        if ch == '`' {
            best = best.max(run);
            run = 0;
            if let Some(end) = ps_word_unicode_escape_end(&raw[offset..]) {
                offset += end;
            } else if let Some(escaped) = raw[offset..].chars().next() {
                offset += escaped.len_utf8();
            }
        } else if ch == '\'' && quote == PsWordQuote::Bare {
            quote = PsWordQuote::Verbatim;
        } else if ch == '"' {
            if quote == PsWordQuote::Expandable && raw[offset..].starts_with('"') {
                run += 1;
                offset += 1;
            } else {
                quote = if quote == PsWordQuote::Bare {
                    PsWordQuote::Expandable
                } else {
                    PsWordQuote::Bare
                };
            }
        } else if ch == '$'
            && let Some(end) = ps_word_variable_end(&raw[offset - 1..])
        {
            best = best.max(run);
            run = 0;
            offset += end - 1;
        } else {
            run += 1;
        }
    }
    narrow(best.max(run))
}

// Length of a valid backtick-Unicode escape (`u{1F600}, `u and hex digits
// case-insensitive) in bytes, measured from after the backtick. PowerShell
// decodes the whole sequence to one character, so the word scanner consumes
// it as one escape. Anything else is not a Unicode escape: the caller falls
fn ps_word_unicode_escape_end(raw: &str) -> Option<usize> {
    let body = raw.strip_prefix('u').or_else(|| raw.strip_prefix('U'))?;
    let hex = body.strip_prefix('{')?;
    let close = hex.find('}')?;
    let digits = &hex[..close];
    if !(1..=6).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(1 + 1 + digits.len() + 1)
}

// Byte spans of `$variable` interpolations the grammar leaves as plain word
// text (v$VERSION.tar.gz emits no interpolation child). Single-quoted
// stretches stay verbatim, so dollars inside them are never dynamics.
pub(super) fn ps_word_inline_dynamics(raw: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    let mut verbatim = false;
    while offset < raw.len() {
        let ch = match raw[offset..].chars().next() {
            Some(ch) => ch,
            None => break,
        };
        if verbatim {
            if ch == '\'' {
                if raw[offset + 1..].starts_with('\'') {
                    offset += 2;
                    continue;
                }
                verbatim = false;
            }
            offset += ch.len_utf8();
            continue;
        }
        if ch == '\'' {
            verbatim = true;
        } else if ch == '$' {
            if let Some(end) = ps_word_variable_end(&raw[offset..]) {
                out.push((offset, offset + end));
                offset += end;
                continue;
            }
        }
        offset += ch.len_utf8();
    }
    out
}
