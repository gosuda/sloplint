//! Terminal control characters in source, paths, and configuration never reach the user raw.
//!
//! The binary is run with `CLICOLOR_FORCE=1` so anstream passes bytes through even on a pipe,
//! which is exactly what a terminal receives. Without it a pipe strips escape sequences and the
//! defect stays invisible. Controls are C0 (except tab and the line feed that ends a line),
//! DEL, and C1; everything else, including valid non-ASCII text, must come through unchanged.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

use serde::Deserialize;
use sloplint::TerminalText;

const HOSTILE_LINE: &str = "pub fn handle() {} // \u{1b}]52;c;Zm9v\u{7} \u{9b}2J \u{7f} end";
const HOSTILE_LINE_VISIBLE: &str = "pub fn handle() {} // \\x1b]52;c;Zm9v\\x07 \\u{9b}2J \\x7f end";
const WHY_VISIBLE: &str = "why \\x1b[2K\\u{9b}";
const FIX_VISIBLE: &str = "fix \\x1b[1A\\x7f";
const MESSAGE_VISIBLE: &str = "bad \\x1b]0;pwn\\x07 macro dbg";
const CUSTOM_RULE: &str = "custom/no-dbg-macro";
const CONFIG: &str = r#"[[custom_rules]]
id = "custom/no-dbg-macro"
language = "rust"
severity = "warn"
message = "bad \u001b]0;pwn\u0007 macro {m}"
why = "why \u001b[2K\u009b"
fix = "fix \u001b[1A\u007f"
query = '''
(macro_invocation macro: (identifier) @m (#eq? @m "dbg")) @match
'''
"#;
const UNICODE_LINE: &str = concat!(
    "pub fn handle() {} // tab\there \u{e9} \u{a9}\u{a0}\u{ff} ",
    "\u{1f980} e\u{301} \u{1f468}\u{200d}\u{1f469}"
);

#[derive(Deserialize)]
struct Row {
    rule: String,
    path: String,
    message: String,
    fix: String,
    context: String,
}

#[derive(Deserialize)]
struct Document {
    findings: Vec<Row>,
    rules: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RuleRow {
    id: String,
    summary: String,
    why: String,
}

#[derive(Deserialize)]
struct RuleList {
    rules: Vec<RuleRow>,
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    for (rel, body) in files {
        write(tmp.path(), rel, body);
    }
    tmp
}

fn hostile_project() -> tempfile::TempDir {
    let tmp = project(&[
        ("sloplint.toml", CONFIG),
        ("a.rs", format!("{HOSTILE_LINE}\n").as_str()),
        ("c.rs", "fn c() { dbg!(1); }\n"),
    ]);
    #[cfg(unix)]
    write(
        tmp.path(),
        "dir/\u{1b}[2J\u{9b}evil\u{7}.rs",
        "pub fn handle() {}\n",
    );
    tmp
}

fn sloplint(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(args)
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("output is not UTF-8: {e}"))
}

fn assert_inert(bytes: &[u8], label: &str) {
    for (at, &byte) in bytes.iter().enumerate() {
        let control = (byte < 0x20 && byte != b'\n' && byte != b'\t') || byte == 0x7f;
        assert!(
            !control,
            "{label}: raw control byte {byte:#04x} at offset {at}"
        );
    }
    let c1 = bytes
        .windows(2)
        .any(|pair| pair[0] == 0xc2 && (0x80..=0x9f).contains(&pair[1]));
    assert!(!c1, "{label}: raw C1 control character");
}

fn without_sgr(bytes: &[u8]) -> Vec<u8> {
    let mut kept = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'[') {
            let tail = &bytes[at + 2..];
            let params = tail
                .iter()
                .take_while(|byte| byte.is_ascii_digit() || **byte == b';')
                .count();
            if tail.get(params) == Some(&b'm') {
                at += params + 3;
                continue;
            }
        }
        kept.push(bytes[at]);
        at += 1;
    }
    kept
}

fn assert_contains(haystack: &str, needle: &str, label: &str) {
    assert!(
        haystack.contains(needle),
        "{label}: missing {needle:?} in:\n{haystack}"
    );
}

// A027: agent lines carry path, message, context, fix, and rationale; each is escaped.
#[test]
fn agent_output_shows_controls_as_visible_escapes() {
    let tmp = hostile_project();
    let out = sloplint(tmp.path(), &["check", "--no-cache"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_inert(&out.stdout, "agent stdout");
    let stdout = text(&out.stdout);
    assert_contains(
        stdout,
        &format!("  context: {HOSTILE_LINE_VISIBLE}\n"),
        "context",
    );
    assert_contains(
        stdout,
        &format!("warn: {MESSAGE_VISIBLE} [{CUSTOM_RULE}]\n"),
        "message",
    );
    assert_contains(stdout, &format!("  fix: {FIX_VISIBLE}\n"), "fix");
    assert_contains(
        stdout,
        &format!("  {CUSTOM_RULE}: {WHY_VISIBLE}\n"),
        "rationale",
    );
    #[cfg(unix)]
    assert_contains(
        stdout,
        "dir/\\x1b[2J\\u{9b}evil\\x07.rs:1:8: warn:",
        "path with controls",
    );
}

// A027: the human renderer already normalizes C0 in snippets, but not C1, paths, help, or note.
#[test]
fn human_output_shows_controls_as_visible_escapes() {
    let tmp = hostile_project();
    let out = sloplint(tmp.path(), &["check", "--no-cache", "--format", "human"]);
    assert_eq!(out.status.code(), Some(0), "human exit code");
    let plain = without_sgr(&out.stdout);
    assert_inert(&plain, "human stdout without styling");
    let stdout = text(&plain);
    assert_contains(stdout, HOSTILE_LINE_VISIBLE, "snippet line");
    assert_contains(stdout, MESSAGE_VISIBLE, "title");
    assert_contains(stdout, &format!("help: {FIX_VISIBLE}\n"), "help line");
    assert_contains(stdout, &format!("note: {WHY_VISIBLE}\n"), "note line");
    #[cfg(unix)]
    assert_contains(
        stdout,
        "dir/\\x1b[2J\\u{9b}evil\\x07.rs:1:8",
        "snippet path",
    );
}

// A027: escapes are longer than the control they replace, so the underline must be remapped.
#[test]
fn human_underline_stays_under_the_finding_after_escaping() {
    let tmp = project(&[
        ("sloplint.toml", ""),
        ("a.rs", "/*\u{1b}\u{9b}*/ pub fn handle() {}\n"),
    ]);
    let out = sloplint(tmp.path(), &["check", "--no-cache", "--format", "human"]);
    let plain = without_sgr(&out.stdout);
    assert_inert(&plain, "human stdout without styling");
    let rows: Vec<&str> = text(&plain).lines().collect();
    let source = rows
        .iter()
        .position(|row| row.contains("pub fn handle"))
        .unwrap_or_else(|| panic!("no snippet row in:\n{}", text(&plain)));
    let underline = rows
        .get(source + 1)
        .unwrap_or_else(|| panic!("no underline row in:\n{}", text(&plain)));
    let name_at = rows[source]
        .find("handle")
        .map(|byte| rows[source][..byte].chars().count());
    let mark_at = underline.chars().position(|c| c == '\u{2501}');
    assert!(name_at.is_some(), "snippet row lacks the name");
    assert_eq!(
        mark_at, name_at,
        "underline column differs from the name column:\n{}\n{}",
        rows[source], underline
    );
}

// A027: JSON keeps raw semantic data; the serializer escapes C0, so only DEL and C1 need care.
#[test]
fn json_output_round_trips_every_character_without_raw_controls() {
    let tmp = hostile_project();
    let out = sloplint(tmp.path(), &["check", "--no-cache", "--format", "json"]);
    assert_eq!(out.status.code(), Some(0), "json exit code");
    assert_inert(&out.stdout, "json stdout");
    let doc: Document =
        sonic_rs::from_slice(&out.stdout).unwrap_or_else(|e| panic!("stdout is not JSON: {e}"));
    let custom = doc
        .findings
        .iter()
        .find(|row| row.rule == CUSTOM_RULE)
        .unwrap_or_else(|| panic!("no {CUSTOM_RULE} finding"));
    assert_eq!(
        custom.message, "bad \u{1b}]0;pwn\u{7} macro dbg",
        "the JSON message must keep its raw control characters"
    );
    assert_eq!(
        custom.fix, "fix \u{1b}[1A\u{7f}",
        "the JSON fix must keep its raw ESC and DEL"
    );
    assert_eq!(
        doc.rules.get(CUSTOM_RULE).map(String::as_str),
        Some("why \u{1b}[2K\u{9b}"),
        "the JSON rationale must keep its raw ESC and C1 character"
    );
    let line = doc
        .findings
        .iter()
        .find(|row| row.path == "a.rs")
        .unwrap_or_else(|| panic!("no finding for a.rs"));
    assert_eq!(
        line.context, HOSTILE_LINE,
        "the JSON context must keep the source line byte for byte"
    );
    #[cfg(unix)]
    assert!(
        doc.findings
            .iter()
            .any(|row| row.path == "dir/\u{1b}[2J\u{9b}evil\u{7}.rs"),
        "path with controls did not round-trip"
    );
}

// A027: `rules` prints configuration text; agent text is escaped and JSON stays lossless.
#[test]
fn rules_command_escapes_text_and_keeps_json_lossless() {
    let tmp = hostile_project();
    let agent = sloplint(tmp.path(), &["rules", CUSTOM_RULE]);
    assert_eq!(agent.status.code(), Some(0), "rules exit code");
    assert_inert(&agent.stdout, "rules stdout");
    assert_contains(
        text(&agent.stdout),
        &format!("{CUSTOM_RULE} level=warn | bad \\x1b]0;pwn\\x07 macro {{m}}\n"),
        "rules summary line",
    );
    assert_contains(
        text(&agent.stdout),
        &format!("why: {WHY_VISIBLE}\n"),
        "rules why line",
    );
    let json = sloplint(tmp.path(), &["rules", "--format", "json"]);
    assert_inert(&json.stdout, "rules json stdout");
    let list: RuleList = sonic_rs::from_slice(&json.stdout)
        .unwrap_or_else(|e| panic!("rules stdout is not JSON: {e}"));
    let custom = list
        .rules
        .iter()
        .find(|rule| rule.id == CUSTOM_RULE)
        .unwrap_or_else(|| panic!("rules lists no {CUSTOM_RULE}"));
    assert_eq!(
        custom.why, "why \u{1b}[2K\u{9b}",
        "rules JSON must keep the raw rationale"
    );
    assert_eq!(
        custom.summary, "bad \u{1b}]0;pwn\u{7} macro {m}",
        "rules JSON must keep the raw summary"
    );
}

// A027: error text echoes configuration values and goes to stderr outside anstream.
#[test]
fn error_text_shows_controls_as_visible_escapes() {
    let tmp = project(&[(
        "sloplint.toml",
        "[rules.\"bad\\u001b]0;x\\u0007\"]\nlevel = \"warn\"\n",
    )]);
    let out = sloplint(tmp.path(), &["check", "--no-cache"]);
    assert_eq!(out.status.code(), Some(2), "config error exit code");
    assert_inert(&out.stderr, "stderr");
    assert_contains(
        text(&out.stderr),
        "unknown rule `bad\\x1b]0;x\\x07`",
        "stderr",
    );
    assert!(out.stdout.is_empty(), "stdout must stay empty on error");
}

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

// A027 control: valid Unicode, tabs, and ordinary text are never altered or escaped.
#[test]
fn valid_unicode_and_tabs_pass_through_unchanged() {
    let tmp = project(&[
        ("sloplint.toml", ""),
        ("a.rs", format!("{UNICODE_LINE}\n").as_str()),
    ]);
    let agent = sloplint(tmp.path(), &["check", "--no-cache"]);
    assert_inert(&agent.stdout, "agent stdout");
    assert_contains(
        text(&agent.stdout),
        &format!("  context: {UNICODE_LINE}\n"),
        "agent context",
    );
    let json = sloplint(tmp.path(), &["check", "--no-cache", "--format", "json"]);
    let doc: Document =
        sonic_rs::from_slice(&json.stdout).unwrap_or_else(|e| panic!("stdout is not JSON: {e}"));
    let row = doc
        .findings
        .first()
        .unwrap_or_else(|| panic!("no finding for the Unicode line"));
    assert_eq!(
        row.context, UNICODE_LINE,
        "the JSON context must round-trip valid Unicode and tabs"
    );
    let human = sloplint(tmp.path(), &["check", "--no-cache", "--format", "human"]);
    let plain = without_sgr(&human.stdout);
    assert!(
        !text(&plain).contains("\\x") && !text(&plain).contains("\\u{"),
        "valid text was escaped:\n{}",
        text(&plain)
    );
    assert_contains(text(&plain), "\u{e9} \u{a9}\u{a0}\u{ff}", "human snippet");
}

const BIDI_CONTROLS: &str = "\u{61c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}";
const BIDI_CONTROLS_VISIBLE: &str = "\\u{61c}\\u{200e}\\u{200f}\\u{202a}\\u{202b}\\u{202c}\\u{202d}\\u{202e}\\u{2066}\\u{2067}\\u{2068}\\u{2069}";

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

#[test]
fn bidi_controls_render_escaped_in_terminal_output_and_raw_in_json() {
    let bidi_source = format!("pub fn handle() {{}} // {BIDI_CONTROLS}\n");
    let tmp = project(&[("sloplint.toml", ""), ("a.rs", &bidi_source)]);
    #[cfg(unix)]
    write(
        tmp.path(),
        "dir/\u{202e}evil\u{202c}.rs",
        "pub fn handle() {}\n",
    );

    let human = sloplint(tmp.path(), &["check", "--no-cache", "--format", "human"]);
    assert_eq!(human.status.code(), Some(0), "human exit code");
    let human_plain = without_sgr(&human.stdout);
    let human_text = text(&human_plain);
    assert_contains(
        human_text,
        BIDI_CONTROLS_VISIBLE,
        "bidi controls in human source snippet",
    );
    #[cfg(unix)]
    assert_contains(
        human_text,
        "dir/\\u{202e}evil\\u{202c}.rs",
        "bidi controls in human path snippet",
    );

    let agent = sloplint(tmp.path(), &["check", "--no-cache", "--format", "agent"]);
    assert_eq!(agent.status.code(), Some(0), "agent exit code");
    let agent_text = text(&agent.stdout);
    assert_contains(
        agent_text,
        &format!("  context: pub fn handle() {{}} // {BIDI_CONTROLS_VISIBLE}\n"),
        "bidi controls in agent context",
    );
    #[cfg(unix)]
    assert_contains(
        agent_text,
        "dir/\\u{202e}evil\\u{202c}.rs",
        "bidi controls in agent path",
    );

    let json = sloplint(tmp.path(), &["check", "--no-cache", "--format", "json"]);
    assert_eq!(json.status.code(), Some(0), "json exit code");
    let doc: Document =
        sonic_rs::from_slice(&json.stdout).unwrap_or_else(|e| panic!("stdout is not JSON: {e}"));
    let a_finding = doc
        .findings
        .iter()
        .find(|row| row.path == "a.rs")
        .unwrap_or_else(|| panic!("no finding for a.rs"));
    assert_eq!(
        a_finding.context,
        format!("pub fn handle() {{}} // {BIDI_CONTROLS}"),
        "JSON context must keep raw bidi controls"
    );
    #[cfg(unix)]
    assert!(
        doc.findings
            .iter()
            .any(|row| row.path == "dir/\u{202e}evil\u{202c}.rs"),
        "JSON path must keep raw bidi controls"
    );
}
