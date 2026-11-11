//! Visible escaping of terminal control characters in untrusted text.

use std::borrow::Cow;
use std::fmt;
use std::io;

/// Escapes terminal and bidi controls (CVE-2021-42574) in untrusted text before display.
///
/// Line mode renders a single line: line feeds are escaped with the other controls. Block mode
/// renders a document: line feeds are structural, and a line whose first non-whitespace
/// characters would forge a CI workflow command has that first colon shown as `\x3a`.
#[derive(Debug, Clone, Copy)]
pub struct TerminalText<'a> {
    text: &'a str,
    keep_newlines: bool,
}

impl<'a> TerminalText<'a> {
    pub fn line(text: &'a str) -> Self {
        TerminalText {
            text,
            keep_newlines: false,
        }
    }

    pub fn block(text: &'a str) -> Self {
        TerminalText {
            text,
            keep_newlines: true,
        }
    }

    pub fn into_text(self) -> Cow<'a, str> {
        if self.needs_escape() {
            Cow::Owned(self.to_string())
        } else {
            Cow::Borrowed(self.text)
        }
    }

    pub fn write_escaped<W: io::Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }

    fn needs_escape(&self) -> bool {
        if self.keep_newlines {
            self.text.split('\n').any(|line| {
                command_colon(line).is_some() || next_control(line.as_bytes(), 0).is_some()
            })
        } else {
            next_control(self.text.as_bytes(), 0).is_some()
        }
    }
}

/// Reports the next control the shared table escapes at or after `from` as
/// `(offset, byte width, code point)`, or `None` when the rest is clean. The table covers C0
/// except tab, DEL, C1, and the bidi and format controls; newline policy belongs to callers.
pub fn next_control(bytes: &[u8], from: usize) -> Option<(usize, usize, u32)> {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at..].first_chunk::<16>().is_some_and(plain_block) {
            at += 16;
        } else if let Some(found) = control_at(bytes, at) {
            return Some(found);
        } else {
            at += 1;
        }
    }
    None
}

fn control_at(bytes: &[u8], at: usize) -> Option<(usize, usize, u32)> {
    match bytes[at] {
        b'\t' => None,
        byte @ (0x00..=0x1f | 0x7f) => Some((at, 1, u32::from(byte))),
        0xC2 => bytes
            .get(at + 1)
            .copied()
            .filter(|next| (0x80..=0x9f).contains(next))
            .map(|next| (at, 2, u32::from(next))),
        0xD8 => bytes
            .get(at + 1)
            .copied()
            .filter(|&next| next == 0x9C)
            .map(|_| (at, 2, 0x061c)),
        0xE2 => match (bytes.get(at + 1).copied(), bytes.get(at + 2).copied()) {
            (Some(b1), Some(b2)) => {
                let code = (2 << 12) | (u32::from(b1 & 0x3f) << 6) | u32::from(b2 & 0x3f);
                match code {
                    0x200e | 0x200f | 0x202a..=0x202e | 0x2066..=0x2069 => Some((at, 3, code)),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn plain_block(block: &[u8; 16]) -> bool {
    !block.iter().fold(false, |hit, &byte| {
        hit | (byte < 0x20) | (byte == 0x7f) | (byte == 0xC2) | (byte == 0xD8) | (byte == 0xE2)
    })
}

impl fmt::Display for TerminalText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.keep_newlines {
            return escaped_controls(self.text, f);
        }
        for (index, line) in self.text.split('\n').enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            if let Some(at) = command_colon(line) {
                escaped_controls(&line[..at], f)?;
                f.write_str("\\x3a")?;
                escaped_controls(&line[at + 1..], f)?;
            } else {
                escaped_controls(line, f)?;
            }
        }
        Ok(())
    }
}

fn command_colon(line: &str) -> Option<usize> {
    let command = line.trim_start();
    command
        .starts_with("::")
        .then(|| line.len() - command.len())
}

fn escaped_controls(text: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let bytes = text.as_bytes();
    let mut run = 0;
    let mut at = 0;
    while let Some((off, len, code)) = next_control(bytes, at) {
        f.write_str(&text[run..off])?;
        if len == 1 {
            write!(f, "\\x{code:02x}")?;
        } else {
            write!(f, "\\u{{{code:x}}}")?;
        }
        run = off + len;
        at = run;
    }
    f.write_str(&text[run..])
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{TerminalText, next_control};

    #[test]
    fn line_escapes_controls_and_keeps_tab() {
        let shown = TerminalText::line("a\u{1b}]52;c;Zm9v\u{7}b\u{9b}2J\tc").to_string();
        assert_eq!(
            shown, "a\\x1b]52;c;Zm9v\\x07b\\u{9b}2J\tc",
            "ascii controls, DEL, and C1 must render as escapes while tab stays"
        );
    }

    #[test]
    fn into_text_borrows_clean_text_and_escapes_controls() {
        assert!(
            matches!(
                TerminalText::line("plain \u{e9}").into_text(),
                Cow::Borrowed(_)
            ),
            "clean text must come back borrowed"
        );
        assert_eq!(
            TerminalText::line("a\nb").into_text(),
            "a\\x0ab",
            "line mode must escape the line feed"
        );
        assert_eq!(
            TerminalText::block("a\nb").into_text(),
            "a\nb",
            "block mode must keep the line feed"
        );
    }

    #[test]
    fn write_escaped_emits_the_display_bytes() {
        let mut out = Vec::new();
        TerminalText::line("a\x1b[2J\u{9b}")
            .write_escaped(&mut out)
            .unwrap_or_else(|e| panic!("write to a Vec: {e}"));
        assert_eq!(
            out.as_slice(),
            b"a\\x1b[2J\\u{9b}".as_slice(),
            "write_escaped must emit the Display bytes"
        );
    }

    #[test]
    fn block_defuses_workflow_commands_and_keeps_structural_lines() {
        assert_eq!(
            TerminalText::block("a\n::warning file=f").to_string(),
            "a\n\\x3a:warning file=f",
            "block mode must defuse a workflow command forged at a line start"
        );
        assert_eq!(
            TerminalText::block(":label\nmore").to_string(),
            ":label\nmore",
            "a single leading colon is not a workflow command"
        );
        assert_eq!(
            TerminalText::block("a\n::x").into_text(),
            "a\n\\x3a:x",
            "into_text must agree with Display on defused lines"
        );
        assert_eq!(
            TerminalText::block("a\n ::error::x\n\t::warning file=f\n\u{a0}::notice::y")
                .to_string(),
            "a\n \\x3a:error::x\n\t\\x3a:warning file=f\n\u{a0}\\x3a:notice::y",
            "the runner trims leading whitespace, so a padded command must be defused too"
        );
        assert_eq!(
            TerminalText::line("::warning").to_string(),
            "::warning",
            "line mode does not own the line start, so the caller's prefix governs"
        );
    }

    #[test]
    fn control_scanner_shares_the_terminal_table() {
        assert_eq!(next_control(b"a\x7f", 0), Some((1, 1, 0x7f)));
        assert_eq!(next_control("a\u{9b}".as_bytes(), 0), Some((1, 2, 0x9b)));
        assert_eq!(next_control("a\u{61c}".as_bytes(), 0), Some((1, 2, 0x61c)));
        assert_eq!(
            next_control("a\u{202e}".as_bytes(), 0),
            Some((1, 3, 0x202e))
        );
        assert_eq!(
            next_control("a\u{2069}".as_bytes(), 0),
            Some((1, 3, 0x2069))
        );
        assert_eq!(next_control(b"a\n", 0), Some((1, 1, 0x0a)));
        assert_eq!(next_control(b"a\tb", 0), None);
        assert_eq!(next_control(b"clean", 0), None);
        assert_eq!(next_control(b"clean", 3), None);
        assert_eq!(next_control("x\u{202d}y".as_bytes(), 2), None);
    }
}
