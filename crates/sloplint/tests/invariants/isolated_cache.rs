//! Cached runs through the real binary. The per-user cache lives outside the checkout and a
//! library run with the cache on would use the runner's real cache directory, so every cached
//! run here is a child process whose cache and home directories are temp dirs owned by the
//! test. Nothing mutates this process's environment, so tests stay safe to run concurrently.

use std::io;
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
        self.output_of(Path::new(env!("CARGO_BIN_EXE_sloplint")), root, extra)
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
    }

    /// Runs the executable `exe` like [`CacheHome::output`]; the spawn error is returned.
    fn output_of(&self, exe: &Path, root: &Path, extra: &[&str]) -> io::Result<Output> {
        let mut command = Command::new(exe);
        command
            .current_dir(root)
            .args(["check", "--strictness", "standard", "--fail-on", "never"])
            .args(extra);
        self.isolate(&mut command).output()
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

/// C++ that fails as C but parses as C++. Its forwarding method makes a finding that only the
/// C++ analysis can report.
const CPP_HEADER: &str = "namespace ui {

template <typename T>
class Stack {
public:
    void push(const T& value) { items_.push_back(value); }
private:
    std::vector<T> items_;
};

}  // namespace ui
";

/// Plain C that parses as C.
const C_HEADER: &str = "#include <string.h>

unsigned long get_length(const char *s) {
    return strlen(s);
}
";

/// Objective-C: it fails as C and as C++, and `NSLog` is a log call in the `objc` table only.
const OBJC_HEADER: &str = "@interface Counter : NSObject
- (void)increment;
@end

static inline void LogCount(NSString *name) {
    NSLog(name);
}
";

/// Invalid in C (line 1) and in C++ (line 3).
const BROKEN_HEADER: &str = "template <typename T>
struct Box { T value; };
int x = ;
";

/// The stored text of the C++ header's finding, and a same-length stand-in for it.
const CPP_FINDING: &str = "only forwards its parameters to `items_.push_back`";

const CPP_PLANTED: &str = "only forwards its parameters to `items_.push_bacm`";

/// Rewrites the one stored occurrence of `from` in the cache file with the same-length `to`.
fn plant(file: &Path, from: &str, to: &str) {
    assert_eq!(from.len(), to.len(), "a plant must keep the stored length");
    let mut bytes = std::fs::read(file).unwrap_or_else(|e| panic!("read cache: {e}"));
    let spots: Vec<usize> = bytes
        .windows(from.len())
        .enumerate()
        .filter(|(_, window)| *window == from.as_bytes())
        .map(|(at, _)| at)
        .collect();
    assert_eq!(spots.len(), 1, "the cache must hold {from:?} exactly once");
    bytes[spots[0]..spots[0] + to.len()].copy_from_slice(to.as_bytes());
    std::fs::write(file, &bytes).unwrap_or_else(|e| panic!("plant: {e}"));
}

/// Runs `spawn`, retrying while the freshly written executable is busy. Another test thread
/// that forks while this test holds the file open for writing keeps a copy of that descriptor
/// until its own `exec`, and `exec` of a file open for writing fails with `ETXTBSY` (os error
/// 26). The retry is bounded to 5 s, so a genuinely busy binary still fails the test; any other
/// error fails it at once.
fn spawn_when_not_busy(spawn: impl Fn() -> io::Result<Output>) -> Output {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut last_busy = None;
    while std::time::Instant::now() < deadline {
        match spawn() {
            Ok(out) => return out,
            Err(e) if e.raw_os_error() == Some(26) => {
                last_busy = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => panic!("spawn altered executable: {e}"),
        }
    }
    panic!(
        "spawn stayed ETXTBSY for 5 s: {}",
        last_busy.map_or("unknown".to_string(), |e| e.to_string())
    )
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
/// binary: the shell's `$$` is the pid the binary keeps. The stored cache is removed first: a
/// warm hit stores nothing, so only a run that stores reaches the temporary file.
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
    std::fs::remove_file(&files[0]).unwrap_or_else(|e| panic!("remove cache: {e}"));
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

/// One `.h` path whose bytes move between plain C, C++, and invalid input. Every cached run
/// must equal an uncached run of the same bytes, so an entry stored for one outcome is never
/// served for another.
#[test]
fn cache_follows_a_header_across_the_cpp_fallback() {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    let home = CacheHome::new();
    let mut stored = Vec::new();
    for (label, text) in [
        ("plain C", C_HEADER),
        ("C++", CPP_HEADER),
        ("invalid", BROKEN_HEADER),
        ("C++ again", CPP_HEADER),
        ("plain C again", C_HEADER),
    ] {
        write(&root, "include/a.h", text);
        let cold = home.check(&root, &["--no-cache"]);
        let cached = home.check(&root, &[]);
        let warm = home.check(&root, &[]);
        assert_eq!(cached, cold, "{label}: the run that stores the entry");
        assert_eq!(warm, cold, "{label}: the run that reads the entry");
        stored.push(cold);
    }
    assert!(
        stored[0].contains("get_length") && !stored[0].contains("[meta/parse-error]"),
        "plain C parses as C: {}",
        stored[0]
    );
    assert!(
        stored[1].contains("`push`") && !stored[1].contains("[meta/parse-error]"),
        "the header parses as C++ and reports the C++ method: {}",
        stored[1]
    );
    assert!(
        stored[2].contains("include/a.h:1:1") && stored[2].contains("[meta/parse-error]"),
        "invalid input reports the C parse position: {}",
        stored[2]
    );
    assert_eq!(stored[3], stored[1], "the C++ outcome returns unchanged");
    assert_eq!(stored[4], stored[0], "the C outcome returns unchanged");
}

/// One `.h` path whose bytes move between plain C, C++, Objective-C, and invalid input. Every
/// cached run must equal an uncached run of the same bytes, so an entry stored for one outcome
/// is never served for another.
#[test]
fn cache_follows_a_header_across_the_header_fallbacks() {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    let home = CacheHome::new();
    let mut stored = Vec::new();
    for (label, text) in [
        ("plain C", C_HEADER),
        ("C++", CPP_HEADER),
        ("invalid", BROKEN_HEADER),
        ("C++ again", CPP_HEADER),
        ("plain C again", C_HEADER),
        ("Objective-C", OBJC_HEADER),
        ("C++ after Objective-C", CPP_HEADER),
        ("Objective-C again", OBJC_HEADER),
    ] {
        write(&root, "include/a.h", text);
        let cold = home.check(&root, &["--no-cache"]);
        let cached = home.check(&root, &[]);
        let warm = home.check(&root, &[]);
        assert_eq!(cached, cold, "{label}: the run that stores the entry");
        assert_eq!(warm, cold, "{label}: the run that reads the entry");
        stored.push(cold);
    }
    assert!(
        stored[0].contains("get_length") && !stored[0].contains("[meta/parse-error]"),
        "plain C parses as C: {}",
        stored[0]
    );
    assert!(
        stored[1].contains("`push`") && !stored[1].contains("[meta/parse-error]"),
        "the header parses as C++ and reports the C++ method: {}",
        stored[1]
    );
    assert!(
        stored[2].contains("include/a.h:1:1") && stored[2].contains("[meta/parse-error]"),
        "invalid input reports the C parse position: {}",
        stored[2]
    );
    assert_eq!(stored[3], stored[1], "the C++ outcome returns unchanged");
    assert_eq!(stored[4], stored[0], "the C outcome returns unchanged");
    assert!(
        stored[5].contains("include/a.h:6:5")
            && stored[5].contains("[grep/contiguous-log-literal]")
            && !stored[5].contains("[meta/parse-error]"),
        "the header parses as Objective-C and reports the `NSLog` call: {}",
        stored[5]
    );
    assert_eq!(
        stored[6], stored[1],
        "the C++ outcome returns after Objective-C"
    );
    assert_eq!(
        stored[7], stored[5],
        "the Objective-C outcome returns unchanged"
    );
}

/// The cache key is the path plus the content hash, and a path fixes its routed language. The
/// same bytes at a `.h` path and at a `.c` path must therefore keep their own outcomes: the
/// header falls back to C++ and the `.c` file stays a C parse error, cold and warm.
#[test]
fn cache_keeps_the_same_bytes_apart_by_routed_path() {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(&root, "include/stack.h", CPP_HEADER);
    write(&root, "src/stack.c", CPP_HEADER);
    let home = CacheHome::new();
    let cold = home.check(&root, &["--no-cache"]);
    assert!(
        cold.contains("src/stack.c:3:1") && cold.contains("[meta/parse-error]"),
        "the `.c` file is a C parse error: {cold}"
    );
    assert!(
        !cold.contains("include/stack.h:3:1") && cold.contains("include/stack.h:6:"),
        "the `.h` file is analyzed as C++: {cold}"
    );
    assert_eq!(
        home.check(&root, &[]),
        cold,
        "the run that stores the entries"
    );
    assert_eq!(
        home.check(&root, &[]),
        cold,
        "the run that reads the entries"
    );
    let files = home.cache_files();
    assert_eq!(
        files.len(),
        1,
        "one project stores one cache file: {files:?}"
    );
    plant(&files[0], CPP_FINDING, CPP_PLANTED);
    let warm = home.check(&root, &[]);
    assert!(
        warm.contains(CPP_PLANTED) && warm.contains("src/stack.c:3:1"),
        "the warm run serves the stored header outcome and the stored parse error: {warm}"
    );
}

/// A binary with different grammars or a different fallback hashes differently, and the hash
/// is part of the cache key, so no entry stored by another binary is served. Linux only:
/// appending a byte keeps an ELF image runnable, which signed or checked formats do not.
#[cfg(target_os = "linux")]
#[test]
fn a_different_executable_never_reuses_stored_header_outcomes() {
    use std::io::Write as _;

    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(&root, "include/stack.h", CPP_HEADER);
    let home = CacheHome::new();
    let genuine = home.check(&root, &[]);
    assert!(
        genuine.contains(CPP_FINDING),
        "the header is analyzed as C++: {genuine}"
    );
    let files = home.cache_files();
    assert_eq!(
        files.len(),
        1,
        "one project stores one cache file: {files:?}"
    );
    plant(&files[0], CPP_FINDING, CPP_PLANTED);
    assert!(
        home.check(&root, &[]).contains(CPP_PLANTED),
        "the same executable reads its own stored entry"
    );
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap_or_else(|e| panic!("scratch dir: {e}"));
    let other = scratch.path().join("sloplint-other");
    std::fs::copy(env!("CARGO_BIN_EXE_sloplint"), &other)
        .unwrap_or_else(|e| panic!("copy executable: {e}"));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&other)
        .and_then(|mut file| file.write_all(&[0]))
        .unwrap_or_else(|e| panic!("append: {e}"));
    let out = spawn_when_not_busy(|| home.output_of(&other, &root, &[]));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "the altered executable must run cleanly: {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains(CPP_FINDING) && !stdout.contains(CPP_PLANTED),
        "a different executable served an entry stored by another one: {stdout}"
    );
}
