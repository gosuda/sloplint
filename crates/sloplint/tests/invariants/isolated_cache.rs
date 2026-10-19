//! Cached runs through the real binary. The per-user cache lives outside the checkout and a
//! library run with the cache on would use the runner's real cache directory, so every cached
//! run here is a child process whose cache and home directories are temp dirs owned by the
//! test. Nothing mutates this process's environment, so tests stay safe to run concurrently.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::{custom_rule, mkdir, tempdir, write};

/// One generic-name finding, with a message that holds `GENERIC_TEXT` exactly once.
const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const GENERIC_TEXT: &str = "shared by unrelated code";

/// Temp dirs standing in for the user's cache and home directories.
struct CacheHome {
    /// Keeps the directories alive for the test's lifetime.
    _tmp: tempfile::TempDir,
    base: PathBuf,
}

impl CacheHome {
    fn new() -> CacheHome {
        let (tmp, base) = tempdir();
        for name in ["xdg", "home", "local"] {
            mkdir(&base.join(name));
        }
        CacheHome { _tmp: tmp, base }
    }

    /// Points the child's per-user cache and home directories at this test's temp dirs.
    fn isolate<'c>(&self, command: &'c mut Command) -> &'c mut Command {
        command
            .env("XDG_CACHE_HOME", self.base.join("xdg"))
            .env("HOME", self.base.join("home"))
            .env("LOCALAPPDATA", self.base.join("local"))
    }

    /// Runs `sloplint check` in `root` and returns whatever the process produced.
    fn output(&self, root: &Path, extra: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sloplint"));
        command
            .current_dir(root)
            .args(["check", "--strictness", "standard", "--fail-on", "never"])
            .args(extra);
        self.isolate(&mut command)
            .output()
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
    }

    /// Runs `sloplint check` in `root` and returns stdout; a stderr warning fails the test.
    fn check(&self, root: &Path, extra: &[&str]) -> String {
        let out = self.output(root, extra);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success() && stderr.is_empty(),
            "sloplint check {extra:?}: {:?}: {stderr}",
            out.status
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Every stored cache file, `<xdg>/sloplint/<key>/cache-v1.bin`.
    fn cache_files(&self) -> Vec<PathBuf> {
        let namespace = self.base.join("xdg").join("sloplint");
        let Ok(keys) = std::fs::read_dir(&namespace) else {
            return Vec::new();
        };
        keys.map(|key| {
            let key = key.unwrap_or_else(|e| panic!("{}: {e}", namespace.display()));
            key.path().join("cache-v1.bin")
        })
        .filter(|file| file.is_file())
        .collect()
    }
}

/// A project holding [`SAMPLE`] and an empty configuration.
fn sample_project() -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(&root, "sample.rs", SAMPLE);
    (tmp, root)
}

/// The stored findings are reused, not recomputed: a same-length change planted inside the
/// stored finding shows up in the next cached run, and only in a cached run.
#[test]
fn warm_cache_reuses_stored_findings() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let planted = GENERIC_TEXT.replace("code", "mode");
    assert!(
        home.check(&root, &[]).contains(GENERIC_TEXT),
        "the cold run lost the generic-name finding"
    );
    let files = home.cache_files();
    assert_eq!(
        files.len(),
        1,
        "one project stores one cache file: {files:?}"
    );
    let mut bytes = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read cache: {e}"));
    let spots: Vec<usize> = bytes
        .windows(GENERIC_TEXT.len())
        .enumerate()
        .filter(|(_, window)| *window == GENERIC_TEXT.as_bytes())
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        spots.len(),
        1,
        "the stored finding must hold its message once"
    );
    bytes[spots[0]..spots[0] + planted.len()].copy_from_slice(planted.as_bytes());
    std::fs::write(&files[0], &bytes).unwrap_or_else(|e| panic!("plant: {e}"));
    let warm = home.check(&root, &[]);
    assert!(
        warm.contains(&planted),
        "the warm run recomputed the finding instead of reusing the stored one: {warm}"
    );
    let uncached = home.check(&root, &["--no-cache"]);
    assert!(
        uncached.contains(GENERIC_TEXT) && !uncached.contains(&planted),
        "an uncached run must report the computed finding: {uncached}"
    );
}

/// A cache populated by a `--rule` run must not hide rules from the next unfiltered run.
#[test]
fn audit_filter_cache_stays_unfiltered() {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(
        &root,
        "sample.rs",
        "pub fn handle(flag: bool, other: bool) {}\n",
    );
    let home = CacheHome::new();
    let filtered = home.check(&root, &["--rule", "grep/generic-name"]);
    assert!(
        filtered.contains("[grep/generic-name]"),
        "the filtered run missed its rule"
    );
    assert!(
        !filtered.contains("[complexity/boolean-params]"),
        "the filter leaked another rule"
    );
    assert_eq!(
        home.cache_files().len(),
        1,
        "the filtered run stored no cache, so the full run would pass cold"
    );
    let full = home.check(&root, &[]);
    assert!(
        full.contains("[grep/generic-name]"),
        "the full run after the filtered cache missed generic-name"
    );
    assert!(
        full.contains("[complexity/boolean-params]"),
        "the filtered cache poisoned the later full run"
    );
}

/// Edits to the configuration and to a custom-rule file must not be served from the cache.
#[test]
fn cache_follows_config_and_custom_rule_file_edits() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(&root, "rules.toml", &custom_rule("custom/first-hit"));
    let config = "[project]\ncustom_rule_files = [\"rules.toml\"]\n";
    write(&root, "sloplint.toml", config);
    let home = CacheHome::new();
    let first = home.check(&root, &[]);
    assert!(
        first.contains("[custom/first-hit]") && first.contains("[grep/generic-name]"),
        "the cold run lost a finding: {first}"
    );
    write(&root, "rules.toml", &custom_rule("custom/second-hit"));
    let second = home.check(&root, &[]);
    assert!(
        second.contains("[custom/second-hit]") && !second.contains("[custom/first-hit]"),
        "the cache served a stale custom-rule file: {second}"
    );
    let disabled = format!("{config}[rules.\"grep/generic-name\"]\nlevel = \"off\"\n");
    write(&root, "sloplint.toml", &disabled);
    let third = home.check(&root, &[]);
    assert!(
        !third.contains("[grep/generic-name]") && third.contains("[custom/second-hit]"),
        "the cache served a stale configuration: {third}"
    );
}

/// A symlink planted at the cache file's own name must neither be read nor written through.
/// A valid cache planted behind the link must not be reused; the run must still succeed with
/// the cold findings. Whether the link is kept or replaced is not part of the contract.
#[cfg(unix)]
#[test]
fn planted_cache_symlink_is_never_written_through() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let cold = home.check(&root, &[]);
    let files = home.cache_files();
    assert_eq!(
        files.len(),
        1,
        "one project stores one cache file: {files:?}"
    );
    let genuine = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read cache: {e}"));
    let planted = GENERIC_TEXT.replace("code", "mode");
    let at = genuine
        .windows(GENERIC_TEXT.len())
        .position(|window| window == GENERIC_TEXT.as_bytes())
        .unwrap_or_else(|| panic!("the finding message is not in the cache bytes"));
    let mut forged = genuine.clone();
    forged[at..at + planted.len()].copy_from_slice(planted.as_bytes());
    let (_outside, base) = tempdir();
    let planted_path = base.join("forged.bin");
    std::fs::write(&planted_path, &forged).unwrap_or_else(|e| panic!("plant: {e}"));
    std::fs::remove_file(&files[0]).unwrap_or_else(|e| panic!("remove cache: {e}"));
    std::os::unix::fs::symlink(&planted_path, &files[0]).unwrap_or_else(|e| panic!("symlink: {e}"));
    let out = home.output(&root, &[]);
    assert!(
        out.status.success(),
        "a planted link failed the run: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        cold,
        "the cache load followed a symlinked cache file"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(&planted),
        "a forged cache behind a symlink was reused: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let after = std::fs::read(&planted_path).unwrap_or_else(|e| panic!("read planted: {e}"));
    assert_eq!(
        after, forged,
        "the cache store wrote through a planted symlink"
    );
}

/// A symlink planted at the temporary file's name must be left alone and never written through.
/// The temporary name carries the child's pid, so a shell plants the link and then `exec`s the
/// binary: the shell's `$$` is the pid the binary keeps.
#[cfg(unix)]
#[test]
fn planted_temp_symlink_is_preserved_and_never_written_through() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let cold = home.check(&root, &[]);
    let files = home.cache_files();
    assert_eq!(
        files.len(),
        1,
        "one project stores one cache file: {files:?}"
    );
    let key_dir = files[0]
        .parent()
        .unwrap_or_else(|| panic!("{} has no directory", files[0].display()))
        .to_path_buf();
    let (_outside, base) = tempdir();
    write(&base, "sentinel.txt", "sentinel");
    let marker = base.join("sentinel.txt");
    let script = r#"ln -s "$1" "$2/cache-v1.bin.$$.tmp" && shift 2 && exec "$@""#;
    let mut command = Command::new("sh");
    command
        .current_dir(&root)
        .args(["-c", script, "sh"])
        .arg(&marker)
        .arg(&key_dir)
        .arg(env!("CARGO_BIN_EXE_sloplint"))
        .args(["check", "--strictness", "standard", "--fail-on", "never"]);
    let out = home
        .isolate(&mut command)
        .output()
        .unwrap_or_else(|e| panic!("spawn sh: {e}"));
    assert!(
        out.status.success(),
        "a planted temp link failed the run: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        cold,
        "a planted temp link changed the findings"
    );
    let planted: Vec<PathBuf> = std::fs::read_dir(&key_dir)
        .unwrap_or_else(|e| panic!("{}: {e}", key_dir.display()))
        .map(|entry| entry.unwrap_or_else(|e| panic!("entry: {e}")).path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("cache-v1.bin.") && name.ends_with(".tmp")
        })
        .collect();
    assert_eq!(
        planted.len(),
        1,
        "the planted temp link must remain: {planted:?}"
    );
    let target = std::fs::read_link(&planted[0])
        .unwrap_or_else(|e| panic!("{} is not a symlink: {e}", planted[0].display()));
    assert_eq!(target, marker, "the planted temp link was replaced");
    let after = std::fs::read_to_string(&marker).unwrap_or_else(|e| panic!("read sentinel: {e}"));
    assert_eq!(
        after, "sentinel",
        "the cache store wrote through a planted temp link"
    );
}
