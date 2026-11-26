//! Literal spans and variable boundaries in PowerShell command words.

use std::sync::LazyLock;

use regex::Regex;
use sloplint_core::facts::ArgShape;

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

// Generic-word variables can be hidden in anonymous grammar tokens. Scan the
// name without including its following literal text.
fn ps_word_variable_end(raw: &str) -> Option<usize> {
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

// Unicode escape spelling is one escape, including its braces and hex digits.
// PowerShell accepts UTF-16 surrogate values as well as Unicode scalar values.
fn ps_unicode_escape_len(raw: &str) -> Option<usize> {
    let hex = raw.strip_prefix("u{")?;
    let mut value = 0;
    for (offset, byte) in hex.bytes().take(7).enumerate() {
        if byte == b'}' {
            return (offset != 0 && value <= 0x10ffff).then_some(offset + 3);
        }
        value = value * 16 + char::from(byte).to_digit(16)?;
    }
    None
}

// PowerShell's ScanSubExpression balances raw parentheses before parsing the
// body. The computed expression's source never contributes literal text.
fn ps_word_subexpression_end(raw: &str) -> Option<usize> {
    let body = raw.strip_prefix("$(")?;
    let mut depth = 1;
    for (offset, byte) in body.bytes().enumerate() {
        if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(offset + 3);
            }
        }
    }
    None
}

// Named AST holes are disjoint and sorted, and their byte offsets are relative
// to this UTF-8 word. Quotes change lexical mode without breaking a literal run.
pub(super) fn ps_word_shape(raw: &str, dynamics: &[(usize, usize)]) -> ArgShape<'static> {
    let mut quote = PsWordQuote::Bare;
    let mut offset = 0;
    let mut dynamic = 0;
    let mut interpolated = false;
    let mut has_unicode_escape = false;
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
            interpolated = true;
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
            let unicode_escape = ps_unicode_escape_len(&raw[offset..]);
            has_unicode_escape |= unicode_escape.is_some();
            offset += unicode_escape
                .unwrap_or_else(|| raw[offset..].chars().next().map_or(0, char::len_utf8));
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
            && let Some(end) = ps_word_subexpression_end(&raw[offset - 1..])
                .or_else(|| ps_word_variable_end(&raw[offset - 1..]))
        {
            best = best.max(run);
            run = 0;
            offset += end - 1;
            interpolated = true;
        } else {
            run += 1;
        }
    }
    if interpolated {
        ArgShape::Concat {
            longest_static: narrow(best.max(run)),
        }
    } else {
        ArgShape::Str {
            longest_static: narrow(if has_unicode_escape {
                best.max(run)
            } else {
                raw.chars().count()
            }),
        }
    }
}
