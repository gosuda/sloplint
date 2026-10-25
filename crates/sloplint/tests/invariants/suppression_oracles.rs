//! Suppression directives: each form removes exactly the findings it names, and malformed or
//! unused directives are reported at their own line. Every oracle runs over [`SAMPLES`], one row
//! per language and comment placement, so each grammar's comment syntax is exercised.

use sloplint::Report;

use super::{fired, run_ok, standard, tempdir, write};

/// One row of the oracle table: a sample file, the comment syntax its directives use, and the
/// findings its bare source produces.
struct Sample {
    /// Names the row in failure messages.
    label: &'static str,
    /// Config key of the language, for the custom-rule oracle.
    language: &'static str,
    /// Path the sample is written to; its extension selects the grammar.
    file: &'static str,
    /// Wraps directive text in the language's plain-comment syntax, line break included.
    comment: fn(&str) -> String,
    /// The sample with `directive` (already wrapped) placed directly above its one function.
    source: fn(&str) -> String,
    /// Findings of the bare sample as `(rule, line)`.
    control: &'static [(&'static str, u32)],
    /// Findings left after an exact `grep/generic-name` directive covers the function.
    after_exact: &'static [(&'static str, u32)],
    /// Wildcard family whose findings the sample carries.
    family: &'static str,
    /// Findings left after `family` covers the function.
    after_family: &'static [(&'static str, u32)],
    /// `[[custom_rules]]` query whose match starts on the function's name line. Declaration nodes
    /// in Java and Kotlin start at their annotations, so a whole-declaration match would anchor the
    /// custom finding one line above the directive that covers the name.
    query: &'static str,
}

/// Language rows. Each source holds one function with a generic name and, where the grammar has
/// typed parameters, two boolean parameters. Bash has no typed parameters, so it carries only the
/// name finding.
static SAMPLES: &[Sample] = &[
    Sample {
        label: "rust",
        language: "rust",
        file: "src/lib.rs",
        comment: |d| format!("// {d}\n"),
        source: |d| format!("{d}pub fn handle(flag: bool, other: bool) {{}}\n"),
        control: &[("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        after_exact: &[("complexity/boolean-params", 2)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 2)],
        query: "(function_item) @match",
    },
    Sample {
        label: "python",
        language: "python",
        file: "src/lib.py",
        comment: |d| format!("# {d}\n"),
        source: |d| format!("{d}def handle(flag: bool, other: bool) -> None:\n    pass\n"),
        control: &[("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        after_exact: &[("complexity/boolean-params", 2)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 2)],
        query: "(function_definition) @match",
    },
    Sample {
        label: "python decorator",
        language: "python",
        file: "src/decorated.py",
        comment: |d| format!("# {d}\n"),
        source: |d| {
            format!("@decorator\n{d}def handle(flag: bool, other: bool) -> None:\n    pass\n")
        },
        control: &[("complexity/boolean-params", 2), ("grep/generic-name", 2)],
        after_exact: &[("complexity/boolean-params", 3)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 3)],
        query: "(function_definition) @match",
    },
    Sample {
        label: "c block comment",
        language: "c",
        file: "src/lib.c",
        comment: |d| format!("/* {d} */\n"),
        source: |d| format!("{d}void handle(bool flag, bool other) {{}}\n"),
        control: &[("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        after_exact: &[("complexity/boolean-params", 2)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 2)],
        query: "(function_definition) @match",
    },
    Sample {
        label: "ruby",
        language: "ruby",
        file: "src/lib.rb",
        comment: |d| format!("# {d}\n"),
        source: |d| format!("{d}def handle(flag = true, other = false)\nend\n"),
        control: &[("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        after_exact: &[("complexity/boolean-params", 2)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 2)],
        query: "(method) @match",
    },
    Sample {
        label: "bash",
        language: "bash",
        file: "src/lib.sh",
        comment: |d| format!("# {d}\n"),
        source: |d| format!("{d}handle() {{\n    :\n}}\n"),
        control: &[("grep/generic-name", 1)],
        after_exact: &[],
        family: "grep/*",
        after_family: &[],
        query: "(function_definition) @match",
    },
    Sample {
        label: "ocaml block comment",
        language: "ocaml",
        file: "src/lib.ml",
        comment: |d| format!("(* {d} *)\n"),
        source: |d| format!("{d}let handle (flag : bool) (other : bool) = ()\n"),
        control: &[("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        after_exact: &[("complexity/boolean-params", 2)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 2)],
        query: "(let_binding) @match",
    },
    Sample {
        label: "java annotation",
        language: "java",
        file: "src/Sample.java",
        comment: |d| format!("// {d}\n"),
        source: |d| {
            format!(
                "class Sample {{\n    @Deprecated\n{d}    public void handle(boolean flag, boolean other) {{}}\n}}\n"
            )
        },
        control: &[("complexity/boolean-params", 3), ("grep/generic-name", 3)],
        after_exact: &[("complexity/boolean-params", 4)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 4)],
        query: "(method_declaration name: (identifier) @match)",
    },
    Sample {
        label: "kotlin annotation",
        language: "kotlin",
        file: "src/Sample.kt",
        comment: |d| format!("// {d}\n"),
        source: |d| {
            format!(
                "annotation class Ann\n@Ann\n{d}fun handle(flag: Boolean, other: Boolean) {{}}\n"
            )
        },
        control: &[("complexity/boolean-params", 3), ("grep/generic-name", 3)],
        after_exact: &[("complexity/boolean-params", 4)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 4)],
        query: "(function_declaration name: (identifier) @match)",
    },
    Sample {
        label: "go",
        language: "go",
        file: "src/lib.go",
        comment: |d| format!("// {d}\n"),
        source: |d| format!("package sample\n\n{d}func Handle(flag bool, other bool) {{}}\n"),
        control: &[("complexity/boolean-params", 3), ("grep/generic-name", 3)],
        after_exact: &[("complexity/boolean-params", 4)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 4)],
        query: "(function_declaration) @match",
    },
    Sample {
        label: "powershell block comment",
        language: "powershell",
        file: "src/lib.ps1",
        comment: |d| format!("<# {d} #>\n"),
        source: |d| format!("{d}function Handle([bool]$Flag, [bool]$Other) {{}}\n"),
        control: &[("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        after_exact: &[("complexity/boolean-params", 2)],
        family: "complexity/*",
        after_family: &[("grep/generic-name", 2)],
        query: "(function_statement) @match",
    },
];

/// `(rule, line)` pairs of the findings, in report order.
fn rule_lines(report: &Report) -> Vec<(&str, u32)> {
    report.findings.iter().map(|f| (&*f.rule, f.line)).collect()
}

/// `(rule, line)` pairs of the `meta/` findings only.
fn meta_lines(report: &Report) -> Vec<(&str, u32)> {
    rule_lines(report)
        .into_iter()
        .filter(|(rule, _)| rule.starts_with("meta/"))
        .collect()
}

/// Directive text as every row writes it. The space before `)` keeps a wildcard from forming
/// `*)`, which closes an OCaml comment early.
fn directive(operand: &str, reason: &str) -> String {
    format!("sloplint-allow({operand} ): {reason}")
}

/// The standard-profile report of a project with `config` as sloplint.toml and `source` at `file`.
fn report_of(file: &str, source: &str, config: &str) -> Report {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", config);
    write(&root, file, source);
    run_ok(&standard(&root))
}

/// The report of `sample` with `body` spliced above its function.
fn sample_report(sample: &Sample, body: &str, config: &str) -> Report {
    report_of(sample.file, &(sample.source)(body), config)
}

/// The 1-based line `body` occupies in the sample's source.
fn directive_line(sample: &Sample, body: &str) -> u32 {
    let source = (sample.source)(body);
    let at = source
        .find(body)
        .unwrap_or_else(|| panic!("{}: source lacks its directive", sample.label));
    let line = source[..at].matches('\n').count() + 1;
    u32::try_from(line).unwrap_or_else(|e| panic!("line {line}: {e}"))
}

/// Records a failure when `report`'s findings differ from `expected`.
fn expect_lines(
    failures: &mut Vec<String>,
    sample: &Sample,
    what: &str,
    report: &Report,
    expected: &[(&str, u32)],
) {
    let actual = rule_lines(report);
    if actual != expected {
        failures.push(format!(
            "{} ({what}): expected {expected:?}, got {actual:?}",
            sample.label
        ));
    }
}

/// Fails once, listing every row that broke, so one run names all affected languages.
fn assert_no_failures(failures: &[String]) {
    assert!(
        failures.is_empty(),
        "suppression oracles failed:\n{}",
        failures.join("\n")
    );
}

#[test]
fn suppressions_drop_only_their_findings() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let control = sample_report(sample, "", "");
        expect_lines(&mut failures, sample, "control", &control, sample.control);
        let exact = (sample.comment)(&directive("grep/generic-name", "the name is the contract"));
        let exact_report = sample_report(sample, &exact, "");
        expect_lines(
            &mut failures,
            sample,
            "exact directive",
            &exact_report,
            sample.after_exact,
        );
        let family = (sample.comment)(&directive(sample.family, "the family is the contract"));
        let family_report = sample_report(sample, &family, "");
        expect_lines(
            &mut failures,
            sample,
            "family directive",
            &family_report,
            sample.after_family,
        );
    }
    assert_no_failures(&failures);
}

/// A doc comment that restates the function name.
const DOC: &str = "/// Process the order.\n";

/// A function whose signature carries two more findings next to the echo-comment one.
const ORDER_FN: &str = "pub fn process_order(id: &str, account: &str, qty: u64) -> Result<u64, Box<dyn std::error::Error>> {\n    Ok(0)\n}\n";

// A035: a directive next to a doc comment must not become part of the doc text. Folded in, it
// dilutes the restated name, so the echo finding never exists and the directive that should
// remove it is reported as unused instead. Rust `///` placement only; Python and Java doc
// placement is pinned in tests/syntax_kernel_contracts/directive_docs.rs.
#[test]
fn directive_beside_a_restating_doc_removes_only_the_echo_finding() {
    let control = report_of("src/lib.rs", &format!("{DOC}{ORDER_FN}"), "");
    assert_eq!(
        rule_lines(&control),
        [
            ("slop/echo-comment", 1),
            ("agent/stringly-api", 2),
            ("agent/typed-error-boundary", 2),
        ],
        "control: the restating doc fires before any directive exists"
    );
    let directive = "// sloplint-allow(slop/echo-comment): the name is the contract\n";
    let suppressed = report_of("src/lib.rs", &format!("{directive}{DOC}{ORDER_FN}"), "");
    assert_eq!(
        rule_lines(&suppressed),
        [("agent/stringly-api", 3), ("agent/typed-error-boundary", 3)],
        "the directive above the doc removes only the echo finding and is not reported unused"
    );
}

#[test]
fn invalid_and_unused_suppressions_report() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        for (text, expected) in [
            (
                "sloplint-allow(slop/echo-comment)",
                "meta/invalid-suppression",
            ),
            (
                "sloplint-allow(nope/rule): reason",
                "meta/invalid-suppression",
            ),
            (
                "sloplint-allow(slop/echo-comment): placeholder",
                "meta/unused-suppression",
            ),
        ] {
            let body = (sample.comment)(text);
            let report = sample_report(sample, &body, "");
            let line = directive_line(sample, &body);
            let actual = meta_lines(&report);
            if actual != [(expected, line)] {
                failures.push(format!(
                    "{} ({text}): expected [({expected}, {line})], got {actual:?}",
                    sample.label
                ));
            }
        }
    }
    assert_no_failures(&failures);
}

/// The `[[custom_rules]]` table whose query matches `sample`'s function declarations.
fn custom_rule(sample: &Sample) -> String {
    format!(
        "[[custom_rules]]\nid = \"custom/fn-decl\"\nlanguage = \"{}\"\nseverity = \"warn\"\nmessage = \"declared\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"{}\"\n",
        sample.language, sample.query
    )
}

#[test]
fn suppressions_for_inactive_rules_stay_silent() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let active = custom_rule(sample);
        let inactive = format!("{active}[rules.\"custom/fn-decl\"]\nlevel = \"off\"\n");
        let control = sample_report(sample, "", &active);
        if !fired(&control, "custom/fn-decl") {
            failures.push(format!(
                "{}: the custom query must fire on the bare sample",
                sample.label
            ));
        }
        for (operand, kind) in [("custom/fn-decl", "exact"), ("custom/*", "family")] {
            let body = (sample.comment)(&directive(operand, "reviewed"));
            let covered = sample_report(sample, &body, &active);
            if fired(&covered, "custom/fn-decl") || fired(&covered, "meta/unused-suppression") {
                failures.push(format!(
                    "{} ({kind}): a covering directive must drop the finding and not be unused",
                    sample.label
                ));
            }
            let unmatched = sample_report(sample, &format!("{body}\n"), &active);
            if !fired(&unmatched, "meta/unused-suppression") {
                failures.push(format!(
                    "{} ({kind}): an unmatched directive must be reported unused",
                    sample.label
                ));
            }
            let silent = sample_report(sample, &body, &inactive);
            if fired(&silent, "custom/fn-decl")
                || fired(&silent, "meta/unused-suppression")
                || fired(&silent, "meta/invalid-suppression")
            {
                failures.push(format!(
                    "{} ({kind}): a directive for an inactive rule must stay silent and valid",
                    sample.label
                ));
            }
        }
    }
    assert_no_failures(&failures);
}

/// Known defect, reported and not fixed. `reason_until_terminator` in
/// `crates/sloplint-core/src/suppress.rs` ends a reason at a newline, `*/`, or `-->` only. A block
/// comment closed by any other delimiter keeps that delimiter as its reason, so a blank reason is
/// accepted instead of reported as `meta/invalid-suppression`. Repro: `(* sloplint-allow(grep/generic-name):   *)`
/// above `let handle` in `src/lib.ml`.
#[test]
#[ignore = "known defect: block comments with other closers accept a blank reason"]
fn blank_reason_is_invalid_in_every_sample() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let body = (sample.comment)("sloplint-allow(grep/generic-name):   ");
        let report = sample_report(sample, &body, "");
        let line = directive_line(sample, &body);
        let actual = meta_lines(&report);
        if actual != [("meta/invalid-suppression", line)] {
            failures.push(format!(
                "{}: expected a blank reason at line {line} to be invalid, got {actual:?}",
                sample.label
            ));
        }
    }
    assert_no_failures(&failures);
}
