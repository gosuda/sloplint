//! Visible escaping of terminal control characters in untrusted text.

use std::borrow::Cow;
use std::fmt;

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
        if self.text.chars().any(|c| self.is_control(c)) {
            Cow::Owned(self.to_string())
        } else {
            Cow::Borrowed(self.text)
        }
    }

    fn is_control(&self, c: char) -> bool {
        match c {
            '\t' => false,
            '\n' => !self.keep_newlines,
            '\0'..='\u{1f}' | '\u{7f}'..='\u{9f}' => true,
            _ => false,
        }
    }
}

impl fmt::Display for TerminalText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut run = 0;
        for (at, c) in self.text.char_indices() {
            if !self.is_control(c) {
                continue;
            }
            f.write_str(&self.text[run..at])?;
            if c.is_ascii() {
                write!(f, "\\x{:02x}", u32::from(c))?;
            } else {
                write!(f, "\\u{{{:x}}}", u32::from(c))?;
            }
            run = at + c.len_utf8();
        }
        f.write_str(&self.text[run..])
    }
}
