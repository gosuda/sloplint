//! One unparseable construct must not sink a whole file: ERROR regions are
//! blanked and the source reparsed so rules still see the rest, while the
//! original error position is still reported as meta/parse-error.

use super::{base, put, run_ok, tmp};

#[test]
fn boundary_error_region_excised_rules_still_run() {
    let (_t, root) = tmp();
    // Corpus shape (dune C stubs): `CAMLprim` is an unknown qualifier macro in
    // declaration position, so the grammar cannot place the whole function.
    // `kept` parses fine and nests deep enough for the nesting rule.
    put(
        &root,
        "src/stubs.c",
        "CAMLprim value stub(value v)\n{\n\t(void)v;\n}\n\
         int kept(void)\n{\n\
         \tif (1) {\n\t\tif (1) {\n\t\t\tif (1) {\n\t\t\t\tif (1) {\n\t\t\t\t\tif (1) {\n\t\t\t\t\t\treturn 1;\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n\treturn 0;\n}\n",
    );
    put(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/nesting-depth\"]\nlevel = \"minor\"\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["complexity/nesting-depth", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/stubs.c")
        .unwrap_or_else(|| panic!("rescued parse must still report the excised region"));
    assert_eq!(pe.line, 1, "parse-error marks the unparseable construct");
    let depth = r
        .findings
        .iter()
        .find(|f| &*f.rule == "complexity/nesting-depth" && &*f.path == "src/stubs.c")
        .unwrap_or_else(|| {
            panic!(
                "rules must run on the surviving code: {:?}",
                r.findings.iter().map(|f| &*f.rule).collect::<Vec<_>>()
            )
        });
    assert_eq!(depth.line, 5, "the surviving function was analyzed");
}

#[test]
fn boundary_fully_unparseable_file_still_reports_only_parse_error() {
    let (_t, root) = tmp();
    // Bytes no rule can act on either way: the rescue must not fabricate
    // findings, it must just not get in the way.
    put(&root, "src/blob.c", "@@@###$$$%%%\n^^^^&&&***\n");
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/blob.c")
        .unwrap_or_else(|| panic!("unparseable file must report parse-error"));
    assert_eq!(pe.line, 1);
}
