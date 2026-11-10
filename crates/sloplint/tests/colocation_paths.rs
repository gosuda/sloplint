//! Web colocated-test lookup: a test file covers only modules it shares a key with.

use std::path::Path;

use sloplint::{RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const SOURCE: &str = "export function widget(): void {}\n";
const STUB: &str = "export const x = 1;\n";
const RULE: &str = "agent/colocated-tests";

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn flagged(root: &Path) -> bool {
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![RulePattern::parse(RULE).unwrap()]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    })
    .unwrap();
    report.findings.iter().any(|f| &*f.rule == RULE)
}

fn root_with(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "src/widget.ts", SOURCE);
    for (rel, contents) in extra {
        write(tmp.path(), rel, contents);
    }
    tmp
}

#[test]
fn no_evidence_stays_silent() {
    let tmp = root_with(&[]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: src/widget.ts with no test evidence anywhere must stay silent"
    );
}

#[test]
fn sibling_test_still_suppresses() {
    let tmp = root_with(&[("src/widget.test.ts", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: src/widget.test.ts must suppress the finding"
    );
}

#[test]
fn tests_dir_suppresses() {
    let tmp = root_with(&[("src/__tests__/widget.test.ts", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: src/__tests__/widget.test.ts must suppress the finding"
    );
}

#[test]
fn tests_dir_any_extension_suppresses() {
    let tmp = root_with(&[("src/__tests__/widget.test.tsx", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: tsx variant in __tests__ must suppress the finding"
    );
}

#[test]
fn different_stem_test_does_not_suppress() {
    let tmp = root_with(&[("src/__tests__/gadget.test.ts", STUB)]);
    assert!(
        flagged(tmp.path()),
        "key cover: a different stem shares no key, so widget stays flagged"
    );
}

#[test]
fn other_package_tests_do_not_suppress() {
    let tmp = root_with(&[
        ("src/__tests__/gadget.test.ts", STUB),
        ("other/__tests__/widget.test.ts", STUB),
    ]);
    assert!(
        flagged(tmp.path()),
        "wave-self-lint: other/__tests__ must not suppress src/widget.ts"
    );
}

#[test]
fn unrelated_library_file_does_not_suppress() {
    let tmp = root_with(&[
        ("src/helper.ts", SOURCE),
        ("src/__tests__/gadget.test.ts", STUB),
        ("other/__tests__/widget.test.ts", STUB),
    ]);
    assert!(
        flagged(tmp.path()),
        "wave-self-lint: unrelated src/helper.ts must not suppress src/widget.ts"
    );
}
fn reported_files(root: &Path) -> Vec<String> {
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![RulePattern::parse(RULE).unwrap()]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    })
    .unwrap();
    let mut out: Vec<String> = report
        .findings
        .iter()
        .filter(|f| &*f.rule == RULE)
        .map(|f| f.path.to_string())
        .collect();
    out.sort();
    out
}
fn tmp_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (rel, contents) in files {
        write(tmp.path(), rel, contents);
    }
    tmp
}
#[test]
fn deep_path_with_many_long_imports_stays_bounded() {
    let deep = "d/".repeat(1500) + "x.py";
    let mut files = vec![
        ("pkg/core.py", "def core():\n    return 1\n"),
        ("pkg/other.py", "def other():\n    return 2\n"),
    ];
    let mut big = String::new();
    for k in 0..100 {
        big.push_str("import ");
        big.push_str(&vec!["a"; 1999].join("."));
        big.push_str(&format!(".x{k}\n"));
    }
    files.push(("d/test_big.py", big.as_str()));
    files.push((deep.as_str(), "def run():\n    return 1\n"));
    let tmp = tmp_with(&files);
    let expected = vec![deep];
    assert_eq!(reported_files(tmp.path()), expected);
}
#[test]
fn go_bare_stdlib_does_not_cover_repo_errors() {
    let tmp = tmp_with(&[
        (
            "errors.go",
            "package errors\n\nfunc Do() string {\n\treturn \"x\"\n}\n",
        ),
        (
            "other.go",
            "package other\n\nfunc Do() string {\n\treturn \"x\"\n}\n",
        ),
        (
            "x_test.go",
            "package main\n\nimport \"errors\"\nimport \"testing\"\n\nfunc TestX(t *testing.T) {\n\t_ = errors.New(\"x\")\n}\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        ["errors.go".to_string(), "other.go".to_string()]
    );
}
#[test]
fn python_bare_module_does_not_cover_nested_utils() {
    let tmp = tmp_with(&[
        ("billing/utils.py", "def run():\n    return 1\n"),
        ("billing/other.py", "def other():\n    return 2\n"),
        (
            "billing/test_money.py",
            "from utils import money\n\ndef test_money():\n    assert True\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        [
            "billing/other.py".to_string(),
            "billing/utils.py".to_string()
        ]
    );
}
#[test]
fn python_from_package_import_covers_only_named_module() {
    let tmp = tmp_with(&[
        ("pkg/core.py", "def core():\n    return 1\n"),
        ("pkg/other.py", "def other():\n    return 2\n"),
        (
            "pkg/test_x.py",
            "from pkg import core\n\ndef test_core():\n    assert True\n",
        ),
    ]);
    assert_eq!(reported_files(tmp.path()), ["pkg/other.py".to_string()]);
}
#[test]
fn python_double_dot_resolves_to_parent_package() {
    let tmp = tmp_with(&[
        ("tests/core.py", "def f():\n    return 1\n"),
        ("app/unit/core.py", "def run():\n    return 1\n"),
        (
            "tests/unit/test_x.py",
            "from ..core import f\n\ndef test_x():\n    assert True\n",
        ),
        ("app/test_other.py", "def test_other():\n    assert True\n"),
    ]);
    assert_eq!(reported_files(tmp.path()), ["app/unit/core.py".to_string()]);
}
#[test]
fn rust_hyphenated_nested_crate_resolves_through_crate_name() {
    let tmp = tmp_with(&[
        (
            "libs/my-parser/src/rules/lexer.rs",
            "pub fn tokenize() -> &'static str {\n    \"x\"\n}\n",
        ),
        ("libs/my-parser/src/rules/other.rs", "pub fn other() {}\n"),
        (
            "libs/my-parser/tests/t.rs",
            "#[cfg(test)]\nmod t {\n    use my_parser::rules::lexer::tokenize;\n    #[test]\n    fn t() {\n        assert_eq!(tokenize(), \"x\");\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        ["libs/my-parser/src/rules/other.rs".to_string()]
    );
}
#[test]
fn rust_production_use_does_not_count_as_test_coverage() {
    let tmp = tmp_with(&[
        (
            "crates/app/src/driver.rs",
            "use parser::lexer::tokenize;\n\npub fn drive() -> String {\n    tokenize().to_string()\n}\n\n#[cfg(test)]\nmod tests {\n    use super::drive;\n    #[test]\n    fn t() {\n        assert_eq!(drive(), \"x\");\n    }\n}\n",
        ),
        (
            "crates/parser/src/lexer.rs",
            "pub fn tokenize() -> &'static str {\n    \"x\"\n}\n",
        ),
        ("crates/parser/src/other.rs", "pub fn other() {}\n"),
        ("crates/parser/tests/other.rs", "fn helper() {}\n"),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        [
            "crates/parser/src/lexer.rs".to_string(),
            "crates/parser/src/other.rs".to_string()
        ]
    );
}
#[test]
fn rust_nested_module_import_covers_named_module_only() {
    let tmp = tmp_with(&[
        ("crates/parser/src/rules/lexer.rs", "pub fn tokenize() {}\n"),
        ("crates/parser/src/rules/other.rs", "pub fn other() {}\n"),
        (
            "crates/parser/tests/t.rs",
            "use parser::rules::lexer::tokenize;\n\n#[test]\nfn t() {\n    tokenize();\n}\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        ["crates/parser/src/rules/other.rs".to_string()]
    );
}
#[test]
fn typescript_dotted_basename_resolves_through_first_dot_stem() {
    let tmp = tmp_with(&[
        (
            "src/app/user.service.ts",
            "export function make(): void {}\n",
        ),
        ("src/app/other.ts", "export function other(): void {}\n"),
        (
            "src/app/user.service.test.ts",
            "import { make } from \"./user.service\";\n\nexport const t = 1;\n",
        ),
    ]);
    assert_eq!(reported_files(tmp.path()), ["src/app/other.ts".to_string()]);
}
#[test]
fn python_src_layout_single_import_covers_top_module() {
    let tmp = tmp_with(&[
        ("src/tag.py", "def f():\n    return 1\n"),
        ("src/other.py", "def g():\n    return 2\n"),
        (
            "src/test_x.py",
            "import tag\n\ndef test_x():\n    assert True\n",
        ),
    ]);
    assert_eq!(reported_files(tmp.path()), ["src/other.py".to_string()]);
}
#[test]
fn typescript_alias_prefix_does_not_cover() {
    let tmp = tmp_with(&[
        ("src/tag.ts", "export function f(): void {}\n"),
        ("src/other.ts", "export function g(): void {}\n"),
        (
            "src/dummy.test.ts",
            "import { f } from \"@/tag\";\n\nexport const t = 1;\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        ["src/other.ts".to_string(), "src/tag.ts".to_string()]
    );
}
#[test]
fn go_module_prefix_without_path_suffix_does_not_cover() {
    let tmp = tmp_with(&[
        ("tag.go", "package tag\n\nfunc Add() int {\n\treturn 1\n}\n"),
        (
            "other.go",
            "package tag\n\nfunc Sub() int {\n\treturn 1\n}\n",
        ),
        (
            "app_test.go",
            "package app\n\nimport \"example.com/m/tag\"\nimport \"testing\"\n\nfunc TestApp(t *testing.T) {\n}\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        ["other.go".to_string(), "tag.go".to_string()]
    );
}

#[test]
fn typescript_dot_slash_import_covers_colocated_module() {
    let tmp = tmp_with(&[
        ("src/services/user.ts", "export function f(): void {}\n"),
        ("src/services/other.ts", "export function g(): void {}\n"),
        (
            "src/services/user.test.ts",
            "import { f } from \"./user\";\n\nexport const t = f;\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        ["src/services/other.ts".to_string()]
    );
}

#[test]
fn python_three_segment_import_needs_full_suffix() {
    let tmp = tmp_with(&[
        ("legacy/services/user.py", "def run():\n    return 1\n"),
        ("legacy/services/other.py", "def other():\n    return 2\n"),
        (
            "legacy/test_user.py",
            "from app.services.user import run\n\ndef test_run():\n    assert True\n",
        ),
    ]);
    assert_eq!(
        reported_files(tmp.path()),
        [
            "legacy/services/other.py".to_string(),
            "legacy/services/user.py".to_string()
        ]
    );
}
