//! Literal boundaries from Lean's quotedCharCoreFn and stringGapFn.

use super::{function_names, parse_raw, run, run_clean};

#[test]
fn string_gaps_parse_clean() {
    let source = "def greeting : String := \"hello \\\n  world\"\ndef shout (n : Nat) : String := s!\"count \\\n    {n} done\"\ndef after : Nat := 0\n";
    let analysis = run_clean("src/gaps.lean", source);
    for want in ["greeting", "shout", "after"] {
        assert!(
            function_names(&analysis).contains(&want),
            "{want} must surface as a function, got {:?}",
            function_names(&analysis)
        );
    }
    let sexp = parse_raw(source).root_node().to_sexp();
    for shape in [
        "(str_lit (escape_sequence))",
        "(interpolated_str (escape_sequence) (interpolation ",
    ] {
        assert!(
            sexp.contains(shape),
            "a string gap must lex as an escape sequence, got {sexp}"
        );
    }
}

#[test]
fn string_gaps_accept_one_lf_or_crlf_and_keep_multiline_text() {
    for ending in ["\n", "\r\n"] {
        let ordinary = format!("def gap : String := \"first\\{ending} \tsecond\"{ending}");
        let interpolated =
            format!("def gap (n : Nat) : String := s!\"first\\{ending} \t{{n}}\"{ending}");
        run_clean("src/gap.lean", &ordinary);
        run_clean("src/interpolated-gap.lean", &interpolated);
    }
    for source in [
        "def text : String := \"first\n\nsecond\"\n",
        "def text (n : Nat) : String := s!\"first\n\n{n}\"\n",
        "def escaped : Char := '\\n'\n",
        "def escaped : String := \"first\\nsecond\"\n",
    ] {
        run_clean("src/literal.lean", source);
    }
}

#[test]
fn character_literals_reject_string_gaps() {
    for ending in ["\n", "\r\n"] {
        let source = format!("def bad : Char := '\\{ending}'{ending}");
        assert!(parse_raw(&source).root_node().has_error(), "{source}");
        assert!(run("src/character-gap.lean", &source).parse_error.is_some());
    }
}

#[test]
fn string_gap_spans_consume_the_line_ending_and_all_indentation() {
    for ending in ["\n", "\r\n"] {
        let gap = format!("\\{ending}                    \t");
        let source = format!("def send (n : Nat) := sink s!\"first{gap}{{n}}tail\"{ending}");
        let tree = parse_raw(&source);
        let mut nodes = vec![tree.root_node()];
        let mut spans = Vec::new();
        while let Some(node) = nodes.pop() {
            if node.kind() == "escape_sequence" {
                spans.push(node.byte_range());
            }
            let mut cursor = node.walk();
            nodes.extend(node.named_children(&mut cursor));
        }
        let start = source.find(&gap).expect("fixture contains the string gap");
        assert_eq!(spans, [start..start + gap.len()]);
        let analysis = run_clean("src/gap-span.lean", &source);
        let call = analysis
            .facts
            .calls
            .iter()
            .find(|call| call.callee == "sink")
            .expect("sink receives the interpolated string");
        assert!(
            matches!(
                call.args[0].shape,
                sloplint_core::facts::ArgShape::Concat { longest_static: 5 }
            ),
            "indentation and interpolation must not count as literal text: {:?}",
            call.args
        );
    }
}

#[test]
fn string_gaps_reject_a_second_newline_before_content() {
    for ending in ["\n", "\r\n"] {
        for prefix in ["", "s!", "m!"] {
            for indentation in ["", "  \t"] {
                let source = format!(
                    "def bad : String := {prefix}\"first\\{ending}{indentation}{ending}second\"{ending}"
                );
                assert!(parse_raw(&source).root_node().has_error(), "{source}");
                assert!(run("src/blank-gap.lean", &source).parse_error.is_some());
            }
        }
    }
}
