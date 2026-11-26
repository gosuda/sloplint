//! Interpolated command words must measure the literal value, excluding
//! hidden variable prefixes and quote syntax. Named AST holes still delimit
//! subexpressions; backticks are literal inside single-quoted segments.

use sloplint_core::facts::ArgShape;

use super::powershell::{call, run_clean};

#[test]
fn anonymous_prefix() {
    let analysis = run_clean("Write-Host $r\\one\\\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 5 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn long_anonymous_prefix() {
    let analysis = run_clean("Write-Host $long_root_name\\one\\\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 5 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn braced_prefix() {
    let analysis = run_clean("Write-Host ${r/part}\\one\\\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 5 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn scoped_prefix() {
    let analysis = run_clean("Write-Host $env:TEMP\\one\\\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 5 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_backtick() {
    let analysis = run_clean("Write-Host $r/'abc`defgh'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 10 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn adjacent_quote_modes() {
    let analysis = run_clean("Write-Host $r/\"abc\"'defgh'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 9 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_then_expandable() {
    let analysis = run_clean("Write-Host $r/'abc'\"defgh$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 9 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_doubled_quote() {
    let analysis = run_clean("Write-Host $r/'abc''def'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 8 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn expandable_doubled_quote() {
    let analysis = run_clean("Write-Host $r/\"abc\"\"def$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 8 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn dynamic_boundary() {
    let analysis = run_clean("Write-Host $r/abc\"$id\"defgh\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 5 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_dollar() {
    let analysis = run_clean("Write-Host $r/'abc$id'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 7 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_backtick_dollar() {
    let analysis = run_clean("Write-Host $r/'abc`$id'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 8 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn bare_escape() {
    let analysis = run_clean("Write-Host $r/ab`tcdefgh\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 6 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn expandable_escape() {
    let analysis = run_clean("Write-Host $r/\"ab`tcdefgh$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 6 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_escape_text() {
    let analysis = run_clean("Write-Host $r/'ab`tcdefgh'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 11 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn only_anonymous_variable() {
    let analysis = run_clean("Write-Host $r/abcdefgh\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 9 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn short_anonymous_path() {
    let analysis = run_clean("Write-Host $r/x\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 2 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn bare_inner_variable() {
    let analysis = run_clean("Write-Host $r/foo$middle/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 4 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn unicode_literal() {
    let analysis = run_clean("Write-Host $r/'café😀'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 6 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn unicode_variable() {
    let analysis = run_clean("Write-Host $r/foo$π/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 4 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn subexpression_boundary() {
    let analysis = run_clean("Write-Host $r/ab\"$(Get-Date)\"cdefgh\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 6 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn roman_numeral_is_literal() {
    let analysis = run_clean("Write-Host $r/foo$Ⅳ/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 10 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn numeric_fraction_is_literal() {
    let analysis = run_clean("Write-Host $r/foo$½/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 10 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn astral_letter_is_literal() {
    let analysis = run_clean("Write-Host $r/foo$𝒜/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 10 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn unicode_decimal_variable() {
    let analysis = run_clean("Write-Host $r/foo$٢/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 4 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn variable_ends_before_astral_letter() {
    let analysis = run_clean("Write-Host $r/a$foo𝒜/bar\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 5 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn unicode_escape_counts_as_one_escaped_character() {
    let analysis = run_clean("Write-Host $r/\"`u{1F600}x$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 1 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn uppercase_unicode_escape_counts_as_one_escaped_character() {
    let analysis = run_clean("Write-Host $r/\"`U{1F600}x$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 1 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}

#[test]
fn verbatim_unicode_escape_stays_literal() {
    let analysis = run_clean("Write-Host $r/'`u{1F600}x'\"$id\"\n");
    let command = call(&analysis, "Write-Host");
    assert_eq!(command.args.len(), 1);
    assert!(
        matches!(
            command.args[0].shape,
            ArgShape::Concat { longest_static: 11 }
        ),
        "shape: {:?}",
        command.args[0].shape
    );
}
