//! Cache fallback, tracking, refusal, and placement through the isolated per-user cache home.

use super::fired;
use super::{
    GENERIC, SAMPLE, cache_file, cache_files, cache_home, cli_in, corrupt_caches, json_args,
    namespace, opts, profile_args, project, put, quiet_stdout, read, run_ok, shown, str_of, strict,
    tally, text, xdg,
};
#[cfg(unix)]
use super::{assert_symlink_refused, key_dir, relocate_behind_link};
use sloplint::RunOptions;
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

// AA-S1: corrupt, truncated, and wrong-version caches fall back to the cold result and heal
// (rewritten to the valid bytes), with nothing on stderr. The CLI is the only cached path, in
// an isolated cache home; `--no-cache` runs are the cold truth and store nothing.
#[test]
fn corrupt_cache_files_fall_back_to_the_cold_result() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    let (warm_args, cold_args) = json_args();
    let cold = cli_in(&root, &home, &cold_args);
    let ok = cold.status.success() && text(&cold.stdout).contains(GENERIC);
    assert!(ok, "cold CLI run: {}", text(&cold.stderr));
    assert!(
        text(&cold.stderr).is_empty(),
        "--no-cache printed a warning"
    );
    assert!(cache_files(&home).is_empty(), "--no-cache stored a cache");
    let first = quiet_stdout(&root, &home, &warm_args);
    assert!(first == cold.stdout, "populating CLI run");
    let path = cache_file(&home);
    let valid = read(&path);
    for (what, bytes) in corrupt_caches(&valid) {
        std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert!(
            quiet_stdout(&root, &home, &warm_args) == cold.stdout,
            "{what}: stdout"
        );
        assert!(read(&path) == valid, "{what}: not rewritten whole");
    }
}

// AA-S3: the cache follows edits, including same-length ones.
#[test]
fn cached_runs_track_source_edits() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    let (warm_args, cold_args) = json_args();
    let same_as_cold = |what: &str| {
        let warm = quiet_stdout(&root, &home, &warm_args);
        assert!(warm == quiet_stdout(&root, &home, &cold_args), "{what}");
        text(&warm)
    };
    let first = same_as_cold("first");
    assert!(first.contains(GENERIC), "no finding");
    assert!(cache_file(&home).is_file(), "first run stored no cache");
    same_as_cold("warm");
    put(&root, "sample.rs", &SAMPLE.replace("handle", "settle"));
    let edited = same_as_cold("same-length edit");
    assert!(!edited.contains(GENERIC), "edit missed");
    put(&root, "sample.rs", SAMPLE);
    let reverted = same_as_cold("revert");
    assert!(reverted.contains(GENERIC), "revert missed");
}

// AA-S1 wrong key: a cache written under another effective config is not reused.
#[test]
fn cache_written_under_another_profile_is_not_reused() {
    let (_tmp, root) = project();
    put(&root, "tests/other.rs", "fn helper() {}\n");
    let (_cache_tmp, home) = cache_home();
    let (a, b) = (
        tally(&run_ok(&strict(&root))).0,
        tally(&run_ok(&opts(&root))).0,
    );
    assert_ne!(a, b, "profiles must differ on the sample");
    let (strict_warm, strict_cold) = profile_args("strict");
    let (standard_warm, standard_cold) = json_args();
    let strict_text = quiet_stdout(&root, &home, &strict_cold);
    let standard_text = quiet_stdout(&root, &home, &standard_cold);
    let populated = quiet_stdout(&root, &home, &strict_warm);
    assert!(populated == strict_text, "populating run");
    let standard = quiet_stdout(&root, &home, &standard_warm);
    assert!(standard == standard_text, "stale strict cache used");
    let again = quiet_stdout(&root, &home, &strict_warm);
    assert!(again == strict_text, "and back again");
}

// AA-S16: a symlinked per-user namespace directory holding a foreign-profile cache is refused:
// the findings are the cold ones, the outside cache is neither reused nor rewritten, and the
// CLI warns that the cache is disabled because of the symlink.
#[cfg(unix)]
#[test]
fn symlinked_cache_namespace_is_refused_and_leaves_outside_unchanged() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    let (_outside_tmp, outside) = cache_home();
    let (strict_warm, _) = profile_args("strict");
    quiet_stdout(&root, &home, &strict_warm);
    let link = namespace(&home);
    relocate_behind_link(&home, &outside, &link);
    assert_symlink_refused(&root, &home, &link, &outside);
}

// AA-S16: a symlinked per-project cache directory holding a foreign-profile cache is refused
// the same way, even though the namespace directory above it is a real directory.
#[cfg(unix)]
#[test]
fn symlinked_cache_key_directory_is_refused_and_leaves_outside_unchanged() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    let (_outside_tmp, outside) = cache_home();
    let (strict_warm, _) = profile_args("strict");
    quiet_stdout(&root, &home, &strict_warm);
    let link = key_dir(&home);
    relocate_behind_link(&home, &outside, &link);
    assert_symlink_refused(&root, &home, &link, &outside);
}

// AA-S16 (store): a regular file blocking the per-user namespace directory costs the cache but
// not the run. The command still succeeds with the cold findings while stderr carries a cache
// warning naming the blocked directory, and the blocker is left in place.
#[test]
fn cache_directory_blocked_by_file_warns_but_still_reports() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    let blocker = namespace(&home);
    std::fs::write(&blocker, "blocker").unwrap_or_else(|e| panic!("write blocker: {e}"));
    let (warm_args, cold_args) = json_args();
    let cold = cli_in(&root, &home, &cold_args);
    assert!(cold.status.success(), "cold: {}", text(&cold.stderr));
    assert!(
        cold.stderr.is_empty(),
        "--no-cache warned: {}",
        text(&cold.stderr)
    );
    let cold_stdout = text(&cold.stdout);
    assert!(cold_stdout.contains(GENERIC), "cold lost the finding");
    let out = cli_in(&root, &home, &warm_args);
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(
        out.status.success(),
        "blocked cache failed the run: {stderr}"
    );
    assert_eq!(
        stdout, cold_stdout,
        "finding lost when the cache is unwritable"
    );
    let not_written = stderr.contains("sloplint: warning: cache not written in ");
    let disabled = stderr.contains("sloplint: warning: cache disabled: ");
    assert!(not_written || disabled, "no cache warning in {stderr}");
    assert!(
        stderr.contains(str_of(&blocker)),
        "no blocked directory in {stderr}"
    );
    assert_eq!(read(&blocker), b"blocker", "blocker was replaced");
}

// A cached CLI run keeps its cache in the per-user home and never creates the legacy in-repo
// cache directory in the analyzed project.
#[test]
fn cached_cli_never_creates_repo_sloplint_dir() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    let (warm_args, _) = json_args();
    let first = quiet_stdout(&root, &home, &warm_args);
    let second = quiet_stdout(&root, &home, &warm_args);
    assert!(first == second, "warm run differs from the populating run");
    let created = root.join(".sloplint").exists();
    assert!(
        !created,
        "cached run created a cache directory in the project"
    );
    assert!(
        cache_file(&home).is_file(),
        "no cache file under the isolated home"
    );
}

#[test]
fn library_cache_uses_the_explicit_root() {
    let (_tmp, root) = project();
    let (_cache_tmp, base) = cache_home();
    let xdg_dir = xdg(&base);
    let options = RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        cache_root: Some(xdg_dir),
        use_cache: true,
        jobs: 2,
        ..RunOptions::default()
    };
    let first = shown(&run_ok(&options));
    let second = shown(&run_ok(&options));
    assert_eq!(first, second, "a cache hit must equal the cold library run");
    assert_eq!(
        cache_files(&base).len(),
        1,
        "the explicit root must hold one cache file"
    );
    assert!(
        !root.join(".sloplint").exists(),
        "a cached library run must not write into the project"
    );
}

#[test]
fn added_manifest_restamps_cached_package_keys() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(
        &root,
        "libarea/service.py",
        "def render(name):\n    return name\n",
    );
    put(
        &root,
        "testarea/test_support.py",
        "def test_support():\n    assert True\n",
    );
    let (_cache_tmp, base) = cache_home();
    let cached = || RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![
            RulePattern::parse("agent/colocated-tests").unwrap_or_else(|e| panic!("rule: {e}")),
        ]),
        cache_root: Some(xdg(&base)),
        use_cache: true,
        jobs: 2,
        ..RunOptions::default()
    };
    let cold = || {
        let mut options = cached();
        options.use_cache = false;
        options.cache_root = None;
        run_ok(&options)
    };
    let before = run_ok(&cached());
    assert!(
        !fired(&before, "libarea/service.py", "agent/colocated-tests"),
        "split packages leave the library without evidence in its package: {}",
        shown(&before)
    );
    assert!(
        !cache_files(&base).is_empty(),
        "the first run must populate the cache"
    );
    put(&root, "pyproject.toml", "[project]\nname = \"demo\"\n");
    let united = run_ok(&cached());
    assert!(
        fired(&united, "libarea/service.py", "agent/colocated-tests"),
        "a root python manifest unites the packages so the library gains evidence: {}",
        shown(&united)
    );
    assert_eq!(
        shown(&united),
        shown(&cold()),
        "a cache hit must equal the cold run after the manifest lands"
    );
    put(
        &root,
        "libarea/pyproject.toml",
        "[project]\nname = \"lib\"\n",
    );
    let split = run_ok(&cached());
    assert!(
        !fired(&split, "libarea/service.py", "agent/colocated-tests"),
        "a nearer python manifest splits the library back out so it loses evidence: {}",
        shown(&split)
    );
    assert_eq!(
        shown(&split),
        shown(&cold()),
        "a cache hit must equal the cold run after the split"
    );
}

#[test]
fn setup_py_marks_its_package_and_stays_linted() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(
        &root,
        "pkg/sub/setup.py",
        "from setuptools import setup\n\nsetup(name=\"demo\")\n",
    );
    put(
        &root,
        "pkg/sub/mod.py",
        "def render(name):\n    return name\n",
    );
    put(
        &root,
        "pkg/sub/test_other.py",
        "def test_other():\n    assert True\n",
    );
    let options = || RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![
            RulePattern::parse("agent/colocated-tests").unwrap_or_else(|e| panic!("rule: {e}")),
        ]),
        cache_root: None,
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    };
    let report = run_ok(&options());
    assert!(
        fired(&report, "pkg/sub/mod.py", "agent/colocated-tests"),
        "evidence in the nested package leaves the module uncovered: {}",
        shown(&report)
    );
    assert_eq!(
        report.summary.files,
        3,
        "every file must be analyzed: {}",
        shown(&report)
    );
    std::fs::create_dir_all(root.join("pkg/other")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::fs::rename(
        root.join("pkg/sub/test_other.py"),
        root.join("pkg/other/test_other.py"),
    )
    .unwrap_or_else(|e| panic!("move: {e}"));
    let report = run_ok(&options());
    assert!(
        !fired(&report, "pkg/sub/mod.py", "agent/colocated-tests"),
        "evidence outside the nested package must stay silent: {}",
        shown(&report)
    );
    assert_eq!(
        report.summary.files,
        3,
        "every file must be analyzed: {}",
        shown(&report)
    );
}

#[test]
fn go_packages_ignore_a_root_package_json() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(
        &root,
        "internal/a/a.go",
        "package a\n\nfunc Render(name string) string {\n\treturn name\n}\n",
    );
    put(
        &root,
        "internal/b/b_test.go",
        "package b\n\nimport \"testing\"\n\nfunc TestB(t *testing.T) {\n\tif 1 != 1 {\n\t\tt.Fail()\n\t}\n}\n",
    );
    put(&root, "package.json", "{}\n");
    let (_cache_tmp, base) = cache_home();
    let cached = || RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![
            RulePattern::parse("agent/colocated-tests").unwrap_or_else(|e| panic!("rule: {e}")),
        ]),
        cache_root: Some(xdg(&base)),
        use_cache: true,
        jobs: 2,
        ..RunOptions::default()
    };
    let report = run_ok(&cached());
    assert!(
        !fired(&report, "internal/a/a.go", "agent/colocated-tests"),
        "go packages ignore a javascript manifest so the library stays without evidence: {}",
        shown(&report)
    );
    let mut cold = cached();
    cold.use_cache = false;
    cold.cache_root = None;
    assert_eq!(
        shown(&report),
        shown(&run_ok(&cold)),
        "a cache hit must equal the cold run for go packages"
    );
}

#[test]
fn excluded_manifest_does_not_define_a_package() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "libs/a/setup.py", "from setuptools import setup\n");
    put(
        &root,
        "libs/a/mod.py",
        "def render(name):\n    return name\n",
    );
    put(
        &root,
        "libs/a/tests/test_a.py",
        "def test_a():\n    assert True\n",
    );
    put(&root, "libs/b/mod.py", "def serve():\n    return 1\n");
    put(
        &root,
        "sloplint.toml",
        "[project]\nextend_exclude = [\"**/setup.py\"]\n",
    );
    let options = || RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![
            RulePattern::parse("agent/colocated-tests").unwrap_or_else(|e| panic!("rule: {e}")),
        ]),
        cache_root: None,
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    };
    let excluded = run_ok(&options());
    assert!(
        fired(&excluded, "libs/b/mod.py", "agent/colocated-tests"),
        "an excluded manifest leaves the fallback package united so the library gains evidence: {}",
        shown(&excluded)
    );
    assert_eq!(
        excluded.summary.files,
        3,
        "the excluded setup.py must not be analyzed: {}",
        shown(&excluded)
    );
    std::fs::remove_file(root.join("sloplint.toml"))
        .unwrap_or_else(|e| panic!("remove config: {e}"));
    let included = run_ok(&options());
    assert!(
        !fired(&included, "libs/b/mod.py", "agent/colocated-tests"),
        "a visible manifest scopes its own directory so the library loses evidence: {}",
        shown(&included)
    );
    assert_eq!(
        included.summary.files,
        4,
        "every file must be analyzed: {}",
        shown(&included)
    );
}

#[test]
fn manifests_above_the_root_do_not_count() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let outer = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&outer, "pyproject.toml", "[project]\nname = \"demo\"\n");
    put(
        &outer,
        "inner/libarea/service.py",
        "def render(name):\n    return name\n",
    );
    put(
        &outer,
        "inner/testarea/test_support.py",
        "def test_support():\n    assert True\n",
    );
    let options = |root: std::path::PathBuf| RunOptions {
        cwd: root.clone(),
        root: Some(root),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![
            RulePattern::parse("agent/colocated-tests").unwrap_or_else(|e| panic!("rule: {e}")),
        ]),
        cache_root: None,
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    };
    let whole = run_ok(&options(outer.clone()));
    assert!(
        fired(&whole, "inner/libarea/service.py", "agent/colocated-tests"),
        "a root manifest unites the tree so the library gains evidence: {}",
        shown(&whole)
    );
    let inner = run_ok(&options(outer.join("inner")));
    assert!(
        !fired(&inner, "libarea/service.py", "agent/colocated-tests"),
        "a narrower root misses the manifest above it so the library loses evidence: {}",
        shown(&inner)
    );
}
