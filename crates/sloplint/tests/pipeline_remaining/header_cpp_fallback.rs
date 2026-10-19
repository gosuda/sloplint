//! A `.h` file routes to C. When its C parse fails and a C++ parse is clean, the file is
//! analyzed as C++: its facts, its language tables, and its rules are the C++ ones, while the
//! routed language still decides which files a run selects.
//! Observed through `run`: findings, summary counts, and the reported parse-error position.

use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::{Finding, Severity};

const PARSE_ERROR: &str = "meta/parse-error";
const WRAPPER: &str = "slop/passthrough-wrapper";

const CPP_HEADER: &str = "#pragma once
#include <string>
#include <vector>

namespace ui {

template <typename T>
class Stack {
public:
    void push(const T& value) { items_.push_back(value); }
    bool empty() const { return items_.empty(); }
private:
    std::vector<T> items_;
};

}  // namespace ui
";

/// The existing C fixture of `slop/passthrough-wrapper`, read as a `.h` file below.
const C_SAMPLE: &str = include_str!("../fixtures/slop/passthrough-wrapper__c/pos.c");

/// The `template` on line 1 fails the C parse; `= ;` on line 3 fails the C++ parse.
const INVALID_IN_BOTH: &str = "template <typename T>
struct Box { T value; };
int x = ;
";

/// A header that fails as C. Three rules read a per-language table here: `slop/zombie-catch`
/// and `grep/contiguous-log-literal` the log callees (`spdlog::*` is in the C++ table and not
/// in the C one), and `agent/stringly-api` the primitive types (`std::string` likewise).
const LOGGING_HEADER: &str = "#include <string>
namespace app {
template <typename T>
struct Box { T value; };

void risky();

void load(const std::string& path) {
    try {
        risky();
    } catch (const std::exception& e) {
        spdlog::error(\"load failed\");
        throw;
    }
    spdlog::info(path);
}

void configure(std::string host, std::string name) {}
}  // namespace app
";

fn project(config: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", config);
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn analyze_project(root: &Path) -> Report {
    let options = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    };
    run(&options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn of_rule<'r>(report: &'r Report, rule: &str) -> Vec<&'r Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.rule.as_ref() == rule)
        .collect()
}

/// Every field of every finding except `path` and `fingerprint`, which both depend on the
/// file name, in report order. The colocated-tests fix names the missing test file after the
/// source file's extension, so a twin's report passes `rename` as `(from, to)` to read its
/// file name as the header's.
fn bodies(
    report: &Report,
    rename: Option<(&str, &str)>,
) -> Vec<(String, Severity, u32, u32, String, String, String)> {
    report
        .findings
        .iter()
        .map(|f| {
            (
                f.rule.to_string(),
                f.severity,
                f.line,
                f.column,
                f.message.clone(),
                rename.map_or_else(|| f.fix.clone(), |(from, to)| f.fix.replace(from, to)),
                f.context.to_string(),
            )
        })
        .collect()
}

/// The complete findings of `C_SAMPLE` at `plain.h` under the strict profile and an empty
/// configuration, as the binary built from the commit before the header fallback reported
/// them, fingerprints included.
fn plain_header_findings_before_the_fallback() -> Vec<Finding> {
    vec![
        Finding {
            rule: "agent/colocated-tests".into(),
            severity: Severity::Warn,
            path: "plain.h".into(),
            line: 1,
            column: 1,
            message: "module with 1 public functions has no tests".to_string(),
            fix: "add plain_test.h".to_string(),
            context: "".into(),
            fingerprint: "a6981459548c466a58b21a203054c8f8".into(),
        },
        Finding {
            rule: WRAPPER.into(),
            severity: Severity::Warn,
            path: "plain.h".into(),
            line: 3,
            column: 15,
            message: "`get_length` only forwards its parameters to `strlen`".to_string(),
            fix: "call `strlen` directly at the call sites of `get_length` and delete `get_length`"
                .to_string(),
            context: "unsigned long get_length(const char *s) {".into(),
            fingerprint: "b5d276d5a4cf3599b5466afab353f485".into(),
        },
    ]
}

#[test]
fn a_cpp_header_in_a_dot_h_file_is_analyzed_as_cpp_without_a_parse_error() {
    let (_tmp, root) = project("");
    put(&root, "include/stack.h", CPP_HEADER);
    let report = analyze_project(&root);
    assert!(
        of_rule(&report, PARSE_ERROR).is_empty(),
        "a header that parses as C++ is not a parse error: {:#?}",
        report.findings
    );
    assert_eq!(report.summary.files, 1, "the header is analyzed");
    assert_eq!(report.summary.skipped, 0, "the header is not skipped");
    let wrappers = of_rule(&report, WRAPPER);
    assert_eq!(wrappers.len(), 1, "one forwarding method: {wrappers:#?}");
    assert_eq!(wrappers[0].path.as_ref(), "include/stack.h");
    assert_eq!(wrappers[0].line, 10, "`push` is declared on line 10");
    assert!(
        wrappers[0].message.contains("`push`"),
        "the finding names the C++ method: {}",
        wrappers[0].message
    );
}

#[test]
fn a_plain_c_header_reports_what_the_same_bytes_report_in_a_dot_c_file() {
    let (_tmp, root) = project("");
    put(&root, "plain.h", C_SAMPLE);
    let header = analyze_project(&root);
    assert_eq!(
        header.findings,
        plain_header_findings_before_the_fallback(),
        "plain C in a `.h` file keeps every field of the findings it had before the fallback existed"
    );
    let (_twin, twin_root) = project("");
    put(&twin_root, "plain.c", C_SAMPLE);
    let source = analyze_project(&twin_root);
    assert_eq!(
        bodies(&header, None),
        bodies(&source, Some(("plain_test.c", "plain_test.h"))),
        "the `.h` and `.c` spellings of one C source report the same findings"
    );
}

#[test]
fn a_fallback_header_follows_the_cpp_log_and_type_tables() {
    let (_tmp, root) = project("");
    put(&root, "include/log.h", LOGGING_HEADER);
    let report = analyze_project(&root);
    assert!(
        of_rule(&report, PARSE_ERROR).is_empty(),
        "the header parses as C++: {:#?}",
        report.findings
    );
    let zombies = of_rule(&report, "slop/zombie-catch");
    assert_eq!(zombies.len(), 1, "the C++ table knows spdlog: {zombies:#?}");
    assert_eq!(zombies[0].line, 11, "the handler is the `catch` on line 11");
    assert_eq!(zombies[0].severity, Severity::Deny);
    assert_eq!(
        zombies[0].message,
        "error handler logs and rethrows the same error"
    );
    assert_eq!(
        zombies[0].fix,
        "either log and stop propagation here, or propagate without logging; doing both reports the failure twice"
    );
    let literals: Vec<u32> = of_rule(&report, "grep/contiguous-log-literal")
        .iter()
        .map(|f| f.line)
        .collect();
    assert_eq!(literals, [12, 15], "both `spdlog` calls are log calls");
    let stringly = of_rule(&report, "agent/stringly-api");
    assert_eq!(stringly.len(), 1, "`std::string` is a C++ primitive");
    assert_eq!(stringly[0].line, 18);
    assert!(
        stringly[0].message.contains("(host, name)"),
        "the finding names the run: {}",
        stringly[0].message
    );
}

#[test]
fn a_fallback_header_reports_what_the_same_bytes_report_in_a_dot_cpp_file() {
    let (_tmp, root) = project("");
    put(&root, "log.h", LOGGING_HEADER);
    let header = analyze_project(&root);
    let (_twin, twin_root) = project("");
    put(&twin_root, "log.cpp", LOGGING_HEADER);
    let source = analyze_project(&twin_root);
    assert!(
        header.findings.len() >= 5,
        "a vacuous comparison proves nothing: {:#?}",
        header.findings
    );
    assert_eq!(
        bodies(&header, None),
        bodies(&source, Some(("log_test.cpp", "log_test.h"))),
        "a header that parsed as C++ reads exactly as the same bytes in a `.cpp` file"
    );
}

/// The language tables a rule consults come from the grammar that parsed the file. With `c`
/// listing `spdlog::error` and `cpp` listing `Trace`, only the second handler is a log and
/// rethrow when the header was parsed as C++.
#[test]
fn a_fallback_header_reads_the_cpp_table_of_the_configuration() {
    let config = "[languages.c]
log_callees = [\"spdlog::error\"]
[languages.cpp]
log_callees = [\"Trace\"]
";
    let (_tmp, root) = project(config);
    let source = "#include <string>
template <typename T>
struct Box { T value; };
void risky();
void first() {
    try { risky(); } catch (...) { spdlog::error(\"a\"); throw; }
}
void second() {
    try { risky(); } catch (...) { Trace(\"b\"); throw; }
}
";
    put(&root, "handlers.h", source);
    let report = analyze_project(&root);
    assert!(
        of_rule(&report, PARSE_ERROR).is_empty(),
        "{:#?}",
        report.findings
    );
    let lines: Vec<u32> = of_rule(&report, "slop/zombie-catch")
        .iter()
        .map(|f| f.line)
        .collect();
    assert_eq!(
        lines,
        [9],
        "`[languages.cpp]` marks the `Trace` handler, and `[languages.c]` is not consulted"
    );
}

#[test]
fn the_report_does_not_depend_on_the_job_count_over_a_mix_of_c_and_fallback_headers() {
    let (_tmp, root) = project("");
    for n in 0..8 {
        put(&root, &format!("c/plain_{n}.h"), C_SAMPLE);
        put(&root, &format!("c/unit_{n}.c"), C_SAMPLE);
        put(&root, &format!("cpp/log_{n}.h"), LOGGING_HEADER);
        put(&root, &format!("bad/box_{n}.h"), INVALID_IN_BOTH);
    }
    let run_with = |jobs: usize| {
        let options = RunOptions {
            cwd: root.clone(),
            root: Some(root.clone()),
            strictness: Some(Strictness::Strict),
            mode: Some(Mode::Threshold),
            use_cache: false,
            jobs,
            ..RunOptions::default()
        };
        run(&options).unwrap_or_else(|e| panic!("run with {jobs} jobs failed: {e}"))
    };
    let serial = run_with(1);
    assert_eq!(
        serial.summary.files, 32,
        "every header and source is analyzed"
    );
    assert_eq!(
        of_rule(&serial, "slop/zombie-catch").len(),
        8,
        "each fallback header reports its C++ handler: {:#?}",
        serial.findings
    );
    assert_eq!(
        of_rule(&serial, PARSE_ERROR).len(),
        8,
        "only the headers that fail as C and as C++ report a parse error"
    );
    for jobs in [3, 8] {
        let parallel = run_with(jobs);
        assert_eq!(
            parallel.findings, serial.findings,
            "{jobs} jobs reorder or change the findings of one job"
        );
        assert_eq!(parallel.summary.files, serial.summary.files);
        assert_eq!(parallel.summary.findings, serial.summary.findings);
        assert_eq!(parallel.summary.skipped, serial.summary.skipped);
    }
}

#[test]
fn a_header_invalid_in_both_grammars_reports_the_c_parse_error_position() {
    let (_tmp, root) = project("");
    put(&root, "box.h", INVALID_IN_BOTH);
    let report = analyze_project(&root);
    assert_eq!(
        report.findings.len(),
        1,
        "an unparseable file is judged by its syntax error alone: {:#?}",
        report.findings
    );
    let finding = &report.findings[0];
    assert_eq!(finding.rule.as_ref(), PARSE_ERROR);
    assert_eq!(finding.path.as_ref(), "box.h");
    assert_eq!(
        (finding.line, finding.column),
        (1, 1),
        "the position is the C parse's first error, `template` on line 1, not the C++ error on line 3"
    );
    assert_eq!(finding.context.as_ref(), "template <typename T>");
    put(&root, "box.c", INVALID_IN_BOTH);
    let both = analyze_project(&root);
    let positions: Vec<(&str, u32, u32)> = both
        .findings
        .iter()
        .map(|f| (f.path.as_ref(), f.line, f.column))
        .collect();
    assert_eq!(
        positions,
        [("box.c", 1, 1), ("box.h", 1, 1)],
        "the `.h` file reports where the `.c` file with the same bytes reports"
    );
}

#[test]
fn a_cpp_source_in_a_dot_c_file_still_reports_a_parse_error() {
    let (_tmp, root) = project("");
    put(&root, "stack.c", CPP_HEADER);
    let report = analyze_project(&root);
    let errors = of_rule(&report, PARSE_ERROR);
    assert_eq!(errors.len(), 1, "only `.h` files fall back: {errors:#?}");
    assert_eq!(errors[0].path.as_ref(), "stack.c");
}

#[test]
fn the_routed_language_alone_selects_headers() {
    let (_tmp, root) = project("[project]\nlanguages = [\"c\"]\n");
    put(&root, "include/stack.h", CPP_HEADER);
    put(
        &root,
        "src/note.cpp",
        "int twice(int n) { return n * 2; }\n",
    );
    let selected = analyze_project(&root);
    assert_eq!(
        selected.summary.files, 1,
        "`languages = [\"c\"]` selects the `.h` file and not the `.cpp` file"
    );
    assert_eq!(
        of_rule(&selected, WRAPPER).len(),
        1,
        "the selected header is analyzed as C++: {:#?}",
        selected.findings
    );
    put(&root, "sloplint.toml", "[project]\nlanguages = [\"cpp\"]\n");
    let excluded = analyze_project(&root);
    assert_eq!(
        excluded.summary.files, 1,
        "`languages = [\"cpp\"]` selects the `.cpp` file and not the `.h` file"
    );
    assert!(
        excluded
            .findings
            .iter()
            .all(|f| f.path.as_ref() != "include/stack.h"),
        "the header stays unselected when only `cpp` is enabled: {:#?}",
        excluded.findings
    );
}
