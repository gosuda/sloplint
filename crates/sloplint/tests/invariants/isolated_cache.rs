//! Cached runs through the real binary. The per-user cache lives outside the checkout and a
//! library run with the cache on would use the runner's real cache directory, so every cached
//! run here is a child process whose cache and home directories are temp dirs owned by the
//! test. Nothing mutates this process's environment, so tests stay safe to run concurrently.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::{custom_rule, mkdir, tempdir, write};

#[path = "isolated_cache/planted_links.rs"]
mod planted_links;

/// One generic-name finding, with a message that holds `GENERIC_TEXT` exactly once.
const SAMPLE: &str = "pub fn handle(flag: bool, other: bool) {}\n";

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

    /// Every stored cache file, `<xdg>/sloplint/<key>/cache-v2/shard-<NN>.bin`.
    fn cache_files(&self) -> Vec<PathBuf> {
        let namespace = self.base.join("xdg").join("sloplint");
        let Ok(keys) = std::fs::read_dir(&namespace) else {
            return Vec::new();
        };
        let mut files = Vec::new();
        for key in keys {
            let key = key.unwrap_or_else(|e| {
                panic!("cache key listing failed {}: {e}", namespace.display())
            });
            let Ok(shards) = std::fs::read_dir(key.path().join("cache-v2")) else {
                continue;
            };
            files.extend(
                shards
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| {
                                name.len() == "shard-00.bin".len()
                                    && name.starts_with("shard-")
                                    && name.ends_with(".bin")
                            })
                            && path.is_file()
                    }),
            );
        }
        files
    }
}

/// C++ that fails as C but parses as C++. Its generic-named method makes a finding that only
/// the C++ analysis can report.
const CPP_HEADER: &str = "namespace ui {

template <typename T>
class Stack {
public:
    void process_data(const T& value) { store(value); }
private:
    std::vector<T> items_;
};

}  // namespace ui
";

/// Plain C that parses as C.
const C_HEADER: &str = "#include <string.h>

unsigned long process_data(const char *s) {
    return strlen(s);
}
";

/// Objective-C: it fails as C and as C++, and its exported generic name reports at standard.
const OBJC_HEADER: &str = "@interface Counter : NSObject
- (void)increment;
@end

void process_data(NSString *name) {
    NSLog(name);
}
";

/// Invalid in C (line 1) and in C++ (line 3).
const BROKEN_HEADER: &str = "template <typename T>
struct Box { T value; };
int x = ;
";

/// The stored text of the C++ header's finding, and a same-length stand-in for it.
const CPP_FINDING: &str = "`process_data` is a generic name";

const CPP_PLANTED: &str = "`process_data` is a generic neme";

/// Rewrites the one stored occurrence of `from` in the shard payload with the same-length
/// `to`, then re-digests the payload, so the forged shard stays decodable and self-consistent.
fn plant(file: &Path, from: &str, to: &str) {
    assert_eq!(from.len(), to.len(), "a plant must keep the stored length");
    let mut bytes = std::fs::read(file).unwrap_or_else(|e| panic!("read cache: {e}"));
    assert!(
        bytes.len() > 32,
        "a shard must hold a 32-byte digest and a payload"
    );
    let spots: Vec<usize> = bytes[32..]
        .windows(from.len())
        .enumerate()
        .filter(|(_, window)| *window == from.as_bytes())
        .map(|(at, _)| at)
        .collect();
    assert_eq!(spots.len(), 1, "the cache must hold {from:?} exactly once");
    let (digest, payload) = bytes.split_at_mut(32);
    payload[spots[0]..spots[0] + to.len()].copy_from_slice(to.as_bytes());
    digest.copy_from_slice(blake3::hash(payload).as_bytes());
    std::fs::write(file, &bytes).unwrap_or_else(|e| panic!("plant rewrite failed: {e}"));
}

/// A project holding [`SAMPLE`] and an empty configuration.
fn sample_project() -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(&root, "sample.rs", SAMPLE);
    (tmp, root)
}

/// Cold-runs one project and returns its stdout plus its single stored shard file.
fn cold_shard(home: &CacheHome, root: &Path) -> (String, Vec<PathBuf>) {
    let cold = home.check(root, &[]);
    let files = home.cache_files();
    assert_eq!(
        files.len(),
        1,
        "one project stores one cache file: {files:?}"
    );
    (cold, files)
}

/// Prints the capability skip, or fails when SLOPLINT_REQUIRE_FS_CAPS demands the capability,
/// so a runner that silently drops the capability coverage turns red in CI.
#[cfg(windows)]
fn skip_unless_fs_caps_required(context: &str, error: &std::io::Error) {
    if std::env::var_os("SLOPLINT_REQUIRE_FS_CAPS").is_some_and(|value| !value.is_empty()) {
        panic!("SLOPLINT_REQUIRE_FS_CAPS is set but the capability is missing: {context}: {error}");
    }
    eprintln!("SKIP (capability unavailable) {context}: {error}");
}

/// The stored findings are reused, not recomputed: a same-length change planted inside the
/// stored finding shows up in the next cached run, and only in a cached run.
#[test]
fn warm_cache_reuses_stored_findings() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let planted = GENERIC_TEXT.replace("code", "mode");
    let (cold, files) = cold_shard(&home, &root);
    assert!(
        cold.contains(GENERIC_TEXT),
        "the cold run lost the generic-name finding"
    );
    plant(&files[0], GENERIC_TEXT, &planted);
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

/// A shard file is the BLAKE3 digest of its payload followed by the payload. Flipping one
/// byte of the stored finding keeps the payload decodable but fails the digest, so the warm
/// run must reanalyze and print exactly what an uncached run prints.
#[test]
fn a_flipped_byte_inside_a_decodable_shard_is_not_served() {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(&root, "src/lib.rs", SAMPLE);
    let home = CacheHome::new();
    let run = |extra: &[&str]| -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sloplint"));
        command
            .current_dir(&root)
            .args(["check", "--strictness", "strict", "--fail-on", "never"])
            .args(extra);
        home.isolate(&mut command)
            .output()
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
    };
    let cold = run(&[]);
    let cold_stdout = String::from_utf8_lossy(&cold.stdout);
    assert!(
        cold_stdout.contains("[complexity/boolean-params]"),
        "the cold run must report the boolean-parameter finding: {cold_stdout}"
    );
    let files = home.cache_files();
    assert_eq!(files.len(), 1, "one project stores one shard: {files:?}");
    let mut bytes = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    let Some(at) = bytes
        .windows(b"handle".len())
        .position(|window| window == b"handle")
    else {
        panic!("the stored finding must name the function");
    };
    assert_eq!(bytes[at], b'h', "the flip must land on the h of handle");
    bytes[at] = b'j';
    std::fs::write(&files[0], &bytes).unwrap_or_else(|e| panic!("plant rewrite failed: {e}"));
    let warm = run(&[]);
    let fresh = run(&["--no-cache"]);
    assert_eq!(
        String::from_utf8_lossy(&warm.stdout),
        String::from_utf8_lossy(&fresh.stdout),
        "a shard whose digest fails must be reanalyzed, not served: {}",
        String::from_utf8_lossy(&warm.stdout)
    );
    assert_eq!(
        warm.status.code(),
        fresh.status.code(),
        "the reanalyzed run must exit like the uncached run"
    );
    assert!(
        warm.stderr.is_empty(),
        "the reanalysis must stay silent on stderr: {}",
        String::from_utf8_lossy(&warm.stderr)
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
        "/// Handle.\npub fn handle(flag: bool) {}\n",
    );
    let home = CacheHome::new();
    let filtered = home.check(&root, &["--rule", "grep/generic-name"]);
    assert!(
        filtered.contains("[grep/generic-name]"),
        "the filtered run missed its rule"
    );
    assert!(
        !filtered.contains("[slop/echo-comment]"),
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
        full.contains("[slop/echo-comment]"),
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
        stored[0].contains("process_data")
            && stored[0].contains("a.h:3:")
            && !stored[0].contains("[meta/parse-error]"),
        "plain C parses as C: {}",
        stored[0]
    );
    assert!(
        stored[1].contains("process_data")
            && stored[1].contains("a.h:6:")
            && !stored[1].contains("[meta/parse-error]"),
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
        stored[0].contains("process_data")
            && stored[0].contains("a.h:3:")
            && !stored[0].contains("[meta/parse-error]"),
        "plain C parses as C: {}",
        stored[0]
    );
    assert!(
        stored[1].contains("process_data")
            && stored[1].contains("a.h:6:")
            && !stored[1].contains("[meta/parse-error]"),
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
        stored[5].contains("process_data")
            && stored[5].contains("a.h:5:")
            && stored[5].contains("[grep/generic-name]")
            && !stored[5].contains("[meta/parse-error]"),
        "the header parses as Objective-C and reports the exported generic name: {}",
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
        cold.contains("process_data")
            && !cold.contains("include/stack.h:3:1")
            && cold.contains("include/stack.h:6:"),
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
    assert_eq!(files.len(), 2, "one shard per file: {files:?}");
    let header = files
        .iter()
        .find(|path| {
            std::fs::read(path).is_ok_and(|bytes| {
                bytes
                    .windows(CPP_FINDING.len())
                    .any(|window| window == CPP_FINDING.as_bytes())
            })
        })
        .unwrap_or_else(|| panic!("no shard holds the header outcome: {files:?}"));
    plant(header, CPP_FINDING, CPP_PLANTED);
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
    let (genuine, files) = cold_shard(&home, &root);
    assert!(
        genuine.contains(CPP_FINDING),
        "the header is analyzed as C++: {genuine}"
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
    let out = crate::spawn::spawn_forgiving(|| home.output_of(&other, &root, &[]));
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

/// Gate-only flags must not invalidate stored analysis, while a rule level must.
/// The shard bytes stay identical across `--fail-on` and `--min-confidence` runs
/// and change once `grep/generic-name` is disabled.
#[test]
fn gate_only_flags_reuse_stored_shards_while_rule_levels_do_not() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let run = |extra: &[&str]| -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sloplint"));
        command
            .current_dir(&root)
            .args(["check", "--strictness", "standard"])
            .args(extra);
        home.isolate(&mut command)
            .output()
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
    };
    let cold = run(&["--fail-on", "never"]);
    assert!(
        cold.status.success(),
        "the cold run must succeed: {:?}: {}",
        cold.status,
        String::from_utf8_lossy(&cold.stderr)
    );
    assert!(
        String::from_utf8_lossy(&cold.stdout).contains(GENERIC_TEXT),
        "the cold run must report the generic-name finding"
    );
    let files = home.cache_files();
    assert_eq!(files.len(), 1, "one project stores one shard: {files:?}");
    let before = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    let gated = run(&["--fail-on", "major", "--min-confidence", "high"]);
    assert!(
        gated.status.success(),
        "the gated run must succeed: {:?}: {}",
        gated.status,
        String::from_utf8_lossy(&gated.stderr)
    );
    let after = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    assert_eq!(
        before, after,
        "gate-only flags must reuse the stored shard bytes"
    );
    write(
        &root,
        "sloplint.toml",
        "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n",
    );
    let releveled = run(&["--fail-on", "never"]);
    assert!(
        releveled.status.success(),
        "the releveled run must succeed: {:?}: {}",
        releveled.status,
        String::from_utf8_lossy(&releveled.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&releveled.stdout).contains(GENERIC_TEXT),
        "disabling the rule must drop its finding"
    );
    let rotated = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    assert_ne!(
        before, rotated,
        "a rule level changes analysis and must not reuse the stored shard bytes"
    );
}
