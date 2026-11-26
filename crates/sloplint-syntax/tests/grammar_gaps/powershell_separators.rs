//! Native PowerShell 7.7.0 argument boundaries and protected whitespace.

use super::powershell::{call, run_clean};

const HORIZONTAL_SPACES: &[char] = &[
    '\u{9}', '\u{b}', '\u{c}', '\u{20}', '\u{85}', '\u{a0}', '\u{1680}', '\u{2000}', '\u{2001}',
    '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}',
    '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}',
];

#[test]
fn unicode_horizontal_spaces_preserve_native_argument_counts() {
    for &space in HORIZONTAL_SPACES {
        for (kind, source, count) in [
            (
                "after-command",
                format!("Capture-Native{space}hello world\n"),
                2,
            ),
            (
                "between-arguments",
                format!("Capture-Native hello{space}world\n"),
                2,
            ),
            (
                "before-double-dash",
                format!("Capture-Native hello{space}-- world\n"),
                3,
            ),
            (
                "after-double-dash",
                format!("Capture-Native hello --{space}world\n"),
                3,
            ),
            (
                "before-single-dash",
                format!("Capture-Native hello{space}- world\n"),
                3,
            ),
            (
                "after-single-dash",
                format!("Capture-Native hello -{space}world\n"),
                3,
            ),
        ] {
            let analysis = run_clean(&source);
            let actual = call(&analysis, "Capture-Native").args.len();
            assert_eq!(
                actual,
                count,
                "{kind}, U+{:04X}: {source:?}",
                u32::from(space)
            );
            println!(
                "NATIVE_ARGUMENT_CASE space-{:04x}-{kind} {actual}",
                u32::from(space)
            );
        }
    }
}

#[test]
fn protected_spaces_and_format_marks_stay_inside_native_arguments() {
    for &space in HORIZONTAL_SPACES {
        for (kind, source) in [
            ("quoted", format!("Capture-Native \"hello{space}world\"\n")),
            ("escaped", format!("Capture-Native hello`{space}world\n")),
        ] {
            let analysis = run_clean(&source);
            let actual = call(&analysis, "Capture-Native").args.len();
            assert_eq!(actual, 1, "{kind}, U+{:04X}: {source:?}", u32::from(space));
            println!(
                "NATIVE_ARGUMENT_CASE space-{:04x}-{kind} {actual}",
                u32::from(space)
            );
        }
    }
    for mark in ['\u{feff}', '\u{200b}', '\u{2060}'] {
        for (kind, source) in [
            ("word", format!("Capture-Native hello{mark}world\n")),
            ("dash", format!("Capture-Native --{mark}tail\n")),
        ] {
            let analysis = run_clean(&source);
            let actual = call(&analysis, "Capture-Native").args.len();
            assert_eq!(actual, 1, "{kind}, U+{:04X}: {source:?}", u32::from(mark));
            println!(
                "NATIVE_ARGUMENT_CASE format-{:04x}-{kind} {actual}",
                u32::from(mark)
            );
        }
    }
}
