use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::facts::{FileFacts, NamedSite, Pos};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Minor,
    Major,
    Critical,
    Blocker,
}

impl Severity {
    pub const LADDER: [Severity; 5] = [
        Severity::Info,
        Severity::Minor,
        Severity::Major,
        Severity::Critical,
        Severity::Blocker,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Minor => "minor",
            Self::Major => "major",
            Self::Critical => "critical",
            Self::Blocker => "blocker",
        }
    }

    pub fn parse(text: &str) -> Option<Severity> {
        match text {
            "info" => Some(Severity::Info),
            "minor" => Some(Severity::Minor),
            "major" => Some(Severity::Major),
            "critical" => Some(Severity::Critical),
            "blocker" => Some(Severity::Blocker),
            _ => None,
        }
    }

    pub fn lowered(self, rungs: u8) -> Option<Severity> {
        let index = Self::LADDER.iter().position(|&rung| rung == self)?;
        index
            .checked_sub(usize::from(rungs))
            .and_then(|next| Self::LADDER.get(next).copied())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(text: &str) -> Option<Confidence> {
        match text {
            "low" => Some(Confidence::Low),
            "medium" => Some(Confidence::Medium),
            "high" => Some(Confidence::High),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Off,
    At(Severity),
}

impl Level {
    pub(crate) fn rung(self) -> Option<Severity> {
        match self {
            Self::Off => None,
            Self::At(severity) => Some(severity),
        }
    }
}

impl Serialize for Level {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Off => serializer.serialize_str("off"),
            Self::At(severity) => serializer.serialize_str(severity.as_str()),
        }
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "off" {
            return Ok(Level::Off);
        }
        Severity::parse(&text).map(Level::At).ok_or_else(|| {
            serde::de::Error::unknown_variant(
                &text,
                &["off", "info", "minor", "major", "critical", "blocker"],
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailOn {
    At(Severity),
    Never,
}

impl FailOn {
    pub fn fails(self, severity: Severity) -> bool {
        match self {
            Self::Never => false,
            Self::At(threshold) => severity >= threshold,
        }
    }
}

impl Serialize for FailOn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Never => serializer.serialize_str("never"),
            Self::At(severity) => serializer.serialize_str(severity.as_str()),
        }
    }
}

impl<'de> Deserialize<'de> for FailOn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "never" {
            return Ok(FailOn::Never);
        }
        Severity::parse(&text).map(FailOn::At).ok_or_else(|| {
            serde::de::Error::unknown_variant(
                &text,
                &["info", "minor", "major", "critical", "blocker", "never"],
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule: Box<str>,
    pub severity: Severity,
    pub confidence: Confidence,
    pub path: Box<str>,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub fix: String,
    pub context: Box<str>,
    pub fingerprint: Box<str>,
    #[serde(skip)]
    pub context_pending: bool,
    #[serde(skip)]
    pub file_wide: bool,
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
        finding.file_wide = false;
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
            confidence: Confidence::Medium,
            path: path.into(),
            line: 1,
            column: 1,
            message,
            fix,
            context: Box::from(""),
            fingerprint: Box::from(""),
            context_pending: false,
            file_wide: true,
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
            confidence: Confidence::Medium,
            path: text.path.into(),
            line: pos.line,
            column,
            message,
            fix,
            context,
            fingerprint: Box::from(""),
            context_pending: false,
            file_wide: false,
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
    let rest = line.trim_start();
    let mut buf = String::with_capacity(MAX_CONTEXT_CHARS + 3);
    let mut count = 0usize;
    let mut extra: Option<char> = None;
    let mut end = rest.len();
    for (at, c) in rest.char_indices() {
        end = at + c.len_utf8();
        if c == '\r' {
            continue;
        }
        if count < MAX_CONTEXT_CHARS {
            buf.push(c);
            count += 1;
        } else {
            extra = Some(c);
            break;
        }
    }
    match extra {
        Some(c) if !c.is_whitespace() => {
            buf.push_str("...");
            Cow::Owned(buf)
        }
        Some(c) => {
            if rest[end..].chars().any(|next| !next.is_whitespace()) {
                buf.push_str("...");
                Cow::Owned(buf)
            } else {
                buf.push(c);
                Cow::Owned(clip_context(buf.trim_end()).into_owned())
            }
        }
        None => {
            let keep = buf.trim_end().len();
            buf.truncate(keep);
            Cow::Owned(buf)
        }
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
    fn orders_and_serializes_ladder() {
        assert!(
            Severity::Info < Severity::Minor
                && Severity::Minor < Severity::Major
                && Severity::Major < Severity::Critical
                && Severity::Critical < Severity::Blocker,
            "ladder should order info < minor < major < critical < blocker"
        );
        assert!(
            Level::Off < Level::At(Severity::Info)
                && Level::At(Severity::Info) < Level::At(Severity::Blocker),
            "levels should order Off below every rung"
        );
        assert!(
            FailOn::At(Severity::Info) < FailOn::Never
                && FailOn::At(Severity::Blocker) < FailOn::Never,
            "fail-on thresholds should order every rung below Never"
        );
        assert_eq!(sonic_rs::to_string(&Severity::Minor).unwrap(), "\"minor\"");
        assert_eq!(sonic_rs::to_string(&Confidence::High).unwrap(), "\"high\"");
        assert_eq!(sonic_rs::to_string(&FailOn::Never).unwrap(), "\"never\"");
        assert_eq!(
            sonic_rs::to_string(&Level::At(Severity::Major)).unwrap(),
            "\"major\""
        );
        let level: Level = sonic_rs::from_str("\"major\"").unwrap();
        assert_eq!(level, Level::At(Severity::Major));
        let off: Level = sonic_rs::from_str("\"off\"").unwrap();
        assert_eq!(off, Level::Off);
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
        let from_source = Finding::at_source(
            &text,
            pos,
            "r/one",
            Severity::Blocker,
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
            Severity::Minor,
            "m".into(),
            "f".into(),
        );
        assert_eq!((from_site.line, from_site.column), (1, 5));
        assert!(
            from_site.context.is_empty() && from_site.context_pending,
            "a site finding defers its context to the merge step"
        );
    }
    #[test]
    fn carriage_return_outside_window_leaves_context_unchanged() {
        let mut beyond = "a".repeat(100_000);
        beyond.replace_range(90_000..90_001, "\r");
        let (_, context) = source_site(&beyond, Pos { line: 1, byte: 0 });
        let expected_beyond = "a".repeat(MAX_CONTEXT_CHARS) + "...";
        assert_eq!(
            context.as_ref(),
            expected_beyond.as_str(),
            "CR past the window must not reach context {context:?}"
        );
        let mut inside = "b".repeat(100_000);
        inside.replace_range(100..101, "\r");
        let (_, context) = source_site(&inside, Pos { line: 1, byte: 0 });
        let expected_inside = "b".repeat(MAX_CONTEXT_CHARS) + "...";
        assert_eq!(
            context.as_ref(),
            expected_inside.as_str(),
            "CR inside the window must be stripped from context {context:?}"
        );
    }
}
