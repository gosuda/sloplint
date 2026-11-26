//! Format marks are literal native argument text after one or two dashes.

use super::powershell::{call, run_clean};
use sloplint_core::facts::ArgShape;

fn assert_word(source: &str, word: &str, dynamic: bool) {
    let analysis = run_clean(source);
    let call = call(&analysis, "Capture-Native");
    assert_eq!(call.args.len(), 1, "{source:?}");
    if dynamic {
        assert!(
            matches!(call.args[0].shape, ArgShape::Concat { .. }),
            "{source:?}"
        );
    } else {
        assert!(
            matches!(call.args[0].shape, ArgShape::Str { .. }),
            "{source:?}"
        );
    }
    let (words, _) = argument_spans(source);
    assert_eq!(words, [word], "{source:?}");
}

fn argument_spans(source: &str) -> (Vec<&str>, Vec<&str>) {
    let language: tree_sitter::Language = tree_sitter_powershell::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("powershell language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let mut stack = vec![tree.root_node()];
    let mut words = Vec::new();
    let mut arrays = Vec::new();
    while let Some(node) = stack.pop() {
        if node.kind() == "generic_token" {
            words.push(node.utf8_text(source.as_bytes()).expect("source is UTF-8"));
        } else if node.kind() == "array_literal_expression" {
            arrays.push(node.utf8_text(source.as_bytes()).expect("source is UTF-8"));
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    (words, arrays)
}

#[test]
fn format_mark_dash_words_keep_native_counts_and_complete_extents() {
    for mark in ['\u{200b}', '\u{feff}', '\u{2060}'] {
        for dashes in ["-", "--"] {
            for (kind, suffix, dynamic) in [
                ("empty", "", false),
                ("tail", "tail", false),
                ("double-quoted", "\"two words\"", false),
                ("single-quoted", "'two words'", false),
                ("escaped-space", "` tail", false),
                ("escaped-dollar", "`$tail", false),
                ("variable", "$tail", true),
            ] {
                let word = format!("{dashes}{mark}{suffix}");
                let prelude = if dynamic { "$tail = 'value'\n" } else { "" };
                let source = format!("{prelude}Capture-Native {word}\n");
                assert_word(&source, &word, dynamic);
                println!(
                    "NATIVE_DASH_CASE dash-{:04x}-{}-{kind} 1",
                    u32::from(mark),
                    dashes.len()
                );
            }
        }
    }
}

#[test]
fn quoted_format_mark_dash_words_remain_single_arguments() {
    for mark in ['\u{200b}', '\u{feff}', '\u{2060}'] {
        for dashes in ["-", "--"] {
            for quote in ['\'', '"'] {
                let source = format!("Capture-Native {quote}{dashes}{mark}{quote}\n");
                let analysis = run_clean(&source);
                let call = call(&analysis, "Capture-Native");
                assert_eq!(call.args.len(), 1, "{source:?}");
                assert!(
                    matches!(call.args[0].shape, ArgShape::Str { .. }),
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn format_mark_commas_keep_native_arrays_and_protected_words() {
    for mark in ['\u{200b}', '\u{feff}', '\u{2060}'] {
        for dashes in ["-", "--"] {
            for (kind, suffix) in [("bare", "a,b"), ("quoted", "\"a,b\""), ("escaped", "a`,b")] {
                let word = format!("{dashes}{mark}{suffix}");
                let source = format!("Capture-Native {word}\n");
                if kind == "bare" {
                    let analysis = run_clean(&source);
                    let call = call(&analysis, "Capture-Native");
                    assert_eq!(call.args.len(), 1, "{source:?}");
                    assert!(matches!(call.args[0].shape, ArgShape::Other), "{source:?}");
                    let (words, arrays) = argument_spans(&source);
                    assert_eq!(arrays, [word.as_str()], "{source:?}");
                    let first = format!("{dashes}{mark}a");
                    assert!(words.contains(&first.as_str()), "{source:?}");
                    assert!(words.iter().all(|span| !span.contains(',')), "{source:?}");
                } else {
                    assert_word(&source, &word, false);
                }
                println!(
                    "NATIVE_COMMA_CASE comma-{:04x}-{}-{kind} 1",
                    u32::from(mark),
                    dashes.len()
                );
            }
        }
    }
}
