//! One unparseable construct must not sink a whole file: ERROR regions are
//! blanked and the source reparsed so rules still see the rest, while the
//! original error position is still reported as meta/parse-error.

use super::{base, put, run_ok, tmp};

#[test]
fn boundary_error_region_excised_rules_still_run() {
    let (_t, root) = tmp();
    // `template <typename T>` is not C: the grammar cannot place the whole
    // function, so it is excised. (`CAMLprim`-style unknown qualifier macros
    // no longer work here — the grammar now accepts them.) `kept` parses fine
    // and nests deep enough for the nesting rule.
    put(
        &root,
        "src/stubs.c",
        "template <typename T> int stub(T v)\n{\n\treturn (int)v;\n}\n\
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
fn boundary_excised_error_inside_construct_reports_no_tainted_findings() {
    let (_t, root) = tmp();
    // Review-found false positive: the ERROR inside the catch body is
    // excised, leaving an artificially empty `catch (e) {}` — without the
    // taint filter, slop/zombie-catch reports the artificial deletion as a
    // swallowed error.
    put(
        &root,
        "src/handler.js",
        "function ok() { return 1 }\ntry { ok() } catch (e) { !@#$% }\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["slop/zombie-catch", "meta/*"]),
        ..base(&root)
    });
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/handler.js"),
        "the excised region must still be reported: {:?}",
        r.findings.iter().map(|f| &*f.rule).collect::<Vec<_>>()
    );
    assert!(
        !r.findings.iter().any(|f| &*f.rule == "slop/zombie-catch"),
        "findings inside excised constructs judge truncated code: {:?}",
        r.findings
    );
}

#[test]
fn boundary_suppression_inside_excised_region_still_applies() {
    let (_t, root) = tmp();
    // Review-found comment loss: the file-level directive sits inside the
    // unparseable declaration, so it vanishes from the rescued tree; the
    // original parse's comments are merged back so it still suppresses the
    // rule that fires on the surviving function. (meta/parse-error itself is
    // not a suppressible operand; nesting-depth is.)
    let source = "Mystery /* sloplint-allow-file(complexity/nesting-depth): vendor macro */ int x;\n\
         int kept(void)\n{\n\
         \tif (1) {\n\t\tif (1) {\n\t\t\tif (1) {\n\t\t\t\tif (1) {\n\t\t\t\t\tif (1) {\n\t\t\t\t\t\treturn 1;\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n\treturn 0;\n}\n";
    put(&root, "src/vend.c", source);
    put(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/nesting-depth\"]\nlevel = \"minor\"\n",
    );
    let options = super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["complexity/nesting-depth", "meta/*"]),
        ..base(&root)
    };
    let r = run_ok(&options);
    assert!(r.findings.iter().any(|f| &*f.rule == "meta/parse-error"));
    assert!(
        !r.findings
            .iter()
            .any(|f| &*f.rule == "complexity/nesting-depth"),
        "the directive inside the excised region must still suppress: {:?}",
        r.findings
    );
    let unsuppressed = source.replace(
        "sloplint-allow-file(complexity/nesting-depth): vendor macro",
        "vendor macro",
    );
    put(&root, "src/vend.c", &unsuppressed);
    let control = run_ok(&options);
    assert!(
        control
            .findings
            .iter()
            .any(|f| &*f.rule == "complexity/nesting-depth"),
        "the same surviving function must be diagnosed without its directive"
    );
}

#[test]
fn boundary_rescue_drops_file_wide_findings() {
    let (_t, root) = tmp();
    // Review-found miscount: 40 unparseable lines plus one valid decl. The
    // file is long by the original line count, so file-token-budget would
    // fire on data the rules never saw — it must be dropped on a partial run.
    let mut body = String::from("int ok(void) { return 0; }\n");
    for _ in 0..40 {
        body.push_str("@@@###$$$%%%\n");
    }
    put(&root, "src/blob.c", &body);
    put(
        &root,
        "sloplint.toml",
        "[rules.\"agent/file-token-budget\"]\nlevel = \"minor\"\n\
         minor_at = 5\nmajor_at = 10\ncritical_at = 20\nblocker_at = 30\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["agent/file-token-budget", "meta/*"]),
        ..base(&root)
    });
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/blob.c"),
        "the rescue must still report the excised region"
    );
    assert!(
        !r.findings
            .iter()
            .any(|f| &*f.rule == "agent/file-token-budget"),
        "file-wide aggregates over excised bytes miscount: {:?}",
        r.findings
    );
}

#[test]
fn boundary_rescued_file_does_not_count_as_barrel() {
    let (_t, root) = tmp();
    // Review-found world-rule miscount: `a.ts` holds one real re-export and
    // one unparseable construct. Excision leaves a file that reads as a
    // pure barrel re-exporting the barrel `b.ts` — but the excised bytes
    // may have held real declarations, so the completeness judgment is
    // unprovable and the chain finding must stay silent.
    put(&root, "src/a.ts", "export * from './b';\n@@@###$$$\n");
    put(&root, "src/b.ts", "export * from './c';\n");
    put(&root, "src/c.ts", "export const value = 1;\n");
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["slop/barrel-file", "meta/*"]),
        ..base(&root)
    });
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/a.ts"),
        "the rescue must still report the excised region: {:?}",
        r.findings
    );
    assert!(
        !r.findings
            .iter()
            .any(|f| &*f.rule == "slop/barrel-file" && &*f.path == "src/a.ts"),
        "a partially parsed file cannot be proven a pure barrel: {:?}",
        r.findings
    );
}

#[test]
fn boundary_deeply_nested_error_does_not_overflow() {
    let (_t, root) = tmp();
    // Review-found recursion: an ERROR deep inside nested blocks must be
    // collected iteratively, not one stack frame per level.
    let mut body = String::from("int f(void) {\n");
    for _ in 0..4000 {
        body.push_str("if (1) {\n");
    }
    body.push_str("@@@###\n");
    for _ in 0..4000 {
        body.push_str("}\n");
    }
    body.push_str("}\nint handle(void) { return 0; }\n");
    put(&root, "src/deep.c", &body);
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Standard),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/deep.c"),
        "a deeply nested error still surfaces as parse-error"
    );
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "grep/generic-name" && f.line == 8004),
        "the function after the deeply nested error must still be analyzed"
    );
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
