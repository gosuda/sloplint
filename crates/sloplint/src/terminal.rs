//! Visible escaping of terminal control characters in untrusted text.

use std::borrow::Cow;
use std::fmt;
use std::io;

/// Escapes terminal and bidi controls (CVE-2021-42574) in untrusted text before display.
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
        if next_control(self.text.as_bytes(), self.keep_newlines, 0).is_some() {
            Cow::Owned(self.to_string())
        } else {
            Cow::Borrowed(self.text)
        }
    }

    pub fn write_escaped<W: io::Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let bytes = self.text.as_bytes();
        let mut run = 0;
        let mut at = 0;
        while let Some((off, len, code)) = next_control(bytes, self.keep_newlines, at) {
            out.write_all(&bytes[run..off])?;
            write_escape(out, len, code)?;
            run = off + len;
            at = run;
        }
        out.write_all(&bytes[run..])
    }
}

fn next_control(bytes: &[u8], keep_newlines: bool, from: usize) -> Option<(usize, usize, u32)> {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at..].first_chunk::<16>().is_some_and(plain_block) {
            at += 16;
        } else if let Some((len, code)) = control_at(bytes, at, keep_newlines) {
            return Some((at, len, code));
        } else {
            at += 1;
        }
    }
    None
}

fn control_at(bytes: &[u8], at: usize, keep_newlines: bool) -> Option<(usize, u32)> {
    match bytes[at] {
        b'\t' => None,
        b'\n' if keep_newlines => None,
        byte @ (0x00..=0x1f | 0x7f) => Some((1, u32::from(byte))),
        0xC2 => bytes
            .get(at + 1)
            .copied()
            .filter(|next| (0x80..=0x9f).contains(next))
            .map(|next| (2, u32::from(next))),
        0xD8 => bytes
            .get(at + 1)
            .copied()
            .filter(|&next| next == 0x9C)
            .map(|_| (2, 0x061c)),
        0xE2 => match (bytes.get(at + 1).copied(), bytes.get(at + 2).copied()) {
            (Some(b1), Some(b2)) => {
                let code = (2 << 12) | (u32::from(b1 & 0x3f) << 6) | u32::from(b2 & 0x3f);
                match code {
                    0x200e | 0x200f | 0x202a..=0x202e | 0x2066..=0x2069 => Some((3, code)),
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

fn write_escape<W: io::Write + ?Sized>(out: &mut W, len: usize, code: u32) -> io::Result<()> {
    if len == 1 {
        let hi = usize::try_from((code >> 4) & 0x0f).expect("hex digit fits usize");
        let lo = usize::try_from(code & 0x0f).expect("hex digit fits usize");
        out.write_all(&[b'\\', b'x', HEX[hi], HEX[lo]])
    } else {
        let mut buf = [0u8; 10];
        buf[0] = b'\\';
        buf[1] = b'u';
        buf[2] = b'{';
        let mut cursor = 3;
        let bits = 32 - code.leading_zeros();
        let shift = bits.saturating_sub(1) / 4 * 4;
        let mut s = shift;
        loop {
            let digit = usize::try_from((code >> s) & 0x0f).expect("hex digit fits usize");
            buf[cursor] = HEX[digit];
            cursor += 1;
            if s < 4 {
                break;
            }
            s -= 4;
        }
        buf[cursor] = b'}';
        cursor += 1;
        out.write_all(&buf[..cursor])
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

impl fmt::Display for TerminalText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.text.as_bytes();
        let mut run = 0;
        let mut at = 0;
        while let Some((off, len, code)) = next_control(bytes, self.keep_newlines, at) {
            f.write_str(&self.text[run..off])?;
            if len == 1 {
                write!(f, "\\x{:02x}", code)?;
            } else {
                write!(f, "\\u{{{:x}}}", code)?;
            }
            run = off + len;
            at = run;
        }
        f.write_str(&self.text[run..])
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::TerminalText;

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
}
