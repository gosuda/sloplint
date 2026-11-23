//! Discovery policy and caller-relative paths through the public API.

use std::path::{Path, PathBuf};

use sloplint::cli::Format;
use sloplint::rules_cmd::{RulesRequest, render};
use sloplint::{RunError, RunOptions, execute, implicit_scope};
use sloplint_core::suppress::RulePattern;
use sloplint_core::{FailOn, Mode, RuleFilter, Strictness};

const COVERAGE: &str = "agent/colocated-tests";

fn project() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary project");
    let root = temp.path().canonicalize().expect("canonical project root");
    (temp, root)
}

fn put(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("file parent")).expect("create parent");
    std::fs::write(path, source).expect("write project file");
}

fn options(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        fail_on: Some(FailOn::Never),
        rule_filter: RuleFilter(vec![RulePattern::parse(COVERAGE).expect("coverage rule")]),
        jobs: 1,
        ..RunOptions::default()
    }
}

#[test]
fn data_manifests_neither_define_packages_nor_count_as_skips() {
    for (manifest, library, source, test, test_source) in [
        (
            "pyproject.toml",
            "libarea/service.py",
            "def render(name):\n    return name\n",
            "testarea/test_support.py",
            "def test_support():\n    assert True\n",
        ),
        (
            "package.json",
            "libarea/service.js",
            "export function render(name) { return name; }\n",
            "testarea/support.test.js",
            "function testSupport() { return true; }\n",
        ),
    ] {
        let (_temp, root) = project();
        put(&root, library, source);
        put(&root, test, test_source);
        let manifest_source = if manifest.ends_with(".json") {
            "{\"name\":\"demo\"}\n"
        } else {
            "[project]\nname = \"demo\"\n"
        };
        put(&root, manifest, manifest_source);
        let opts = RunOptions {
            fail_on_skipped: true,
            ..options(&root)
        };
        let covered_package = execute(&opts).expect("root manifest run");
        assert!(
            covered_package
                .report
                .findings
                .iter()
                .any(|finding| { &*finding.path == library && &*finding.rule == COVERAGE }),
            "the root manifest must unite library and unrelated test: {manifest}"
        );

        put(
            &root,
            "sloplint.toml",
            &format!("[project]\ndata_globs = [\"{manifest}\"]\n"),
        );
        let excluded = execute(&opts).expect("excluded root manifest run");
        assert!(
            excluded.report.findings.is_empty(),
            "a data manifest must not unite the fallback packages: {manifest}: {:?}",
            excluded.report.findings
        );
        assert_eq!(excluded.report.summary.files, 2, "{manifest}");
        assert_eq!(excluded.report.summary.skipped, 0, "{manifest}");
        assert!(
            !excluded.failed,
            "a non-source manifest must not trip the skip gate"
        );

        let nested = format!("libarea/{manifest}");
        put(&root, &nested, manifest_source);
        put(
            &root,
            "sloplint.toml",
            &format!("[project]\ndata_globs = [\"{nested}\"]\n"),
        );
        let united = execute(&opts).expect("excluded nested manifest run");
        assert!(
            united
                .report
                .findings
                .iter()
                .any(|finding| { &*finding.path == library && &*finding.rule == COVERAGE }),
            "a data manifest must not split a package defined by the root manifest: {manifest}"
        );
        assert_eq!(united.report.summary.skipped, 0, "{manifest}");
    }
}

#[test]
fn readable_data_directories_count_only_enabled_source_files() {
    let (_temp, root) = project();
    put(&root, "sample.rs", "pub fn render() {}\n");
    put(&root, "fixtures/a.rs", "pub fn example() {}\n");
    put(&root, "fixtures/nested/b.rs", "pub fn example() {}\n");
    put(&root, "fixtures/disabled.py", "def example():\n    pass\n");
    put(&root, "fixtures/expected.json", "{}\n");
    put(
        &root,
        "sloplint.toml",
        "[project]\nlanguages = [\"rust\"]\ndata_globs = [\"fixtures\"]\n",
    );
    let checked = execute(&RunOptions {
        fail_on_skipped: true,
        ..options(&root)
    })
    .expect("data source run");
    assert_eq!(
        (checked.report.summary.files, checked.report.summary.skipped),
        (1, 2)
    );
    let skips: Vec<_> = checked
        .report
        .skipped_gate
        .iter()
        .map(|entry| (&*entry.path, &*entry.reason))
        .collect();
    assert_eq!(
        skips,
        [("fixtures/a.rs", "data"), ("fixtures/nested/b.rs", "data")]
    );
}

#[cfg(unix)]
struct LockedDir {
    path: PathBuf,
    permissions: std::fs::Permissions,
}

#[cfg(unix)]
impl LockedDir {
    fn new(path: PathBuf) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::metadata(&path)
            .expect("directory metadata")
            .permissions();
        let guard = Self { path, permissions };
        std::fs::set_permissions(&guard.path, std::fs::Permissions::from_mode(0o0))
            .expect("deny directory access");
        if std::fs::read_dir(&guard.path).is_ok() {
            let context = "directory access remains permitted after chmod(0)";
            if std::env::var_os("SLOPLINT_REQUIRE_FS_CAPS").is_some_and(|value| !value.is_empty()) {
                panic!("SLOPLINT_REQUIRE_FS_CAPS is set but the capability is missing: {context}");
            }
            eprintln!("SKIP (capability unavailable): {context}");
            return None;
        }
        Some(guard)
    }
}

#[cfg(unix)]
impl Drop for LockedDir {
    fn drop(&mut self) {
        std::fs::set_permissions(&self.path, self.permissions.clone())
            .expect("restore directory access");
    }
}

#[cfg(unix)]
fn assert_unreadable_data_directory_is_scoped_skip(directory: &str, config: &str) {
    let (_temp, root) = project();
    put(&root, "sample.rs", "pub fn render() {}\n");
    put(
        &root,
        &format!("{directory}/fixture.rs"),
        "pub fn example() {}\n",
    );
    put(&root, "sloplint.toml", config);
    let Some(_locked) = LockedDir::new(root.join(directory)) else {
        return;
    };
    let checked = execute(&options(&root)).expect("a data directory must not abort lint");
    assert_eq!(
        (checked.report.summary.files, checked.report.summary.skipped),
        (1, 1)
    );
    assert!(!checked.failed, "the skip gate is off");

    let gated = execute(&RunOptions {
        fail_on_skipped: true,
        ..options(&root)
    })
    .expect("a data directory is a gating skip, not an I/O failure");
    assert!(gated.failed);
    assert_eq!(gated.report.skipped_gate.len(), 1);
    assert_eq!(&*gated.report.skipped_gate[0].path, directory);
    assert_eq!(&*gated.report.skipped_gate[0].reason, "data");

    let scoped = execute(&RunOptions {
        paths: vec![PathBuf::from("sample.rs")],
        fail_on_skipped: true,
        ..options(&root)
    })
    .expect("an out-of-scope data directory must not abort lint");
    assert_eq!(scoped.report.summary.skipped, 0);
    assert!(
        !scoped.failed,
        "the data skip lies outside the requested scope"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_data_directories_are_scoped_skips() {
    assert_unreadable_data_directory_is_scoped_skip(
        "data/nested",
        "[project]\ndata_globs = [\"data\"]\n",
    );
}

#[cfg(unix)]
#[test]
fn unreadable_default_data_directories_are_scoped_skips() {
    for directory in ["testdata", "fixtures/testdata"] {
        assert_unreadable_data_directory_is_scoped_skip(directory, "");
    }
}

#[cfg(unix)]
#[test]
fn unreadable_data_directories_match_recursive_globs() {
    assert_unreadable_data_directory_is_scoped_skip(
        "data",
        "[project]\ndata_globs = [\"data/**\"]\n",
    );
}

#[test]
fn directory_data_globs_do_not_skip_a_same_named_source_file() {
    let (_temp, root) = project();
    put(&root, "sample.rs", "pub fn render() {}\n");
    put(
        &root,
        "sloplint.toml",
        "[project]\ndata_globs = [\"sample.rs/**\"]\n",
    );
    let checked = execute(&options(&root)).expect("a file is not a directory glob match");
    assert_eq!(
        (checked.report.summary.files, checked.report.summary.skipped),
        (1, 0)
    );
}

#[cfg(unix)]
#[test]
fn unreadable_source_directories_still_fail_beside_data_skips() {
    for (directory, config) in [
        ("data", "[project]\ndata_globs = [\"data\"]\n"),
        ("data", "[project]\ndata_globs = [\"data/**\"]\n"),
        ("testdata", ""),
    ] {
        let (_temp, root) = project();
        put(
            &root,
            &format!("{directory}/fixture.rs"),
            "pub fn example() {}\n",
        );
        put(&root, "private/source.rs", "pub fn render() {}\n");
        put(&root, "sloplint.toml", config);
        let Some(_data) = LockedDir::new(root.join(directory)) else {
            return;
        };
        let Some(_source) = LockedDir::new(root.join("private")) else {
            return;
        };
        let error = execute(&options(&root)).expect_err("an unreadable source directory must fail");
        assert_eq!(error.exit_code(), 3);
        assert!(error.to_string().contains("private"), "{error}");
    }
}

#[test]
fn a_relative_run_root_uses_the_supplied_working_directory() {
    let (_temp, base) = project();
    put(&base, "checkout/sloplint.toml", "");
    put(&base, "checkout/src/lib.rs", "pub fn render() {}\n");
    let checked = execute(&RunOptions {
        root: Some(PathBuf::from("checkout")),
        ..options(&base)
    })
    .expect("resolve the root against RunOptions.cwd");
    assert_eq!(checked.root, base.join("checkout"));
    assert_eq!(checked.report.summary.files, 1);
}

#[test]
fn a_relative_implicit_root_uses_the_supplied_working_directory() {
    let (_temp, base) = project();
    put(&base, "checkout/src/lib.rs", "pub fn render() {}\n");
    let cwd = base.join("checkout/src");
    let scope = implicit_scope(&cwd, Some(Path::new("..")), None)
        .expect("resolve the root against the supplied cwd");
    assert_eq!(scope, vec![cwd]);
}

#[test]
fn a_default_rules_request_renders_the_current_directory() {
    let text = render(&RulesRequest::default()).expect("render rules from the current directory");
    assert!(
        text.lines()
            .any(|line| line.starts_with("meta/parse-error level=")),
        "the default request must render the rule catalog: {text}"
    );
}

#[test]
fn a_relative_rules_root_loads_its_own_configuration() {
    let (_temp, base) = project();
    put(
        &base,
        "checkout/sloplint.toml",
        "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n",
    );
    let text = render(&RulesRequest {
        cwd: base,
        root: Some(PathBuf::from("checkout")),
        id: Some("grep/generic-name".to_string()),
        format: Format::Agent,
        ..RulesRequest::default()
    })
    .expect("resolve the root against RulesRequest.cwd");
    assert!(text.contains("grep/generic-name level=off"), "{text}");
}

#[test]
fn rules_reject_missing_explicit_roots_as_usage_errors() {
    let (_temp, base) = project();
    put(&base, "file.txt", "not a directory\n");
    for relative in ["missing", "file.txt/missing"] {
        let error = render(&RulesRequest {
            cwd: base.clone(),
            root: Some(PathBuf::from(relative)),
            ..RulesRequest::default()
        })
        .expect_err("a missing explicit root must fail");
        assert_eq!(error.exit_code(), 2);
        assert!(
            matches!(&error, RunError::PathNotFound { path } if *path == base.join(relative)),
            "{error:?}"
        );
    }
}

#[test]
fn rules_reject_an_explicit_file_root_as_a_usage_error() {
    let (_temp, base) = project();
    put(&base, "file.txt", "not a directory\n");
    let error = render(&RulesRequest {
        cwd: base.clone(),
        root: Some(PathBuf::from("file.txt")),
        ..RulesRequest::default()
    })
    .expect_err("an explicit file root must fail");
    assert_eq!(error.exit_code(), 2);
    assert!(
        matches!(&error, RunError::NotADirectory { path } if *path == base.join("file.txt")),
        "{error:?}"
    );
}

#[test]
fn rules_cli_rejects_invalid_roots_without_emitting_defaults() {
    let (_temp, base) = project();
    put(&base, "file.txt", "not a directory\n");
    for relative in ["missing", "file.txt", "file.txt/missing"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_sloplint"))
            .current_dir(&base)
            .args(["rules", "--root", relative, "--format", "json"])
            .output()
            .expect("spawn rules");
        assert_eq!(output.status.code(), Some(2), "{relative}: {output:?}");
        assert!(
            output.stdout.is_empty(),
            "{relative}: emitted default rules"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(relative.split('/').next().expect("a root component")),
            "{relative}: missing root in diagnostic: {output:?}"
        );
    }
}
