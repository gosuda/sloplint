//! Unreadable sources and directories, plus counted skips, through the public API and the CLI.

use super::{
    BASELINE, GENERIC, SAMPLE, assert_cli_failure, assert_failed, assert_io, cache_files,
    cache_home, cause_of, cli_in, fired, io_failure, json_args, opts, project, put, quiet_stdout,
    run_ok, str_of, text, tweak,
};
#[cfg(unix)]
use super::{DENIED, assert_denied, lock, lock_dir};
#[cfg(unix)]
use sloplint::run;

// The tree walk runs on every worker thread, so which walk error it reports must not
// depend on scheduling: every thread count names the same directory, and no cache is stored.
#[cfg(unix)]
#[test]
fn unreadable_directories_fail_the_run_with_the_same_error_at_any_job_count() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    put(&root, "a_dir/x.rs", SAMPLE);
    put(&root, "b_dir/y.rs", SAMPLE);
    let (first, second) = (root.join("a_dir"), root.join("b_dir"));
    let guards = [lock_dir(&second), lock_dir(&first)];
    assert!(
        std::fs::read_dir(&first).is_err(),
        "permission bits were bypassed; run these tests as an unprivileged user, not root"
    );
    let mut reported = Vec::new();
    for jobs in ["1", "2", "8", "1", "8"] {
        let mut args = json_args().0;
        args.extend(["--jobs", jobs]);
        let out = cli_in(&root, &home, &args);
        assert_failed(&out, &args, 3, &[str_of(&first)]);
        let stderr = text(&out.stderr);
        assert!(
            !stderr.contains(str_of(&second)),
            "jobs={jobs}: reported the later directory: {stderr}"
        );
        reported.push(stderr);
    }
    reported.dedup();
    assert_eq!(
        reported.len(),
        1,
        "the walk error changed with jobs: {reported:?}"
    );
    let stored = cache_files(&home);
    assert!(stored.is_empty(), "failed runs stored a cache: {stored:?}");
    drop(guards);
    let summary = run_ok(&opts(&root)).summary;
    assert_eq!((summary.files, summary.skipped), (3, 0), "readable again");
}

// Policy skips stay counted, non-failing skips next to the read-error change.
#[test]
fn oversize_and_generated_sources_stay_counted_skips() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nmax_file_bytes = 64\n");
    put(&root, "gen.rs", &format!("// @generated\n{SAMPLE}"));
    put(&root, "big.rs", &SAMPLE.repeat(3));
    let report = run_ok(&opts(&root));
    assert_eq!((report.summary.files, report.summary.skipped), (1, 2));
    assert!(fired(&report, "sample.rs", GENERIC), "sample.rs lost");
}

// AA-S4: an unreadable source is a typed I/O failure with a stable first error, never a skip,
// and a failed cached CLI run stores no partial cache.
#[cfg(unix)]
#[test]
fn unreadable_source_fails_the_run_with_typed_io() {
    let (_tmp, root) = project();
    let (_cache_tmp, home) = cache_home();
    put(&root, "b_denied.rs", SAMPLE);
    put(&root, "a_denied.rs", SAMPLE);
    let (first, second) = (root.join("a_denied.rs"), root.join("b_denied.rs"));
    let guards = [lock(&second, 0), lock(&first, 0)];
    assert_denied(std::fs::File::open(&first), "a_denied.rs");
    for jobs in [1, 2, 8] {
        let options = tweak(&root, |o| o.jobs = jobs);
        let (path, kind) = io_failure(run(&options), "unreadable source");
        assert_eq!(path, first, "jobs={jobs}: first path-ordered error");
        assert_eq!(kind, DENIED, "jobs={jobs}");
    }
    let cause = cause_of(std::fs::read(&first));
    for jobs in ["1", "2", "8"] {
        let mut args = json_args().0;
        args.extend(["--jobs", jobs]);
        let out = cli_in(&root, &home, &args);
        assert_failed(&out, &args, 3, &[str_of(&first), &cause]);
        let stderr = text(&out.stderr);
        let named = stderr.contains(str_of(&second));
        assert!(!named, "jobs={jobs}: reported the later path: {stderr}");
    }
    let stored = cache_files(&home);
    assert!(stored.is_empty(), "failed runs stored a cache: {stored:?}");
    let scoped = tweak(&root, |o| o.paths = vec![root.join("sample.rs")]);
    assert_io(run(&scoped), "outside PATH scope", &first, DENIED);
    drop(guards);
    let summary = run_ok(&opts(&root)).summary;
    assert_eq!((summary.files, summary.skipped), (3, 0), "readable again");
    quiet_stdout(&root, &home, &json_args().0);
    let stored = cache_files(&home);
    assert_eq!(
        stored.len(),
        3,
        "a readable cached run must store one shard per file: {stored:?}"
    );
}

// AA-S4 at the CLI: exit 3 names the path and cause; `baseline` must not record a partial run.
#[cfg(unix)]
#[test]
fn unreadable_source_cli_exits_3_and_writes_no_baseline() {
    let (_tmp, root) = project();
    put(&root, "denied.rs", SAMPLE);
    let file = root.join("denied.rs");
    let _guard = lock(&file, 0);
    assert_denied(std::fs::File::open(&file), "denied.rs");
    let cause = cause_of(std::fs::read(&file));
    let commands: [&[&str]; 2] = [&["check", "--no-cache"], &["baseline", "--no-cache"]];
    for args in commands {
        assert_cli_failure(&root, args, 3, &[str_of(&file), &cause]);
    }
    assert!(!root.join(BASELINE).exists(), "partial baseline written");
}
