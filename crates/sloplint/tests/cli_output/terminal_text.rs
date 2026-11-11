//! TerminalText unit cases: exact control coverage, block edge scanning, line
//! against block rendering, idempotence, and bidi controls. Cases observed
//! through the binary stay in terminal_escapes.

use std::borrow::Cow;

use sloplint::TerminalText;

use super::terminal_escapes::{BIDI_CONTROLS, BIDI_CONTROLS_VISIBLE};

// A027: the shared formatter escapes the documented controls and nothing around them.
#[test]
fn terminal_text_escapes_exactly_the_documented_controls() {
    let controls = "\u{0}\u{1}\u{7}\u{1b}\u{1f}\u{7f}\u{80}\u{9b}\u{9f}";
    assert_eq!(
        TerminalText::line(controls).to_string(),
        "\\x00\\x01\\x07\\x1b\\x1f\\x7f\\u{80}\\u{9b}\\u{9f}",
        "C0, DEL, and C1 must each show as an escape"
    );
    let untouched = concat!(
        "tab\t space ~ quote\" backslash\\ ",
        "\u{a0}\u{a1}\u{e9}\u{200d}\u{202f}\u{1f980}"
    );
    assert_eq!(
        TerminalText::line(untouched).to_string(),
        untouched,
        "line mode must not change tabs, quotes, or non-control Unicode"
    );
    assert_eq!(
        TerminalText::block(untouched).to_string(),
        untouched,
        "block mode must not change tabs, quotes, or non-control Unicode"
    );
}

// The scanner skips 16-byte blocks that hold no control byte, so every control kind must be
// found at every offset around a block edge, in both renderings, with the same escapes as a
// character-by-character reference.
#[test]
fn terminal_text_finds_controls_at_every_offset_around_block_edges() {
    fn reference(text: &str, keep_newlines: bool) -> String {
        let mut shown = String::new();
        for c in text.chars() {
            match c {
                '\t' => shown.push(c),
                '\n' if keep_newlines => shown.push(c),
                '\0'..='\u{1f}' | '\u{7f}' => shown.push_str(&format!("\\x{:02x}", u32::from(c))),
                '\u{80}'..='\u{9f}'
                | '\u{61c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}' => shown.push_str(&format!("\\u{{{:x}}}", u32::from(c))),
                _ => shown.push(c),
            }
        }
        shown
    }
    let controls = [
        '\u{0}', '\n', '\u{1b}', '\u{1f}', '\u{7f}', '\u{80}', '\u{9b}', '\u{9f}', '\u{a0}',
        '\u{61c}', '\u{202e}', '\u{2069}',
    ];
    let texts = controls.iter().flat_map(|control| {
        ["a", "\u{e9}", "\t"].into_iter().flat_map(move |filler| {
            (0..40)
                .map(move |pad| format!("{}{control}{}", filler.repeat(pad), "z".repeat(pad % 17)))
        })
    });
    for text in texts {
        for (shown, keep_newlines) in [
            (TerminalText::line(&text), false),
            (TerminalText::block(&text), true),
        ] {
            let expected = reference(&text, keep_newlines);
            let mut written = Vec::new();
            shown
                .write_escaped(&mut written)
                .unwrap_or_else(|e| panic!("write to a Vec: {e}"));
            assert_eq!(shown.to_string(), expected, "Display of {text:?}");
            assert_eq!(written, expected.as_bytes(), "write_escaped of {text:?}");
            assert_eq!(shown.into_text(), expected, "into_text of {text:?}");
        }
    }
}

// A027: a field stays on one output line, while block text keeps its line feeds.
#[test]
fn terminal_text_line_escapes_line_breaks_and_block_keeps_line_feeds() {
    assert_eq!(
        TerminalText::line("a\nb\rc").to_string(),
        "a\\x0ab\\x0dc",
        "line mode must escape both the line feed and the carriage return"
    );
    assert_eq!(
        TerminalText::block("a\nb\rc").to_string(),
        "a\nb\\x0dc",
        "block mode must keep the line feed and escape the carriage return"
    );
}

// A027: clean text is borrowed, and escaping already-escaped text changes nothing.
#[test]
fn terminal_text_borrows_clean_text_and_escaping_is_idempotent() {
    let clean = "no controls \u{e9}\t";
    assert!(
        matches!(TerminalText::line(clean).into_text(), Cow::Borrowed(text) if text == clean),
        "clean text was copied"
    );
    let once = TerminalText::line("a\u{1b}[2J\u{9b}")
        .into_text()
        .into_owned();
    assert_eq!(
        once, "a\\x1b[2J\\u{9b}",
        "ESC and the C1 character must show as escapes"
    );
    assert!(
        matches!(TerminalText::line(&once).into_text(), Cow::Borrowed(_)),
        "escaped text still holds a control character"
    );
    assert_eq!(
        TerminalText::line(&once).to_string(),
        once,
        "escaping already-escaped text must not change it"
    );
}

#[test]
fn terminal_text_escapes_unicode_bidi_controls() {
    assert_eq!(
        TerminalText::line(BIDI_CONTROLS).to_string(),
        BIDI_CONTROLS_VISIBLE,
        "Unicode bidi controls must each show as an escape"
    );
    assert_eq!(
        TerminalText::block(BIDI_CONTROLS).to_string(),
        BIDI_CONTROLS_VISIBLE,
        "Unicode bidi controls in block mode must each show as an escape"
    );
}
