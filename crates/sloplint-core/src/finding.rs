use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::facts::{FileFacts, NamedSite, Pos};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warn,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    Warn,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    Warn,
    Deny,
    Never,
}

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
    #[serde(skip)]
    pub context_pending: bool,
}

const MAX_CONTEXT_CHARS: usize = 160;

#[derive(Debug, Clone, Copy)]
pub struct SourceText<'a> {
    pub path: &'a str,
    pub source: &'a str,
    pub line_starts: &'a [u32],
}

impl Finding {
    pub fn at(
        facts: &FileFacts<'_>,
        pos: Pos,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        Self::at_source(
            &SourceText {
                path: facts.path,
                source: facts.source,
                line_starts: &facts.line_starts,
            },
            pos,
            rule,
            severity,
            message,
            fix,
        )
    }

    pub fn at_site(
        path: &str,
        site: &NamedSite,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        let mut finding = Self::at_file(path, rule, severity, message, fix);
        finding.line = site.line;
        finding.column = site.column;
        finding.context_pending = true;
        finding
    }

    pub fn at_file(
        path: &str,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        Finding {
            rule: rule.into(),
            severity,
            path: path.into(),
            line: 1,
            column: 1,
            message,
            fix,
            context: Box::from(""),
            fingerprint: Box::from(""),
            context_pending: false,
        }
    }

    pub fn at_source(
        text: &SourceText<'_>,
        pos: Pos,
        rule: &str,
        severity: Severity,
        message: String,
        fix: String,
    ) -> Finding {
        let (column, context) = source_site_indexed(text.line_starts, text.source, pos);
        Finding {
            rule: rule.into(),
            severity,
            path: text.path.into(),
            line: pos.line,
            column,
            message,
            fix,
            context,
            fingerprint: Box::from(""),
            context_pending: false,
        }
    }
}

pub fn source_site(source: &str, pos: Pos) -> (u32, Box<str>) {
    let (start, text) = line_at(source, pos.line);
    (
        column_in(text, offset(pos.byte).saturating_sub(start)),
        Box::from(context_of(text)),
    )
}

pub fn line_starts(source: &str) -> Vec<u32> {
    let mut starts = vec![0];
    for (at, _) in source.match_indices('\n') {
        starts.push(u32::try_from(at + 1).unwrap_or(u32::MAX));
    }
    starts
}

pub fn source_site_indexed(starts: &[u32], source: &str, pos: Pos) -> (u32, Box<str>) {
    let Some((start, end)) = line_span(starts, source, pos.line) else {
        return (1, Box::from(""));
    };
    let text = source.get(start..end).unwrap_or("");
    (
        column_in(text, offset(pos.byte).saturating_sub(start)),
        Box::from(context_of(text)),
    )
}

pub fn source_column(starts: &[u32], source: &str, pos: Pos) -> u32 {
    let Some((start, end)) = line_span(starts, source, pos.line) else {
        return 1;
    };
    let text = source.get(start..end).unwrap_or("");
    column_in(text, offset(pos.byte).saturating_sub(start))
}

pub fn line_context(starts: &[u32], source: &str, line: u32) -> Box<str> {
    let Some((start, end)) = line_span(starts, source, line) else {
        return Box::from("");
    };
    let text = source.get(start..end).unwrap_or("");
    Box::from(context_of(text))
}

fn line_span(starts: &[u32], source: &str, line: u32) -> Option<(usize, usize)> {
    let index = offset(line.saturating_sub(1));
    let start = offset(*starts.get(index)?);
    let end = match starts.get(index + 1) {
        Some(&next) => offset(next).saturating_sub(1),
        None => source.len(),
    };
    Some((start, end))
}

fn column_in(text: &str, byte_offset: usize) -> u32 {
    let target = byte_offset.min(text.len());
    let chars = match text.get(..target) {
        Some(prefix) if prefix.is_ascii() => target,
        _ => text
            .char_indices()
            .take_while(|(offset, _)| *offset < target)
            .count(),
    };
    u32::try_from(chars).unwrap_or(u32::MAX).saturating_add(1)
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
        };
        let text = SourceText {
            path: "a.rs",
            source,
            line_starts: &line_starts(source),
        };
        let from_source =
            Finding::at_source(&text, pos, "r/one", Severity::Deny, "m".into(), "f".into());
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
        assert!(
            from_site.context.is_empty() && from_site.context_pending,
            "a site finding defers its context to the merge step"
        );
    }
}
