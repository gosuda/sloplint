//! The cache lives in the per-user cache directory; a repository's own `.sloplint` is never trusted.
//!
//! A030: the cache key is computable from public inputs (a released binary plus the repository's
//! own configuration), so a cache file force-added to a checkout could supply any findings for
//! the files it names, including none, and CI checks out that tree. A062: `--no-cache` still
//! hashed the whole executable and could fail on it. Every case runs the real binary with its
//! cache environment set per child; nothing here mutates the test process environment.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const GENERIC: &str = "grep/generic-name";
const CHECK: [&str; 3] = ["check", "--fail-on", "never"];
const UNCACHED: [&str; 4] = ["check", "--fail-on", "never", "--no-cache"];
const CACHE_ENV: [&str; 3] = ["XDG_CACHE_HOME", "HOME", "LOCALAPPDATA"];

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", "");
    put(&root, "sample.rs", SAMPLE);
    (tmp, root)
}

fn temp() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let dir = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    (tmp, dir)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn command(program: &OsStr, cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.current_dir(cwd).args(args);
    command
}

fn spawn(mut command: Command) -> Output {
    command
        .output()
        .unwrap_or_else(|e| panic!("spawn {command:?}: {e}"))
}

/// Runs the binary with every cache location variable pointing at `home`.
fn isolated(root: &Path, args: &[&str], home: &Path) -> Output {
    let mut run = command(env!("CARGO_BIN_EXE_sloplint").as_ref(), root, args);
    for name in CACHE_ENV {
        run.env(name, home);
    }
    spawn(run)
}

/// Runs the binary with no cache location variable at all.
fn homeless(root: &Path, args: &[&str]) -> Output {
    let mut run = command(env!("CARGO_BIN_EXE_sloplint").as_ref(), root, args);
    for name in CACHE_ENV {
        run.env_remove(name);
    }
    spawn(run)
}

fn cache_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(cache_files(&path));
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_cache_file_name)
        {
            found.push(path);
        }
    }
    found
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

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("entry: {e}"))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();
    found
}

fn succeeded_with_finding(out: &Output) {
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains(GENERIC),
        "the lint itself must still run: {stdout}"
    );
}

#[test]
fn the_cache_lives_in_the_user_directory_and_never_in_the_checkout() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();

    let cold = isolated(&root, &CHECK, &home);
    let warm = isolated(&root, &CHECK, &home);

    assert!(
        cold.status.success() && warm.status.success(),
        "both cached runs must succeed; cold stderr {:?}, warm stderr {:?}",
        text(&cold.stderr),
        text(&warm.stderr)
    );
    assert!(cold.stderr.is_empty(), "stderr: {}", text(&cold.stderr));
    assert_eq!(
        cold.stdout, warm.stdout,
        "a cache hit must equal the cold run"
    );
    let files = cache_files(&home);
    assert_eq!(files.len(), 1, "one cache for one project root: {files:?}");
    let relative: Vec<String> = files[0]
        .strip_prefix(&home)
        .unwrap_or_else(|e| panic!("cache outside the home: {e}"))
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        relative.len(),
        4,
        "the cache path is <namespace>/<project key>/<shard dir>/<file>: {relative:?}"
    );
    assert_eq!(
        relative[0], "sloplint",
        "the namespace directory is named after the tool: {relative:?}"
    );
    assert!(
        relative[1].len() == 32
            && relative[1]
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "the project directory is 32 lowercase hex characters: {relative:?}"
    );
    assert_eq!(
        relative[2], "cache-v2",
        "the shards live in a cache-v2 directory: {relative:?}"
    );
    assert_eq!(
        relative[3], "shard-cb.bin",
        "sample.rs hashes to shard 0xcb: {relative:?}"
    );
    assert!(
        !root.join(".sloplint").exists(),
        "the checkout must stay clean"
    );
    assert_eq!(
        names(&root),
        ["sample.rs", "sloplint.toml"],
        "a cached run must add nothing to the project root"
    );
}

#[cfg(unix)]
#[test]
fn the_cache_directories_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    isolated(&root, &CHECK, &home);

    let shards = cache_files(&home)[0]
        .parent()
        .unwrap_or_else(|| panic!("cache file without a directory"))
        .to_path_buf();
    let project_dir = shards
        .parent()
        .unwrap_or_else(|| panic!("shard directory without a project directory"))
        .to_path_buf();
    let namespace = project_dir
        .parent()
        .unwrap_or_else(|| panic!("project directory without a namespace"))
        .to_path_buf();
    for dir in [&shards, &project_dir, &namespace] {
        let mode = std::fs::metadata(dir)
            .unwrap_or_else(|e| panic!("stat {}: {e}", dir.display()))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "{} is not private", dir.display());
    }
}

#[test]
fn a_cache_shipped_in_the_checkout_is_not_trusted() {
    let (_tmp, root) = project();
    let (_first_guard, first_home) = temp();
    let first = isolated(&root, &CHECK, &first_home);
    let genuine =
        std::fs::read(&cache_files(&first_home)[0]).unwrap_or_else(|e| panic!("read: {e}"));

    let needle = b"unrelated code";
    let forged_text = b"FORGED_OUTPUT!";
    assert_eq!(
        needle.len(),
        forged_text.len(),
        "the forged text must keep the postcard length prefix valid"
    );
    let at = genuine
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap_or_else(|| panic!("the finding message is not in the cache bytes"));
    let mut forged = genuine.clone();
    forged[at..at + needle.len()].copy_from_slice(forged_text);
    std::fs::create_dir_all(root.join(".sloplint")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::fs::write(root.join(".sloplint/cache-v1.bin"), &forged)
        .unwrap_or_else(|e| panic!("plant: {e}"));

    let (_second_guard, second_home) = temp();
    let second = isolated(&root, &CHECK, &second_home);

    assert!(second.status.success(), "stderr: {}", text(&second.stderr));
    assert!(
        !text(&second.stdout).contains("FORGED_OUTPUT!"),
        "the repository's cache was trusted: {}",
        text(&second.stdout)
    );
    assert_eq!(
        second.stdout, first.stdout,
        "an untrusted checkout cache must not change the output"
    );
    assert_eq!(
        std::fs::read(root.join(".sloplint/cache-v1.bin")).unwrap_or_default(),
        forged,
        "the planted file must be neither read nor rewritten"
    );
    assert_eq!(
        names(&root.join(".sloplint")),
        ["cache-v1.bin"],
        "nothing else may be created inside the checkout's .sloplint"
    );
}

#[test]
fn a_cache_directory_inside_the_project_root_disables_the_cache() {
    let (_tmp, root) = project();
    let inside = root.join("cachehome");

    let out = isolated(&root, &CHECK, &inside);

    succeeded_with_finding(&out);
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("cache disabled") && stderr.contains("inside the project root"),
        "stderr: {stderr}"
    );
    assert!(
        !inside.exists(),
        "nothing may be created inside the checkout"
    );
}

#[test]
fn without_any_user_directory_the_cache_is_disabled_with_a_warning() {
    let (_tmp, root) = project();

    let out = homeless(&root, &CHECK);

    succeeded_with_finding(&out);
    assert!(
        text(&out.stderr).contains("no per-user cache directory"),
        "stderr: {}",
        text(&out.stderr)
    );
    assert!(
        !root.join(".sloplint").exists(),
        "a disabled cache must not fall back to the checkout"
    );
}

#[test]
fn a_relative_xdg_cache_home_is_ignored() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    let mut run = command(env!("CARGO_BIN_EXE_sloplint").as_ref(), &root, &CHECK);
    run.env("XDG_CACHE_HOME", "relative-cache")
        .env("HOME", &home)
        .env("LOCALAPPDATA", &home);

    let out = spawn(run);

    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    assert!(
        !root.join("relative-cache").exists(),
        "a relative path must not be honored"
    );
    assert_eq!(
        cache_files(&home).len(),
        1,
        "the platform default applies instead"
    );
}

#[test]
fn the_no_cache_flag_does_no_cache_work() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    let never = home.join("never-created");

    let isolated_run = isolated(&root, &UNCACHED, &never);
    succeeded_with_finding(&isolated_run);
    assert!(
        isolated_run.stderr.is_empty(),
        "stderr: {}",
        text(&isolated_run.stderr)
    );
    assert!(
        !never.exists(),
        "--no-cache must not even resolve a cache directory"
    );

    let homeless_run = homeless(&root, &UNCACHED);
    succeeded_with_finding(&homeless_run);
    assert!(
        homeless_run.stderr.is_empty(),
        "a missing home is irrelevant without a cache: {}",
        text(&homeless_run.stderr)
    );
}

#[cfg(unix)]
#[test]
fn a_stale_temp_file_at_this_pid_does_not_block_the_store() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    let cold = isolated(&root, &CHECK, &home);
    succeeded_with_finding(&cold);
    let files = cache_files(&home);
    assert_eq!(files.len(), 1, "one cache for one project root: {files:?}");
    let shard_dir = files[0]
        .parent()
        .unwrap_or_else(|| panic!("{} has no directory", files[0].display()))
        .to_path_buf();
    let shard = files[0]
        .file_name()
        .unwrap_or_else(|| panic!("{} has no file name", files[0].display()))
        .to_string_lossy()
        .into_owned();
    std::fs::remove_file(&files[0]).unwrap_or_else(|e| panic!("remove cache: {e}"));

    let script = r#"printf stale > "$1/$2.$$.tmp" && shift 2 && exec "$@""#;
    let mut run = Command::new("sh");
    run.current_dir(&root)
        .args(["-c", script, "sh"])
        .arg(&shard_dir)
        .arg(&shard)
        .arg(env!("CARGO_BIN_EXE_sloplint"))
        .args(CHECK);
    for name in CACHE_ENV {
        run.env(name, &home);
    }
    let out = spawn(run);

    succeeded_with_finding(&out);
    assert!(
        out.stderr.is_empty(),
        "a stale temp file must not block the store: {}",
        text(&out.stderr)
    );
    assert_eq!(
        text(&out.stdout),
        text(&cold.stdout),
        "a recovered store must not change the findings"
    );
    assert_eq!(
        names(&shard_dir),
        [shard],
        "the stale temp file must be removed and the cache written"
    );
}

#[cfg(unix)]
#[path = "cache_unix.rs"]
mod unix_only;

#[path = "cache_shards.rs"]
mod shards;
