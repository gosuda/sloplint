//! A `.ts` file whose content is XML (Qt translations) is not TypeScript: it
//! must be skipped rather than reported as a TypeScript parse error.

use super::{base, put, run_ok, tmp};

#[test]
fn boundary_xml_declared_ts_file_is_skipped() {
    let (_t, root) = tmp();
    put(
        &root,
        "i18n/app_en.ts",
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"en_US\">\n<context>\n    <name>App</name>\n    <message><source>Hello</source></message>\n</context>\n</TS>\n",
    );
    put(&root, "src/real.ts", "export const x: number = 1;\n");
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["meta/*"]),
        ..base(&root)
    });
    assert!(
        r.findings.iter().all(|f| &*f.rule != "meta/parse-error"),
        "XML must not surface a TypeScript parse error: {:?}",
        r.findings.iter().map(|f| &*f.rule).collect::<Vec<_>>()
    );
    assert_eq!(r.summary.skipped, 1, "the XML file is skipped");
    assert_eq!(r.summary.files, 1, "the real .ts file is analyzed");
}
