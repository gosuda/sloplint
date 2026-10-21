//! The per-file line index answers exactly what `source_site` answers, for every position,
//! including the ones `source_site` clamps, and findings and world sites read it.

use sloplint_core::Pos;
use sloplint_core::facts::WorldFacts;
use sloplint_core::finding::{Finding, Severity, line_starts, source_site, source_site_indexed};
use sloplint_core::lang::Lang;

use crate::support::{facts_for, function};

const SOURCES: [&str; 8] = [
    "",
    "no newline",
    "one line\n",
    "\n\n",
    "ab\n  x.y\n\r\n  tail\r",
    "crlf one\r\ncrlf two\r\nlast",
    "head\u{e9}x\n\u{fc} \u{df}\n",
    "tab\tindented\n\t\tdeeper\n",
];

const MIXED: &str = "fn \u{e9}() {}\r\n    let x = 1;\n\nlast";

/// Every line and byte worth probing: in range, one past, far past, and `u32::MAX`.
fn positions(source: &str) -> Vec<Pos> {
    let lines = u32::try_from(source.split('\n').count()).expect("small source");
    let bytes = u32::try_from(source.len()).expect("small source");
    let mut out = Vec::new();
    for line in (0..=lines + 1).chain([u32::MAX]) {
        for byte in (0..=bytes + 1).chain([u32::MAX]) {
            out.push(Pos { line, byte });
        }
    }
    out
}

#[test]
fn line_starts_mark_where_each_split_part_begins() {
    assert_eq!(line_starts(""), [0], "the empty source is one empty line");
    assert_eq!(
        line_starts("a\nbc\n"),
        [0, 2, 5],
        "a trailing newline starts a last empty line"
    );
    assert_eq!(
        line_starts("a\r\nb"),
        [0, 3],
        "a carriage return stays on the line it ends"
    );
}

#[test]
fn the_index_agrees_with_source_site_at_every_position() {
    for source in SOURCES {
        let starts = line_starts(source);
        for pos in positions(source) {
            assert_eq!(
                source_site_indexed(&starts, source, pos),
                source_site(source, pos),
                "indexed and scanned sites differ for {source:?} at {pos:?}"
            );
        }
    }
}

#[test]
fn an_index_from_another_source_resolves_without_panicking() {
    let pos = Pos { line: 3, byte: 0 };
    assert_eq!(
        source_site_indexed(&[], "abc", pos),
        (1, "".into()),
        "an empty index has no lines"
    );
    let foreign = line_starts("a\nbb\nccc\n");
    assert_eq!(
        source_site_indexed(&foreign, "a", pos),
        (1, "".into()),
        "offsets past the end of this source give an empty line"
    );
}

#[test]
fn findings_agree_with_source_findings_at_every_position() {
    let facts = facts_for(Lang::Rust, "src/mixed.rs", MIXED);
    for pos in positions(MIXED) {
        let indexed = Finding::at(&facts, pos, "r/x", Severity::Warn, "m".into(), "f".into());
        let scanned = Finding::at_source(
            facts.path,
            facts.source,
            pos,
            "r/x",
            Severity::Warn,
            "m".into(),
            "f".into(),
        );
        assert_eq!(
            indexed, scanned,
            "Finding::at must equal Finding::at_source at {pos:?}"
        );
    }
}

#[test]
fn world_sites_take_their_line_from_the_position_line() {
    let source = "ab\ncd\n";
    let mut facts = facts_for(Lang::Rust, "src/sites.rs", source);
    facts.functions = vec![
        function("agree", Pos { line: 2, byte: 4 }),
        function("early", Pos { line: 2, byte: 1 }),
        function("late", Pos { line: 1, byte: 4 }),
    ];
    let world = WorldFacts::from_facts(&facts);
    let mut sites = Vec::new();
    for site in &world.definitions {
        sites.push((site.line, site.column));
    }
    assert_eq!(
        sites,
        [(2, 2), (2, 1), (1, 3)],
        "a byte before its line clamps to column 1 and a byte past it to the line end"
    );
}
