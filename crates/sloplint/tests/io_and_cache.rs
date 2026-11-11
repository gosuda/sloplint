//! Source, configuration, cache, and baseline I/O failures through the public API and the CLI.
//!
//! Comments above each test name the audit assumption ids it pins. Permission cases are
//! unix-only and require an unprivileged runner: they assert the denial is actually observed
//! and fail on root instead of passing silently as not applicable. Every cached run goes
//! through the CLI with an isolated per-user cache home (`XDG_CACHE_HOME`, `HOME`, and
//! `LOCALAPPDATA` set on the child only), so no test touches the real cache or the project tree.

#[path = "io_and_cache/baseline.rs"]
mod baseline;
#[path = "io_and_cache/cache.rs"]
mod cache;
#[path = "io_and_cache/config_io.rs"]
mod config_io;
#[path = "io_and_cache/source_io.rs"]
mod source_io;

use sloplint::{Report, RunError, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Finding;
use std::io::ErrorKind;
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
        cache_root: None,
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

/// Puts the given mode back on drop, so cleanup works even after a failed assertion.
#[cfg(unix)]
struct Restore(PathBuf, u32);

#[cfg(unix)]
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(self.1));
    }
}

/// Sets `mode` on a file; the guard restores `0o644`.
#[cfg(unix)]
fn lock(path: &Path, mode: u32) -> Restore {
    chmod_guarded(path, mode, 0o644)
}

/// Makes a directory unreadable; the guard restores `0o755`.
#[cfg(unix)]
fn lock_dir(path: &Path) -> Restore {
    chmod_guarded(path, 0, 0o755)
}

#[cfg(unix)]
fn chmod_guarded(path: &Path, mode: u32, restore: u32) -> Restore {
    let permissions = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(path, permissions).unwrap_or_else(|e| panic!("chmod: {e}"));
    Restore(path.to_path_buf(), restore)
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

/// Malformed caches derived from a valid one: a 32-byte BLAKE3 digest, then a payload of a version byte, 32 key bytes, and a varint-prefixed entry sequence.
fn corrupt_caches(valid: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    assert!(valid.len() > 65 && valid[32] == 8, "layout changed");
    let payload = &valid[32..];
    let signed = |mut bytes: Vec<u8>| {
        let mut shard = blake3::hash(&bytes).as_bytes().to_vec();
        shard.append(&mut bytes);
        shard
    };
    let after_header = |tail: &[u8]| [&payload[..33], tail].concat();
    let mut max_len = vec![0xff; 9];
    max_len.push(1);
    let mut entry = vec![1u8];
    entry.extend(&max_len);
    let with_payload = |index: usize, byte: u8| {
        let mut bytes = payload.to_vec();
        bytes[index] = byte;
        signed(bytes)
    };
    let with_digest = |index: usize, byte: u8| {
        let mut bytes = valid.to_vec();
        bytes[index] = byte;
        bytes
    };
    vec![
        ("empty file", Vec::new()),
        ("shorter than a digest", valid[..10].to_vec()),
        ("flipped digest byte", with_digest(0, !valid[0])),
        ("garbage payload", signed(b"not a postcard cache".to_vec())),
        (
            "truncated payload",
            signed(payload[..payload.len() / 2].to_vec()),
        ),
        ("entry count u64::MAX", signed(after_header(&max_len))),
        ("path length u64::MAX", signed(after_header(&entry))),
        ("version 0", with_payload(0, 0)),
        ("version 1", with_payload(0, 1)),
        ("flipped key byte", with_payload(1, !payload[1])),
    ]
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
