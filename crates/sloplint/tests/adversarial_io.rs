//! Source, configuration, cache, and baseline I/O failures through the public API and the CLI.
//!
//! Comments above each test name the audit assumption ids it pins. Permission cases are
//! unix-only and require an unprivileged runner: they assert the denial is actually observed
//! and fail on root instead of passing silently as not applicable. Every cached run goes
//! through the CLI with an isolated per-user cache home (`XDG_CACHE_HOME`, `HOME`, and
//! `LOCALAPPDATA` set on the child only), so no test touches the real cache or the project tree.

use sloplint::{Report, RunError, RunOptions, execute, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Finding;
use std::io::ErrorKind::{self, NotFound};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const GENERIC: &str = "grep/generic-name";
const BASELINE: &str = "sloplint-baseline.json";
const NAMESPACE: &str = "sloplint";
const RATCHET: [&str; 4] = ["check", "--no-cache", "--mode", "ratchet"];
#[cfg(unix)]
const DENIED: ErrorKind = ErrorKind::PermissionDenied;

fn seed(root: &Path) {
    put(root, "sample.rs", SAMPLE);
    put(root, "sloplint.toml", "");
}

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    seed(&root);
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        mkdir(dir);
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn mkdir(path: &Path) {
    std::fs::create_dir_all(path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn str_of(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path {}", path.display()))
}

fn set_baseline(root: &Path, name: &str) {
    let body = format!("[project]\nbaseline = {name:?}\n");
    put(root, "sloplint.toml", &body);
}

fn opts(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn tweak(root: &Path, change: impl FnOnce(&mut RunOptions)) -> RunOptions {
    let mut options = opts(root);
    change(&mut options);
    options
}

fn ratchet(root: &Path) -> RunOptions {
    tweak(root, |o| o.mode = Some(Mode::Ratchet))
}

fn writing(root: &Path) -> RunOptions {
    tweak(root, |o| o.write_baseline = true)
}

fn strict(root: &Path) -> RunOptions {
    tweak(root, |o| o.strictness = Some(Strictness::Strict))
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn shown(report: &Report) -> String {
    format!("{:#?}\n{:?}", report.findings, report.summary)
}

/// Findings, baselined, and stale counts.
fn tally(report: &Report) -> (u32, u32, u32) {
    let found = u32::try_from(report.findings.len()).unwrap_or(u32::MAX);
    (found, report.summary.baselined, report.summary.stale)
}

fn fired(report: &Report, path: &str, rule: &str) -> bool {
    let hit = |f: &Finding| &*f.path == path && &*f.rule == rule;
    report.findings.iter().any(hit)
}

fn cause_of<T: std::fmt::Debug>(result: std::io::Result<T>) -> String {
    result.expect_err("expected an I/O error").to_string()
}

fn failure(result: Result<Report, RunError>, what: &str) -> RunError {
    match result {
        Ok(report) => panic!("{what}: expected failure, got {}", shown(&report)),
        Err(error) => error,
    }
}

fn assert_usage(error: &RunError, variant: &str) {
    let debug = format!("{error:?}");
    assert!(debug.starts_with(variant), "expected {variant}: {debug}");
    assert_eq!(error.exit_code(), 2, "{variant} is a usage error");
}

/// The failure must be `RunError::Io`: exit 3, with text naming the path and the OS cause.
fn io_failure(result: Result<Report, RunError>, what: &str) -> (PathBuf, ErrorKind) {
    let error = failure(result, what);
    assert_eq!(error.exit_code(), 3, "{what}: I/O exits 3");
    let text = error.to_string();
    let RunError::Io { path, source } = error else {
        panic!("{what}: expected RunError::Io, got {text}");
    };
    let cause = source.to_string();
    assert!(text.contains(str_of(&path)), "{what}: no path: {text}");
    assert!(text.contains(&cause), "{what}: no cause: {text}");
    (path, source.kind())
}

fn assert_io(result: Result<Report, RunError>, what: &str, path: &Path, kind: ErrorKind) {
    let found = io_failure(result, what);
    assert_eq!(found, (path.to_path_buf(), kind), "{what}");
}

fn sloplint(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sloplint"));
    command.current_dir(root).args(args);
    command
}

fn spawn(command: &mut Command, args: &[&str]) -> Output {
    let spawned = command.output();
    spawned.unwrap_or_else(|e| panic!("spawn {args:?}: {e}"))
}

/// A CLI run that does no cache work: every caller passes `--no-cache`.
fn cli(root: &Path, args: &[&str]) -> Output {
    let mut command = sloplint(root, args);
    spawn(&mut command, args)
}

/// A CLI run in an isolated per-user cache home: the variables are set on the child only, so
/// the process environment is never mutated and concurrent tests cannot see each other.
fn cli_in(root: &Path, cache_home: &Path, args: &[&str]) -> Output {
    let mut command = sloplint(root, args);
    command.env("XDG_CACHE_HOME", xdg(cache_home));
    command.env("HOME", cache_home.join("home"));
    command.env("LOCALAPPDATA", cache_home.join("local"));
    spawn(&mut command, args)
}

/// An isolated per-user cache home: a fresh directory holding the `xdg` directory used as
/// `XDG_CACHE_HOME`, plus empty `home` and `local` directories for `HOME` and `LOCALAPPDATA`.
/// The guard must outlive every run that uses the path.
fn cache_home() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let base = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    for name in ["xdg", "home", "local"] {
        mkdir(&base.join(name));
    }
    (tmp, base)
}

/// The `XDG_CACHE_HOME` directory of an isolated cache home.
fn xdg(cache_home: &Path) -> PathBuf {
    cache_home.join("xdg")
}

/// The per-user `sloplint` directory that holds one subdirectory per analyzed project.
fn namespace(cache_home: &Path) -> PathBuf {
    xdg(cache_home).join(NAMESPACE)
}

/// Whether a cache directory entry is a sharded `cache-v2/shard-<NN>.bin` file.
fn is_cache_file_name(name: &str) -> bool {
    name.len() == "shard-00.bin".len()
        && name.starts_with("shard-")
        && name.ends_with(".bin")
        && name[6..8]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Every shard file stored under `<xdg>/sloplint/<key>/cache-v2/`.
fn cache_files(home: &Path) -> Vec<PathBuf> {
    let Ok(keys) = std::fs::read_dir(namespace(home)) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for key in keys.filter_map(Result::ok) {
        let Ok(entries) = std::fs::read_dir(key.path().join("cache-v2")) else {
            continue;
        };
        files.extend(
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(is_cache_file_name)
                        && path.is_file()
                }),
        );
    }
    files
}

/// The one cache file a run for a single project leaves in `home`.
fn cache_file(home: &Path) -> PathBuf {
    let mut found = cache_files(home);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one cache file under {}: {found:?}",
        home.display()
    );
    found.swap_remove(0)
}

/// The per-project directory that holds `cache_file(home)`: the grandparent of
/// its shard (`<key>/cache-v2/shard-NN.bin`).
#[cfg(unix)]
fn key_dir(home: &Path) -> PathBuf {
    let file = cache_file(home);
    file.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| panic!("{} has no project directory", file.display()))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The CLI must exit `code`, keep stdout empty, and put every needle in its error line.
fn assert_cli_failure(root: &Path, args: &[&str], code: i32, needles: &[&str]) {
    assert_failed(&cli(root, args), args, code, needles);
}

/// `out`, produced by `args`, must be a failure with the given exit code and error needles.
fn assert_failed(out: &Output, args: &[&str], code: i32, needles: &[&str]) {
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(code), "{args:?}: {stderr}");
    assert!(stdout.is_empty(), "{args:?}: stdout {stdout}");
    assert!(stderr.starts_with("sloplint: error: "), "{stderr}");
    for needle in needles {
        assert!(stderr.contains(needle), "no {needle:?} in {stderr}");
    }
}

/// Arguments of a JSON `check` at `strictness`: with the cache, and without it.
fn profile_args(strictness: &'static str) -> (Vec<&'static str>, Vec<&'static str>) {
    let fixed = [
        "check",
        "--format",
        "json",
        "--strictness",
        strictness,
        "--fail-on",
        "never",
    ];
    (fixed.to_vec(), [&fixed[..], &["--no-cache"]].concat())
}

/// Arguments of a JSON `check` at the standard profile: with the cache, and without it.
fn json_args() -> (Vec<&'static str>, Vec<&'static str>) {
    profile_args("standard")
}

/// The stdout of a CLI run that must succeed without a word on stderr.
fn quiet_stdout(root: &Path, cache_home: &Path, args: &[&str]) -> Vec<u8> {
    let out = cli_in(root, cache_home, args);
    let stderr = text(&out.stderr);
    assert!(
        out.status.success() && stderr.is_empty(),
        "{args:?}: {stderr}"
    );
    out.stdout
}

/// Puts `0o644` back on drop, so cleanup works even after a failed assertion.
#[cfg(unix)]
struct Restore(PathBuf);

#[cfg(unix)]
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o644));
    }
}

#[cfg(unix)]
fn lock(path: &Path, mode: u32) -> Restore {
    let permissions = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(path, permissions).unwrap_or_else(|e| panic!("chmod: {e}"));
    Restore(path.to_path_buf())
}

/// Assert the open was denied. Permission bits are bypassed for privileged users, so a
/// successful open means the test is running as root and must fail: these cases require an
/// unprivileged runner instead of passing silently as not applicable.
#[cfg(unix)]
fn assert_denied(opened: std::io::Result<std::fs::File>, what: &str) {
    match opened {
        Err(error) if error.kind() == DENIED => {}
        Err(error) => panic!("{what}: expected PermissionDenied, got {error}"),
        Ok(_) => panic!(
            "{what}: permission bits were bypassed; run these tests as an unprivileged user, not root"
        ),
    }
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

// AA-I4: a directory as explicit or implicit config is I/O (exit 3), not "missing" (exit 2).
#[test]
fn config_directory_is_io_exit_3() {
    let (_tmp, root) = project();
    let dir = root.join("cfgdir");
    mkdir(&dir);
    let explicit = tweak(&root, |o| o.config = Some(dir.clone()));
    let (path, kind) = io_failure(run(&explicit), "explicit config directory");
    assert_eq!(path, dir);
    assert_ne!(kind, NotFound, "a directory is not missing");
    let cause = cause_of(std::fs::read_to_string(&dir));
    let args = ["check", "--no-cache", "--config", str_of(&dir)];
    assert_cli_failure(&root, &args, 3, &[str_of(&dir), &cause]);
    let implicit = root.join("sloplint.toml");
    std::fs::remove_file(&implicit).unwrap_or_else(|e| panic!("remove: {e}"));
    mkdir(&implicit);
    let (path, kind) = io_failure(run(&opts(&root)), "implicit config directory");
    assert_eq!(path, implicit);
    assert_ne!(kind, NotFound, "no fallback to defaults");
}

// AA-I4: an unreadable config file is I/O (exit 3) with its path and cause.
#[cfg(unix)]
#[test]
fn unreadable_config_file_is_io_exit_3() {
    let (_tmp, root) = project();
    put(&root, "locked.toml", "");
    let (explicit, implicit) = (root.join("locked.toml"), root.join("sloplint.toml"));
    let _guards = [lock(&explicit, 0), lock(&implicit, 0)];
    assert_denied(std::fs::File::open(&explicit), "locked.toml");
    let given = tweak(&root, |o| o.config = Some(explicit.clone()));
    assert_io(run(&given), "explicit config", &explicit, DENIED);
    assert_io(run(&opts(&root)), "implicit config", &implicit, DENIED);
}

// AA-I10 gaps: empty and dot baseline names, and a directory as the destination.
#[test]
fn baseline_empty_dot_and_directory_names_are_refused_or_io() {
    let (_tmp, root) = project();
    set_baseline(&root, "");
    for options in [writing(&root), ratchet(&root)] {
        let error = failure(run(&options), "empty baseline name");
        assert_usage(&error, "BaselineOutsideRoot");
        let message = error.to_string();
        assert!(message.contains("outside the project root"), "{message}");
    }
    mkdir(&root.join("sub"));
    for name in [".", "sub/.."] {
        set_baseline(&root, name);
        for options in [writing(&root), ratchet(&root)] {
            let (path, kind) = io_failure(run(&options), name);
            assert_eq!(path, root, "{name} resolves to the root itself");
            assert_ne!(kind, NotFound, "{name}");
        }
        assert!(root.join("sample.rs").is_file(), "{name}: root damaged");
    }
    let dir = root.join("bdir");
    mkdir(&dir);
    set_baseline(&root, "bdir");
    for (options, what) in [(writing(&root), "write"), (ratchet(&root), "ratchet")] {
        let (path, kind) = io_failure(run(&options), what);
        assert_eq!(path, dir, "{what}");
        assert_ne!(kind, NotFound, "{what}: not a missing baseline");
    }
    let left = std::fs::read_dir(&dir).map(|d| d.count());
    assert_eq!(left.ok(), Some(0), "directory was touched");
    let cause = cause_of(std::fs::write(&dir, ""));
    let args = ["baseline", "--no-cache"];
    assert_cli_failure(&root, &args, 3, &[str_of(&dir), &cause]);
}

// AA-S12: a missing leaf is BaselineMissing (exit 2, hint); a missing parent is Io (exit 3).
#[test]
fn baseline_missing_leaf_and_parent_are_distinct_failures() {
    let (_tmp, root) = project();
    let error = failure(run(&ratchet(&root)), "missing baseline");
    assert_usage(&error, "BaselineMissing");
    let (hint, leaf) = ("run 'sloplint baseline'", root.join(BASELINE));
    assert!(error.to_string().contains(hint), "{error}");
    assert!(error.to_string().contains(str_of(&leaf)), "{error}");
    set_baseline(&root, "missing-dir/b.json");
    let parent = root.join("missing-dir");
    for (options, what) in [(ratchet(&root), "ratchet"), (writing(&root), "write")] {
        assert_io(run(&options), what, &parent, NotFound);
    }
    assert!(!parent.exists(), "a failed write created the parent");
    assert_cli_failure(&root, &RATCHET, 3, &[str_of(&parent)]);
}

fn entries(report: &Report) -> String {
    let each = |f: &Finding| {
        let (p, r, h) = (&f.path, &f.rule, &f.fingerprint);
        format!(r#"{{"path":"{p}","rule":"{r}","fingerprint":"{h}"}}"#)
    };
    let parts: Vec<String> = report.findings.iter().map(each).collect();
    parts.join(",")
}

fn baseline_doc(version: &str, entries: &str, extra: &str) -> String {
    format!(r#"{{"version":{version},"findings":[{entries}]{extra}}}"#)
}

/// Ratchet against `body` must fail as `BaselineCorrupt` with `needle`, leaving the file alone.
fn corrupt(root: &Path, what: &str, body: &str, needle: &str) {
    put(root, BASELINE, body);
    let error = failure(run(&ratchet(root)), what);
    assert_usage(&error, "BaselineCorrupt");
    let (shown, file) = (error.to_string(), root.join(BASELINE));
    assert!(shown.contains(needle), "{what}: {shown}");
    assert!(shown.contains(str_of(&file)), "{what}: {shown}");
    assert!(read(&file) == body.as_bytes(), "{what}: modified");
}

// AA-S11: corrupt, truncated, and unsupported-version baselines fail closed with exit 2.
#[test]
fn baseline_corruption_fails_closed_with_exit_2() {
    let (_tmp, root) = project();
    let live = run_ok(&opts(&root));
    let n = tally(&live).0;
    assert!(n > 0, "the sample must produce findings");
    let (real, bad) = (entries(&live), "invalid baseline JSON");
    let good = baseline_doc("1", &real, "");
    let partial = r#"{"version":1,"findings":[{"path":"a.rs","rule":"x/y"}]}"#;
    corrupt(&root, "not json", "{not json", bad);
    corrupt(&root, "empty file", "", bad);
    corrupt(&root, "truncated", &good[..good.len() / 2], bad);
    corrupt(&root, "no findings key", r#"{"version":1}"#, bad);
    corrupt(&root, "no fingerprint", partial, bad);
    corrupt(&root, "v0", &baseline_doc("0", &real, ""), "version 0");
    corrupt(&root, "v2", &baseline_doc("2", &real, ""), "version 2");
    assert_cli_failure(&root, &RATCHET, 2, &["version 2"]);
    put(&root, BASELINE, &baseline_doc("1", "", ""));
    let open = run_ok(&ratchet(&root));
    assert_eq!(tally(&open), (n, 0, 0), "version 1 with no entries");
}

// AA-S9, AA-S10, AA-X10: duplicates collapse, unknown rules go stale, extra fields are ignored.
#[test]
fn baseline_duplicates_unknown_rules_and_extra_fields() {
    let (_tmp, root) = project();
    let live = run_ok(&opts(&root));
    let n = tally(&live).0;
    assert!(n > 0, "the sample must produce findings");
    let real = entries(&live);
    let unknown = r#"{"path":"sample.rs","rule":"no/such-rule","fingerprint":"0000"}"#;
    let gate = |doc: String| {
        put(&root, BASELINE, &doc);
        run_ok(&ratchet(&root))
    };
    let tripled = gate(baseline_doc("1", &format!("{real},{real},{real}"), ""));
    assert_eq!(tally(&tripled), (0, n, 0), "duplicates inflated counts");
    let stale = gate(baseline_doc("1", &format!("{real},{unknown}"), ""));
    assert_eq!(tally(&stale), (0, n, 1), "unknown rule is stale");
    let execution = execute(&ratchet(&root)).unwrap_or_else(|e| panic!("execute: {e}"));
    assert!(!execution.failed, "stale entries must not fail the gate");
    let noted = real.replace('}', r#","note":"hand edit"}"#);
    let extra = gate(baseline_doc("1", &noted, r#","generator":"hand","x":[1]"#));
    assert_eq!(tally(&extra), (0, n, 0), "extra fields are ignored");
}

/// Malformed caches derived from a valid one: a version byte, 32 key bytes, then a
/// varint-prefixed entry sequence.
fn corrupt_caches(valid: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    assert!(valid.len() > 33 && valid[0] == 2, "layout changed");
    let after_header = |tail: &[u8]| [&valid[..33], tail].concat();
    let mut max_len = vec![0xff; 9];
    max_len.push(1);
    let mut entry = vec![1u8];
    entry.extend(&max_len);
    let with = |index: usize, byte: u8| {
        let mut bytes = valid.to_vec();
        bytes[index] = byte;
        bytes
    };
    vec![
        ("empty file", Vec::new()),
        ("garbage text", b"not a postcard cache".to_vec()),
        ("truncated", valid[..valid.len() / 2].to_vec()),
        ("entry count u64::MAX", after_header(&max_len)),
        ("path length u64::MAX", after_header(&entry)),
        ("version 0", with(0, 0)),
        ("version 1", with(0, 1)),
        ("flipped key byte", with(1, !valid[1])),
    ]
}

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

/// Moves the real directory at `link` (somewhere under `home`) to the same relative place
/// under `outside` and leaves a symlink to it behind.
#[cfg(unix)]
fn relocate_behind_link(home: &Path, outside: &Path, link: &Path) {
    let relative = link
        .strip_prefix(home)
        .unwrap_or_else(|e| panic!("{} not in {}: {e}", link.display(), home.display()));
    let moved = outside.join(relative);
    mkdir(moved.parent().unwrap_or(outside));
    std::fs::rename(link, &moved).unwrap_or_else(|e| panic!("move: {e}"));
    std::os::unix::fs::symlink(&moved, link).unwrap_or_else(|e| panic!("symlink: {e}"));
}

/// Every file under `dir` with its bytes, sorted: what a refused cache write must leave alone.
#[cfg(unix)]
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let (mut pending, mut files) = (vec![dir.to_path_buf()], Vec::new());
    while let Some(next) = pending.pop() {
        if next.is_dir() {
            let listing = std::fs::read_dir(&next);
            let entries = listing.unwrap_or_else(|e| panic!("read_dir {}: {e}", next.display()));
            pending.extend(entries.filter_map(Result::ok).map(|entry| entry.path()));
        } else {
            let bytes = read(&next);
            files.push((next, bytes));
        }
    }
    files.sort();
    files
}

/// A cached CLI run whose cache directory is the symlink `link` into `outside` must refuse it:
/// exit 0 with the uncached run's stdout, a stderr warning that the cache is disabled by a
/// symlink and names the link, the link kept, and every file under `outside` untouched.
#[cfg(unix)]
fn assert_symlink_refused(root: &Path, home: &Path, link: &Path, outside: &Path) {
    let (warm_args, cold_args) = json_args();
    let expected = cli_in(root, home, &cold_args);
    let before = snapshot(outside);
    assert!(!before.is_empty(), "no foreign cache planted outside");
    let out = cli_in(root, home, &warm_args);
    let (same, stderr) = (out.stdout == expected.stdout, text(&out.stderr));
    assert!(out.status.success() && same, "{stderr}");
    assert!(
        text(&out.stdout).contains(GENERIC),
        "findings lost: {stderr}"
    );
    let warning = "sloplint: warning: cache disabled: cache directory ";
    assert!(
        stderr.contains(warning),
        "no disabled-cache warning in {stderr}"
    );
    assert!(stderr.contains(str_of(link)), "no link name in {stderr}");
    assert!(stderr.contains("symlink"), "no symlink reason in {stderr}");
    assert!(snapshot(outside) == before, "run changed files outside");
    let meta = std::fs::symlink_metadata(link).unwrap_or_else(|e| panic!("lstat: {e}"));
    assert!(meta.file_type().is_symlink(), "link was replaced");
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
