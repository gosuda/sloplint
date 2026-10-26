//! Findings, severity levels, and the shared position-to-column/context projection.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::facts::{FileFacts, NamedSite, Pos};

/// How loudly a finding speaks; `Warn` sorts before `Deny`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warn,
    Deny,
}

/// A rule's configured loudness cap; `Off` disables the rule entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    Warn,
    Deny,
}

/// The finding severity a run fails on; `Never` never fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    Warn,
    Deny,
    Never,
}

/// One reported defect with its location, instructions, and ratchet fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule: Box<str>,
    pub severity: Severity,
    pub path: Box<str>,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub fix: String,
    pub context: Box<str>,
    pub fingerprint: Box<str>,
}

/// Longest context string kept before the `...` suffix is appended.
const MAX_CONTEXT_CHARS: usize = 160;

impl Finding {
    /// Builds a finding inside an analyzed file; column and context come from `facts.source`.
    ///
    /// The line is found through the file's line index, so the cost is the length of that line.
    /// The result equals [`Finding::at_source`] for the same path, source, and position.
    pub fn at(
        facts: &FileFacts<'_>,
        pos: Pos,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        let (column, context) = source_site_indexed(&facts.line_starts, facts.source, pos);
        Finding {
            rule: rule.into(),
            severity,
            path: facts.path.into(),
            line: pos.line,
            column,
            message,
            fix,
            context,
            fingerprint: Box::from(""),
        }
    }

    /// Builds a finding whose line, column, and context are copied from a `NamedSite`.
    pub fn at_site(
        path: &str,
        site: &NamedSite,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        Finding {
            rule: rule.into(),
            severity,
            path: path.into(),
            line: site.line,
            column: site.column,
            message,
            fix,
            context: site.context.clone(),
            fingerprint: Box::from(""),
        }
    }

    /// Builds a finding from a raw source string, computing column and context at `pos`.
    pub fn at_source(
        path: &str,
        source: &str,
        pos: Pos,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        let (column, context) = source_site(source, pos);
        Finding {
            rule: rule.into(),
            severity,
            path: path.into(),
            line: pos.line,
            column,
            message,
            fix,
            context,
            fingerprint: Box::from(""),
        }
    }
}

/// Projects a source position to `(column, context)`: the column counts Unicode scalar values from the line start, and the context is the trimmed line truncated to 160 chars with `...` when cut.
pub fn source_site(source: &str, pos: Pos) -> (u32, Box<str>) {
    let (start, text) = line_at(source, pos.line);
    let pos_byte = usize::try_from(pos.byte).unwrap_or(usize::MAX);
    let target = pos_byte.saturating_sub(start).min(text.len());
    let chars = match text.get(..target) {
        Some(prefix) if prefix.is_ascii() => target,
        _ => text
            .char_indices()
            .take_while(|(offset, _)| *offset < target)
            .count(),
    };
    let column = u32::try_from(chars).unwrap_or(u32::MAX).saturating_add(1);
    (column, Box::from(context_of(text)))
}

/// The byte offset where each `\n`-separated part of `source` starts, in line order.
///
/// The index always has at least one entry, because the empty source is one empty line, and a
/// trailing newline starts a last empty line. An index belongs to the source it was built from:
/// pass it to [`source_site_indexed`] only with that same source. Offsets past `u32::MAX` clamp,
/// like [`Pos::byte`].
pub fn line_starts(source: &str) -> Vec<u32> {
    let mut starts = vec![0];
    for (at, _) in source.match_indices('\n') {
        starts.push(u32::try_from(at + 1).unwrap_or(u32::MAX));
    }
    starts
}

/// [`source_site`] through a line index built by [`line_starts`] for the same `source`.
///
/// The line is taken from `pos.line` alone, independently of `pos.byte`, and invalid positions
/// resolve exactly as in [`source_site`]: line 0 is the first line, a line past the last one is an
/// empty line, and a byte outside the line clamps to its ends. The cost is the length of the line.
pub fn source_site_indexed(starts: &[u32], source: &str, pos: Pos) -> (u32, Box<str>) {
    let index = offset(pos.line.saturating_sub(1));
    let Some(&start) = starts.get(index) else {
        return (1, Box::from(""));
    };
    let end = match starts.get(index + 1) {
        Some(&next) => offset(next).saturating_sub(1),
        None => source.len(),
    };
    let text = source.get(offset(start)..end).unwrap_or("");
    source_site(
        text,
        Pos {
            line: 1,
            byte: pos.byte.saturating_sub(start),
        },
    )
}

fn offset(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn line_at(source: &str, line: u32) -> (usize, &str) {
    let mut start = 0usize;
    let newline = char::from(10u8);
    for (current, part) in (1u32..).zip(source.split(newline)) {
        if current >= line {
            return (start, part);
        }
        start += part.len() + 1;
    }
    (0, "")
}

fn clip_context(trimmed: &str) -> Cow<'_, str> {
    let mut chars = trimmed.char_indices();
    match chars.nth(MAX_CONTEXT_CHARS) {
        Some((end, _)) => {
            let mut out = String::with_capacity(end + 3);
            out.push_str(&trimmed[..end]);
            out.push_str("...");
            Cow::Owned(out)
        }
        None => Cow::Borrowed(trimmed),
    }
}

fn context_of(line: &str) -> Cow<'_, str> {
    if line.contains('\r') {
        let cleaned = line.replace('\r', "");
        Cow::Owned(clip_context(cleaned.trim()).into_owned())
    } else {
        clip_context(line.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_column_and_context() {
        let source = "ab\n  x.y\n\r\n  tail\r";
        let (column, context) = source_site(source, Pos { line: 2, byte: 5 });
        assert_eq!(column, 3);
        assert_eq!(context.as_ref(), "x.y");
        let (_, context) = source_site(source, Pos { line: 4, byte: 7 });
        assert_eq!(context.as_ref(), "tail");
        let (_, context) = source_site(source, Pos { line: 9, byte: 0 });
        assert_eq!(context.as_ref(), "");
    }

    #[test]
    fn source_positions_clamp_at_unicode_line_boundaries() {
        let source = "head\néx\n";
        for (byte, column) in [
            (4, 1),
            (5, 1),
            (6, 2),
            (7, 2),
            (8, 3),
            (9, 3),
            (u32::MAX, 3),
        ] {
            let (actual, context) = source_site(source, Pos { line: 2, byte });
            assert_eq!(actual, column, "column for absolute byte {byte}");
            assert_eq!(context.as_ref(), "éx", "context for absolute byte {byte}");
        }
        assert_eq!(
            source_site(
                "",
                Pos {
                    line: 0,
                    byte: u32::MAX
                }
            ),
            (1, "".into())
        );
    }

    #[test]
    fn truncates_long_context() {
        let line = "a".repeat(200);
        let (_, context) = source_site(&line, Pos { line: 1, byte: 0 });
        assert_eq!(context.chars().count(), MAX_CONTEXT_CHARS + 3);
        assert!(
            context.ends_with("..."),
            "long context {context:?} should be truncated with ..."
        );
        let short = "a".repeat(MAX_CONTEXT_CHARS);
        let (_, context) = source_site(&short, Pos { line: 1, byte: 0 });
        assert_eq!(context.chars().count(), MAX_CONTEXT_CHARS);
        assert!(
            !context.ends_with("..."),
            "short context {context:?} should not be truncated"
        );
    }

    #[test]
    fn orders_and_serializes_levels() {
        assert!(
            Severity::Warn < Severity::Deny,
            "Warn should order before Deny"
        );
        assert!(
            Level::Off < Level::Warn && Level::Warn < Level::Deny,
            "levels should order Off < Warn < Deny"
        );
        assert!(
            FailOn::Warn < FailOn::Deny && FailOn::Deny < FailOn::Never,
            "fail-on thresholds should order Warn < Deny < Never"
        );
        assert_eq!(sonic_rs::to_string(&Severity::Warn).unwrap(), "\"warn\"");
        assert_eq!(sonic_rs::to_string(&FailOn::Never).unwrap(), "\"never\"");
        let level: Level = sonic_rs::from_str("\"deny\"").unwrap();
        assert_eq!(level, Level::Deny);
    }

    #[test]
    fn constructors_fill_location_and_leave_fingerprint_empty() {
        let source = "let x = 1;\n";
        let pos = Pos { line: 1, byte: 4 };
        let site = NamedSite {
            name: "x".into(),
            line: 1,
            column: 5,
            context: "let x = 1;".into(),
        };
        let from_source = Finding::at_source(
            "a.rs",
            source,
            pos,
            "r/one",
            Severity::Deny,
            "m".into(),
            "f".into(),
        );
        assert_eq!(
            (from_source.line, from_source.column),
            (1, source_site(source, pos).0)
        );
        assert_eq!(from_source.context.as_ref(), "let x = 1;");
        assert_eq!(from_source.fingerprint.as_ref(), "");
        let from_site = Finding::at_site(
            "b.rs",
            &site,
            "r/two",
            Severity::Warn,
            "m".into(),
            "f".into(),
        );
        assert_eq!((from_site.line, from_site.column), (1, 5));
        assert_eq!(from_site.context, site.context);
    }
}
