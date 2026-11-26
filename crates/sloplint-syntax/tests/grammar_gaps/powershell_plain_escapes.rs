//! Plain command-word Unicode escapes retain Str shape without counting spelling.

use sloplint_core::facts::ArgShape;

use super::powershell::{call, run_clean};

#[test]
fn unicode_escape_spelling_does_not_lengthen_plain_literal_runs() {
    for (name, source, expected) in [
        ("plain-bare-short", "Write-Host `u{000041}x\n", 1),
        ("plain-bare-long", "Write-Host `u{000041}abcdefgh\n", 8),
        (
            "plain-bare-prefix-near",
            "Write-Host abcdefg`u{000041}x\n",
            7,
        ),
        (
            "plain-bare-prefix-long",
            "Write-Host abcdefgh`u{000041}x\n",
            8,
        ),
        ("plain-bare-scalar", "Write-Host `u{1F600}x\n", 1),
        ("plain-bare-short-hex", "Write-Host `u{41}x\n", 1),
        ("plain-mixed-expandable", "Write-Host p\"`u{000041}x\"\n", 1),
        (
            "plain-verbatim-and-escape",
            "Write-Host p'`u{000041}'`u{42}x\n",
            11,
        ),
    ] {
        let analysis = run_clean(source);
        let command = call(&analysis, "Write-Host");
        assert_eq!(command.args.len(), 1, "{name}");
        let ArgShape::Str { longest_static } = command.args[0].shape else {
            panic!("{name}: expected Str, got {:?}", command.args[0].shape);
        };
        assert_eq!(longest_static, expected, "{name}: {source:?}");
    }
}

#[test]
fn unaffected_plain_word_measurements_keep_their_existing_contract() {
    for (name, source, expected) in [
        ("plain-mixed-verbatim", "Write-Host p'`u{000041}x'\n", 14),
        (
            "plain-uppercase-ordinary",
            "Write-Host p\"`U{000041}x\"\n",
            14,
        ),
        ("plain-plain", "Write-Host hello\n", 5),
        ("plain-ordinary-space", "Write-Host hello` world\n", 12),
        (
            "plain-ordinary-dollar",
            "Write-Host v`$VERSION.tar.gz\n",
            17,
        ),
    ] {
        let analysis = run_clean(source);
        let command = call(&analysis, "Write-Host");
        assert_eq!(command.args.len(), 1, "{name}");
        let ArgShape::Str { longest_static } = command.args[0].shape else {
            panic!("{name}: expected Str, got {:?}", command.args[0].shape);
        };
        assert_eq!(longest_static, expected, "{name}: {source:?}");
    }
}
