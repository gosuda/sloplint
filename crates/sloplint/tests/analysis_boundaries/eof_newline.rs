//! A source file that does not end in a newline must not mis-parse. Line-oriented
//! grammar rules (preprocessor directives and similar) require a trailing `\n`, so
//! the reader appends one at EOF; compilers accept the same files.

use super::{base, put, run_ok, tmp};

#[test]
fn boundary_missing_final_newline_still_parses() {
    let (_t, root) = tmp();
    std::fs::create_dir_all(root.join("src")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    for (name, body) in [
        // The grammar requires `\n` after each preproc directive; without it the
        // token is MISSING at EOF (corpus: newtonsoft `#pragma warning restore`).
        ("a.cs", "class A {}\n#pragma warning restore 618"),
        ("b.cs", "class A {}\n#endregion"),
        ("c.cs", "class A {}\n#define X 1"),
        // C has the same EOF shape for `#define`.
        ("d.c", "int x;\n#define FOO 1"),
        // Odin's grammar is a crates.io dependency and cannot be patched, so the
        // reader normalization is what covers it.
        ("e.odin", "package main\nmain :: proc() {}"),
    ] {
        put(&root, &format!("src/{name}"), body);
    }
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["meta/*"]),
        ..base(&root)
    });
    let parse_errors: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "meta/parse-error")
        .collect();
    assert!(
        parse_errors.is_empty(),
        "files without a final newline must not report parse errors: {parse_errors:?}"
    );
}
