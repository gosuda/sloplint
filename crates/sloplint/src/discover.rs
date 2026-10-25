use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use globset::GlobSet;
use ignore::{DirEntry, ParallelVisitor, ParallelVisitorBuilder, WalkBuilder, WalkState};
use sloplint_core::Config;
use sloplint_core::lang::Lang;
use sloplint_core::text::normalize_rel_path;

use crate::RunError;
use crate::project::glob_set;

/// One lintable file: its repo-relative path, absolute path, and language.
pub struct Entry {
    /// Path relative to the project root, with `/` separators.
    pub rel: String,
    /// Absolute path on disk.
    pub abs: PathBuf,
    /// Language selected by the file's name.
    pub lang: Lang,
}

/// Every lintable file under the root, sorted by relative path.
pub struct Discovery {
    /// Lintable files in relative-path order.
    pub entries: Vec<Entry>,
    /// Lintable names that are never analyzed, as root-relative paths: non-UTF-8 names and entries that are not regular files (symlinks are never followed, FIFOs and devices never opened).
    pub skipped: Vec<PathBuf>,
}

/// Lists lintable files under `root` that match an enabled language and no exclude glob.
///
/// The tree is walked on `threads` threads. The result does not depend on
/// the thread count: entries and skipped names are sorted, and when the walk
/// fails the reported error is the least error message, chosen after every
/// entry is visited.
pub fn discover(root: &Path, config: &Config, threads: usize) -> Result<Discovery, RunError> {
    let patterns: Vec<String> = config
        .project
        .exclude
        .iter()
        .chain(&config.project.extend_exclude)
        .cloned()
        .collect();
    let excludes = glob_set(&patterns, "project.exclude")?;
    let shared = Mutex::new(Found::default());
    walker(root, excludes)
        .threads(threads)
        .build_parallel()
        .visit(&mut Collect {
            root,
            config,
            shared: &shared,
        });
    let Found {
        mut entries,
        mut skipped,
        errors,
    } = shared.into_inner().unwrap_or_else(PoisonError::into_inner);
    if let Some(message) = errors.into_iter().min() {
        return Err(RunError::Io {
            path: root.to_path_buf(),
            source: io::Error::other(message),
        });
    }
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    skipped.sort();
    Ok(Discovery { entries, skipped })
}

/// What the walker threads found: entries, skipped names, and walk error messages.
#[derive(Default)]
struct Found {
    entries: Vec<Entry>,
    skipped: Vec<PathBuf>,
    errors: Vec<String>,
}

impl Found {
    fn absorb(&mut self, other: &mut Found) {
        self.entries.append(&mut other.entries);
        self.skipped.append(&mut other.skipped);
        self.errors.append(&mut other.errors);
    }

    fn record(&mut self, root: &Path, config: &Config, entry: &DirEntry) {
        let Some(kind) = entry.file_type() else {
            return;
        };
        if kind.is_dir() {
            return;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            return;
        };
        let decision = if kind.is_file() {
            decide_file_name(relative, config)
        } else {
            decide_non_file_name(relative, config)
        };
        match decision {
            NameDecision::Analyze { rel, lang } => self.entries.push(Entry {
                rel,
                abs: entry.path().to_path_buf(),
                lang,
            }),
            NameDecision::Skipped => self.skipped.push(relative.to_path_buf()),
            NameDecision::Ignored => {}
        }
    }
}

/// Whether a walked name is analyzed, counted as a skip, or ignored outright.
pub(crate) enum NameDecision {
    Analyze { rel: String, lang: Lang },
    Skipped,
    Ignored,
}

/// Classifies a regular file's root-relative name without touching the filesystem.
pub(crate) fn decide_file_name(relative: &Path, config: &Config) -> NameDecision {
    decide_name(relative, true, config)
}

/// Classifies a non-regular walked name without touching the filesystem.
pub(crate) fn decide_non_file_name(relative: &Path, config: &Config) -> NameDecision {
    decide_name(relative, false, config)
}

fn decide_name(relative: &Path, is_file: bool, config: &Config) -> NameDecision {
    match normalize_rel_path(relative) {
        Some(rel) => match enabled_lang(&rel, config) {
            Some(lang) if is_file => NameDecision::Analyze { rel, lang },
            Some(_) => NameDecision::Skipped,
            None => NameDecision::Ignored,
        },
        None => {
            if enabled_lang(&lossy_rel(relative), config).is_some() {
                NameDecision::Skipped
            } else {
                NameDecision::Ignored
            }
        }
    }
}

/// Builds one [`Visitor`] per walker thread over a shared [`Found`].
struct Collect<'a> {
    root: &'a Path,
    config: &'a Config,
    shared: &'a Mutex<Found>,
}

impl<'a> ParallelVisitorBuilder<'a> for Collect<'a> {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 'a> {
        Box::new(Visitor {
            root: self.root,
            config: self.config,
            local: Found::default(),
            shared: self.shared,
        })
    }
}

/// One walker thread's findings, merged into the shared [`Found`] when the thread finishes.
struct Visitor<'a> {
    root: &'a Path,
    config: &'a Config,
    local: Found,
    shared: &'a Mutex<Found>,
}

impl ParallelVisitor for Visitor<'_> {
    fn visit(&mut self, result: Result<DirEntry, ignore::Error>) -> WalkState {
        match result {
            Ok(entry) => self.local.record(self.root, self.config, &entry),
            Err(error) => self.local.errors.push(error.to_string()),
        }
        WalkState::Continue
    }
}

impl Drop for Visitor<'_> {
    fn drop(&mut self) {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .absorb(&mut self.local);
    }
}

fn lossy_rel(relative: &Path) -> String {
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `root` or an ancestor holds a `.git` entry: a directory in a clone, or a file in a worktree or submodule.
fn inside_repository(root: &Path) -> bool {
    root.ancestors().any(|dir| dir.join(".git").exists())
}

/// Configures a walk of `root` with gitignore and hidden-file filters, pruning every entry that matches `prune`.
///
/// Inside a repository, `require_git` stops `.gitignore` and `.git/info/exclude` at the repository root, so files above it are not applied. `.ignore` files still apply from every ancestor, as in ripgrep. Outside a repository, ancestor `.gitignore` files apply as before.
fn walker(root: &Path, prune: GlobSet) -> WalkBuilder {
    let base = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(true)
        .require_git(inside_repository(root))
        .follow_links(false);
    builder
        .filter_entry(move |entry| entry.depth() == 0 || !is_pruned(&base, entry.path(), &prune));
    builder
}

/// Converts a walker failure into the run's I/O error.
pub fn walk_error(root: &Path, error: ignore::Error) -> RunError {
    RunError::Io {
        path: root.to_path_buf(),
        source: io::Error::other(error.to_string()),
    }
}

fn is_pruned(base: &Path, path: &Path, prune: &GlobSet) -> bool {
    path.strip_prefix(base)
        .ok()
        .and_then(normalize_rel_path)
        .is_some_and(|rel| prune.is_match(rel.as_str()))
}

fn enabled_lang(rel: &str, config: &Config) -> Option<Lang> {
    Lang::from_path(rel).filter(|lang| {
        config
            .project
            .languages
            .iter()
            .any(|key| key == lang.config_key())
    })
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use sloplint_core::config::Config;
    use sloplint_core::lang::Lang;

    use super::{NameDecision, decide_file_name, decide_non_file_name};

    #[test]
    fn non_utf8_skip_decision_is_hermetic() {
        let config = Config::default();
        let valid = Path::new("src/lib.rs");
        assert!(
            matches!(
                decide_file_name(valid, &config),
                NameDecision::Analyze { .. }
            ),
            "the valid sibling analyzes"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bad = Path::new(std::ffi::OsStr::from_bytes(b"other/bad\xff.rs"));
            assert!(
                matches!(decide_file_name(bad, &config), NameDecision::Skipped),
                "the invalid-byte name is a counted skip"
            );
        }
        #[cfg(not(unix))]
        {
            let standin = Path::new("other/bad\u{FFFD}.rs");
            assert!(
                matches!(
                    decide_file_name(standin, &config),
                    NameDecision::Analyze { .. }
                ),
                "the standin name analyzes"
            );
        }
    }

    #[test]
    fn lossy_equal_file_decision_is_hermetic() {
        let config = Config::default();
        let valid = Path::new("other/bad\u{FFFD}.rs");
        let link = Path::new("other/link.rs");
        assert!(
            matches!(
                decide_file_name(valid, &config),
                NameDecision::Analyze { .. }
            ),
            "the valid name analyzes"
        );
        assert!(
            matches!(decide_non_file_name(link, &config), NameDecision::Skipped),
            "the link is a counted skip"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = Path::new(std::ffi::OsStr::from_bytes(b"other/bad\xff.rs"));
            assert!(
                matches!(decide_file_name(invalid, &config), NameDecision::Skipped),
                "the invalid-byte name is a counted skip"
            );
        }
    }

    #[test]
    fn lossy_equal_dir_decision_is_hermetic() {
        let config = Config::default();
        let pipe = Path::new("parent/scope\u{FFFD}/pipe.rs");
        let child = Path::new("parent/scope\u{FFFD}/b.rs");
        assert!(
            matches!(decide_non_file_name(pipe, &config), NameDecision::Skipped),
            "the FIFO is a counted skip"
        );
        assert!(
            matches!(
                decide_file_name(child, &config),
                NameDecision::Analyze { .. }
            ),
            "the valid child analyzes"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid_child = Path::new(std::ffi::OsStr::from_bytes(b"parent/scope\xff/b.rs"));
            assert!(
                matches!(
                    decide_file_name(invalid_child, &config),
                    NameDecision::Skipped
                ),
                "the invalid-byte child is a counted skip"
            );
        }
    }

    #[test]
    fn boundary_non_utf8_skip_decision_is_hermetic() {
        let config = Config::default();
        let valid = Path::new("src/bad\u{FFFD}.rs");
        let NameDecision::Analyze { rel, lang } = decide_file_name(valid, &config) else {
            panic!("valid U+FFFD name must analyze");
        };
        assert_eq!(rel, "src/bad\u{FFFD}.rs");
        assert_eq!(lang, Lang::Rust);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bad = Path::new(std::ffi::OsStr::from_bytes(b"src/bad\xff.rs"));
            assert!(
                matches!(decide_file_name(bad, &config), NameDecision::Skipped),
                "invalid-byte name is a counted skip"
            );
        }
        #[cfg(not(unix))]
        {
            let second = Path::new("src/other\u{FFFD}.rs");
            assert!(
                matches!(
                    decide_file_name(second, &config),
                    NameDecision::Analyze { .. }
                ),
                "replacement-character names analyze where raw bytes are unrepresentable"
            );
        }
    }
}
