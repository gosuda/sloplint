//! Visible escaping of terminal control characters in untrusted text.

use std::borrow::Cow;
use std::fmt;
use std::io;

/// Untrusted text that displays with terminal control characters made visible.
///
/// Source lines, file names, and configuration values can hold bytes that a terminal
/// interprets as commands, such as an escape sequence that writes the clipboard or moves
/// the cursor over earlier output. Writing a `TerminalText` instead of the raw text
/// replaces each C0 control character (except tab), DEL, and each C1 control character
/// with an escape that shows the code point:
///
/// - ASCII controls and DEL become `\x` plus two lowercase hex digits, such as `\x1b`
///   and `\x7f`.
/// - C1 controls (U+0080 to U+009F) become `\u{..}` with lowercase hex digits, such as
///   `\u{9b}`.
///
/// Every other character, including tab, quotes, backslash, and all other Unicode, is
/// written unchanged. The line feed and carriage return are controls: [`line`] escapes
/// them so one field stays on one output line, while [`block`] keeps the line feed for
/// multi-line text.
///
/// Escaping is idempotent: text that is already escaped contains no control characters,
/// so a second pass changes nothing.
///
/// [`line`]: TerminalText::line
/// [`block`]: TerminalText::block
///
/// # Examples
///
/// ```
/// use sloplint::TerminalText;
///
/// let shown = TerminalText::line("a\u{1b}]52;c;Zm9v\u{7}b\u{9b}2J\tc").to_string();
/// assert_eq!(shown, "a\\x1b]52;c;Zm9v\\x07b\\u{9b}2J\tc");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct TerminalText<'a> {
    text: &'a str,
    keep_newlines: bool,
}

impl<'a> TerminalText<'a> {
    /// Wraps text that belongs on one output line: the line feed and carriage return are
    /// escaped along with every other control character.
    pub fn line(text: &'a str) -> Self {
        TerminalText {
            text,
            keep_newlines: false,
        }
    }

    /// Wraps multi-line text: the line feed is kept as the line separator, and every other
    /// control character is escaped.
    pub fn block(text: &'a str) -> Self {
        TerminalText {
            text,
            keep_newlines: true,
        }
    }

    /// Returns the text itself, borrowed, when it holds no control character, and an
    /// escaped copy otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use sloplint::TerminalText;
    ///
    /// assert!(matches!(TerminalText::line("plain \u{e9}").into_text(), Cow::Borrowed(_)));
    /// assert_eq!(TerminalText::line("a\nb").into_text(), "a\\x0ab");
    /// assert_eq!(TerminalText::block("a\nb").into_text(), "a\nb");
    /// ```
    pub fn into_text(self) -> Cow<'a, str> {
        if next_control(self.text.as_bytes(), self.keep_newlines, 0).is_some() {
            Cow::Owned(self.to_string())
        } else {
            Cow::Borrowed(self.text)
        }
    }

    /// Writes the escaped text directly to `out` without [`fmt`] machinery.
    ///
    /// Clean byte runs go out with [`io::Write::write_all`] and each control is replaced
    /// by its escape bytes, so the emitted bytes equal the [`Display`](fmt::Display)
    /// rendering exactly while skipping the formatter.
    ///
    /// # Examples
    ///
    /// ```
    /// use sloplint::TerminalText;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut out = Vec::new();
    /// TerminalText::line("a\x1b[2J\u{9b}").write_escaped(&mut out)?;
    /// assert_eq!(out.as_slice(), b"a\\x1b[2J\\u{9b}".as_slice());
    /// # Ok(())
    /// # }
    /// ```
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

/// Scans `bytes` from `from` for the next escaped control.
///
/// Returns the control's byte offset, byte length, and code point value. ASCII controls
/// and DEL have length 1 with the byte as the value; C1 controls have length 2 with the
/// trailing byte's value as the code point. Tab is never a control, and the line feed is
/// skipped when `keep_newlines` holds.
fn next_control(bytes: &[u8], keep_newlines: bool, from: usize) -> Option<(usize, usize, u8)> {
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

/// The byte length and code point value of the control starting at `at`, if one does.
fn control_at(bytes: &[u8], at: usize, keep_newlines: bool) -> Option<(usize, u8)> {
    match bytes[at] {
        b'\t' => None,
        b'\n' if keep_newlines => None,
        byte @ (0x00..=0x1f | 0x7f) => Some((1, byte)),
        0xC2 => bytes
            .get(at + 1)
            .copied()
            .filter(|next| (0x80..=0x9f).contains(next))
            .map(|next| (2, next)),
        0x20..=0x7e | 0x80..=0xC1 | 0xC3..=0xff => None,
    }
}

/// Whether a 16-byte block holds no byte that can start a control.
///
/// Every control starts with a byte below `0x20`, the DEL byte, or the `0xC2` lead byte of
/// a C1 character, so a block without them is skipped whole. The fold has no early exit,
/// so it compiles to one vector compare.
fn plain_block(block: &[u8; 16]) -> bool {
    !block.iter().fold(false, |hit, &byte| {
        hit | (byte < 0x20) | (byte == 0x7f) | (byte == 0xC2)
    })
}

/// Writes the escape bytes for one control found by [`next_control`].
///
/// Length 1 writes `\x` plus two lowercase hex digits; length 2 writes `\u{..}` with
/// lowercase hex digits, matching the [`Display`](fmt::Display) rendering exactly.
fn write_escape<W: io::Write + ?Sized>(out: &mut W, len: usize, code: u8) -> io::Result<()> {
    if len == 1 {
        out.write_all(&[
            b'\\',
            b'x',
            HEX[(code >> 4) as usize],
            HEX[(code & 0x0f) as usize],
        ])
    } else {
        out.write_all(&[
            b'\\',
            b'u',
            b'{',
            HEX[((code >> 4) & 0x0f) as usize],
            HEX[(code & 0x0f) as usize],
            b'}',
        ])
    }
}

/// Lowercase hex digits used by [`write_escape`].
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
