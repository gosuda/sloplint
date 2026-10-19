use std::io;
use std::path::{Path, PathBuf};

use globset::GlobSet;
use ignore::WalkBuilder;
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
pub fn discover(root: &Path, config: &Config) -> Result<Discovery, RunError> {
    let patterns: Vec<String> = config
        .project
        .exclude
        .iter()
        .chain(&config.project.extend_exclude)
        .cloned()
        .collect();
    let excludes = glob_set(&patterns, "project.exclude")?;
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for result in walk(root, excludes) {
        let entry = result.map_err(|error| walk_error(root, error))?;
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        match normalize_rel_path(relative) {
            Some(rel) => {
                if let Some(lang) = enabled_lang(&rel, config) {
                    if kind.is_file() {
                        entries.push(Entry {
                            rel,
                            abs: entry.path().to_path_buf(),
                            lang,
                        });
                    } else {
                        skipped.push(relative.to_path_buf());
                    }
                }
            }
            None => {
                let lossy = lossy_rel(relative);
                if enabled_lang(&lossy, config).is_some() {
                    skipped.push(relative.to_path_buf());
                }
            }
        }
    }
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(Discovery { entries, skipped })
}

fn lossy_rel(relative: &Path) -> String {
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Walks `root` with gitignore and hidden-file filters, pruning every entry that matches `prune`.
pub fn walk(root: &Path, prune: GlobSet) -> ignore::Walk {
    let base = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(true)
        .require_git(false)
        .follow_links(false);
    builder
        .filter_entry(move |entry| entry.depth() == 0 || !is_pruned(&base, entry.path(), &prune));
    builder.build()
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
