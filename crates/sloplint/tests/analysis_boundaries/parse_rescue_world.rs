use super::{Mode, RunOptions, Strictness, base, bin_cold, bin_warm, filt, put, run_ok, tmp};

#[test]
fn rescued_barrels_do_not_create_world_chains() {
    let (_project, root) = tmp();
    put(&root, "src/partial.ts", "@@@\nexport * from './inner';\n");
    put(&root, "src/inner.ts", "export * from './leaf';\n");
    put(&root, "src/outer.ts", "export * from './partial';\n");
    put(&root, "src/control.ts", "export * from './inner';\n");
    put(&root, "src/leaf.ts", "export const answer = 42;\n");
    let report = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["slop/barrel-file", "meta/*"]),
        ..base(&root)
    });
    assert!(report.findings.iter().any(|finding| {
        &*finding.rule == "meta/parse-error" && &*finding.path == "src/partial.ts"
    }));
    let barrels: Vec<&str> = report
        .findings
        .iter()
        .filter(|finding| &*finding.rule == "slop/barrel-file")
        .map(|finding| &*finding.path)
        .collect();
    assert_eq!(barrels, ["src/control.ts"]);

    let (_cache, home) = tmp();
    let cold = bin_cold(&root, &home, "json", 1);
    let stored = bin_warm(&root, &home, "json", 1);
    let warm = bin_warm(&root, &home, "json", 2);
    assert_eq!(cold, stored, "storing partial world facts changes nothing");
    assert_eq!(cold, warm, "cached partial world facts stay conservative");
}

#[test]
fn rescued_files_preserve_healthy_world_definitions() {
    let (_project, root) = tmp();
    put(
        &root,
        "src/a.js",
        "@@@\nexport function collide() { return 1; }\n",
    );
    put(
        &root,
        "src/b.js",
        "export function collide() { return 2; }\n",
    );
    put(
        &root,
        "sloplint.toml",
        "[rules.\"grep/homonym-definitions\"]\nlevel = \"minor\"\nmin_files = 2\n",
    );
    let report = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/homonym-definitions", "meta/*"]),
        ..base(&root)
    });
    let definitions: Vec<(&str, u32)> = report
        .findings
        .iter()
        .filter(|finding| &*finding.rule == "grep/homonym-definitions")
        .map(|finding| (&*finding.path, finding.line))
        .collect();
    assert_eq!(definitions, [("src/a.js", 2)]);
    assert!(
        report.findings.iter().any(|finding| {
            &*finding.rule == "meta/parse-error" && &*finding.path == "src/a.js"
        })
    );
}

#[test]
fn rescued_files_drop_tainted_world_definitions() {
    let (_project, root) = tmp();
    put(&root, "src/a.js", "export function collide() { !@#$% }\n");
    put(
        &root,
        "src/b.js",
        "export function collide() { return 2; }\n",
    );
    put(
        &root,
        "sloplint.toml",
        "[rules.\"grep/homonym-definitions\"]\nlevel = \"minor\"\nmin_files = 2\n",
    );
    let report = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/homonym-definitions", "meta/*"]),
        ..base(&root)
    });
    assert!(
        report.findings.iter().any(|finding| {
            &*finding.rule == "meta/parse-error" && &*finding.path == "src/a.js"
        })
    );
    assert!(
        report
            .findings
            .iter()
            .all(|finding| &*finding.rule != "grep/homonym-definitions"),
        "a declaration truncated by rescue must not create a homonym: {:?}",
        report.findings
    );
}

#[test]
fn rescued_files_do_not_report_public_function_totals() {
    let (_project, root) = tmp();
    put(
        &root,
        "src/partial.js",
        "@@@\nexport function answer() { return 42; }\n",
    );
    put(
        &root,
        "src/control.js",
        "export function answer() { return 42; }\n",
    );
    put(&root, "src/other.test.js", "const expected = 42;\n");
    let report = run_ok(&RunOptions {
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["agent/colocated-tests", "meta/*"]),
        ..base(&root)
    });
    assert!(report.findings.iter().any(|finding| {
        &*finding.rule == "meta/parse-error" && &*finding.path == "src/partial.js"
    }));
    let untested: Vec<&str> = report
        .findings
        .iter()
        .filter(|finding| &*finding.rule == "agent/colocated-tests")
        .map(|finding| &*finding.path)
        .collect();
    assert_eq!(untested, ["src/control.js"]);
}

#[test]
fn incomplete_packages_do_not_prove_a_single_implementation() {
    let (_project, root) = tmp();
    for path in [
        "crates/broken/src/contracts.rs",
        "crates/control/src/contracts.rs",
    ] {
        put(
            &root,
            path,
            "trait Channel { fn read(&self); }\n\
             struct First;\n\
             impl Channel for First { fn read(&self) {} }\n",
        );
    }
    put(
        &root,
        "crates/broken/src/damaged.rs",
        "struct Second;\nimpl Channel for Second { fn read(&self) { @@@ } }\n",
    );
    let report = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["slop/single-impl-abstraction", "meta/*"]),
        ..base(&root)
    });
    let abstractions: Vec<(&str, u32)> = report
        .findings
        .iter()
        .filter(|finding| &*finding.rule == "slop/single-impl-abstraction")
        .map(|finding| (&*finding.path, finding.line))
        .collect();
    assert_eq!(abstractions, [("crates/control/src/contracts.rs", 1)]);
    assert!(report.findings.iter().any(|finding| {
        &*finding.rule == "meta/parse-error" && &*finding.path == "crates/broken/src/damaged.rs"
    }));
}
