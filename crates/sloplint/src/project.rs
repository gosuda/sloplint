use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sloplint_core::config::{Config, ConfigError, Mode, Strictness, parse_config};
use sloplint_core::text::normalize_rel_path;
use sloplint_core::{CustomRuleSpec, FailOn};

use crate::RunError;
use crate::discover;

/// Maximum size of `sloplint.toml` or any single custom rule file (1 MiB).
///
/// Legitimate configuration files are small (< 100 KB; this repository uses 65 bytes).
/// 1 MiB accommodates ~25,000 lines of TOML, exceeding any legitimate configuration
/// while stopping oversized hostile inputs before full read and parse.
pub const MAX_CONFIG_FILE_BYTES: u64 = 1_048_576;

/// Maximum combined size across all matched custom rule files (16 MiB).
///
/// Accommodates up to 16 maximum-size rule files or hundreds of modular rule sets
/// holding thousands of custom tree-sitter rules, far above legitimate repository needs.
pub const MAX_CUSTOM_RULES_TOTAL_BYTES: u64 = 16_777_216;

/// Maximum size of a ratchet baseline file (512 MiB).
///
/// Real baselines measure ~186 bytes/entry on this repository (1,575 entries in 286 KB)
/// and ~209 bytes/entry on a Go module-cache tree with deep paths (15,201 entries in 3.19 MB).
/// For the 1,000,000 entry ceiling, 512 MiB provides ~536 bytes/entry (>2.5x measured average),
/// accommodating long paths while preventing memory exhaustion.
pub const MAX_BASELINE_BYTES: u64 = 536_870_912;

/// File name of the configuration file.
pub const CONFIG_FILE: &str = "sloplint.toml";

/// Where a run looks for its project root and configuration file.
#[derive(Debug, Clone)]
pub struct Location {
    /// Project root; every path is relative to it.
    pub root: PathBuf,
    /// Explicit configuration file from `--config`. `None` means no explicit
    /// file was given, so `<root>/sloplint.toml` applies when present and
    /// defaults apply when absent.
    pub config_path: Option<PathBuf>,
}

/// CLI overrides applied on top of the configuration file.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overrides {
    /// Strictness override.
    pub strictness: Option<Strictness>,
    /// Gate mode override.
    pub mode: Option<Mode>,
    /// Fail-on override.
    pub fail_on: Option<FailOn>,
}

/// A fully resolved configuration plus the custom-rule files it absorbed.
pub struct Loaded {
    /// Configuration with overrides applied.
    pub config: Config,
    /// Custom-rule files as `(relative path, text)`, sorted by path.
    pub custom_sources: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct Peek {
    project: Option<PeekProject>,
}

#[derive(Deserialize)]
struct PeekProject {
    #[serde(default)]
    custom_rule_files: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomRuleFile {
    #[serde(default)]
    custom_rules: Vec<CustomRuleSpec>,
}

#[derive(Serialize)]
struct CustomFragment<'a> {
    custom_rules: &'a [CustomRuleSpec],
}

/// Resolves the project root and configuration path: explicit values first, then the nearest ancestor of `cwd` that holds `sloplint.toml` or `.git`.
///
/// The ancestor walk stops at the first directory with either marker, so a
/// `sloplint.toml` above the nearest `.git` belongs to some other project and
/// is never used for this checkout; `--config` is the explicit override. With
/// no marker anywhere the root is `cwd`.
///
/// An explicit `config` is kept as `Some`, even when the file does not exist,
/// so `load` can fail on it. `None` means the configuration is implicit and
/// `load` falls back to defaults when `<root>/sloplint.toml` is absent.
pub fn locate(cwd: &Path, root: Option<&Path>, config: Option<&Path>) -> Location {
    let config_path = config.map(|path| cwd.join(path));
    match (root, config_path) {
        (Some(root), config_path) => Location {
            root: root.to_path_buf(),
            config_path,
        },
        (None, Some(config_path)) => Location {
            root: config_path
                .parent()
                .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf),
            config_path: Some(config_path),
        },
        (None, None) => Location {
            root: find_root(cwd),
            config_path: None,
        },
    }
}

/// Reads and merges the configuration, including every `custom_rule_files` match, then applies overrides.
///
/// An explicit configuration file must exist: a missing file fails with the
/// path, and any other read failure reports the path with its I/O error. An
/// implicit configuration falls back to defaults when absent. Each candidate
/// is read exactly once; no existence probe precedes the read.
pub fn load(location: &Location, overrides: Overrides) -> Result<Loaded, RunError> {
    let (main, origin) = match &location.config_path {
        Some(path) => (read_required(path)?, origin_name(path)),
        None => {
            let path = location.root.join(CONFIG_FILE);
            match read_bounded_string(&path, MAX_CONFIG_FILE_BYTES) {
                Ok(text) => (text, origin_name(&path)),
                Err(source) if source.kind() == io::ErrorKind::NotFound => {
                    match fs::symlink_metadata(&path) {
                        Ok(meta) if meta.file_type().is_symlink() => {
                            return Err(RunError::Io { path, source });
                        }
                        Ok(_) => {}
                        Err(meta_err) if meta_err.kind() == io::ErrorKind::NotFound => {}
                        Err(meta_err) => {
                            return Err(RunError::Io {
                                path,
                                source: meta_err,
                            });
                        }
                    }
                    (String::new(), origin_name(&path))
                }
                Err(source) => {
                    return Err(RunError::Io { path, source });
                }
            }
        }
    };
    let custom_sources = read_custom_sources(&location.root, &custom_rule_patterns(&main))?;
    let mut specs = Vec::new();
    for (rel, text) in &custom_sources {
        let file: CustomRuleFile =
            toml::from_str(text).map_err(|error| ConfigError::from_toml(rel, text, &error))?;
        specs.extend(file.custom_rules);
    }
    let combined = combine(main, &specs, &origin)?;
    let mut config = parse_config(&combined, &origin)?;
    if let Some(strictness) = overrides.strictness {
        config.project.strictness = strictness;
    }
    if let Some(mode) = overrides.mode {
        config.project.mode = mode;
    }
    if let Some(fail_on) = overrides.fail_on {
        config.project.fail_on = fail_on;
    }
    Ok(Loaded {
        config,
        custom_sources,
    })
}

/// Root-relative PATH scopes, indexed so a membership test costs one hash lookup per path component.
#[derive(Debug, Default)]
pub struct Scopes {
    paths: HashSet<PathBuf>,
}

impl Scopes {
    /// Builds scopes directly from root-relative paths, without resolving anything on disk.
    #[cfg(test)]
    pub(crate) fn from_relative(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            paths: paths.into_iter().collect(),
        }
    }

    /// Whether `path` is one of the scopes or lies under one, matching whole path components; no scopes cover the whole root.
    pub fn contains(&self, path: &Path) -> bool {
        self.paths.is_empty()
            || path
                .ancestors()
                .any(|ancestor| self.paths.contains(ancestor))
    }
}

/// Resolves the PATH arguments to root-relative scopes; no scopes means the whole root.
///
/// Every PATH is validated, including those after one that names the root itself.
pub fn scopes(root: &Path, cwd: &Path, paths: &[PathBuf]) -> Result<Scopes, RunError> {
    let mut resolved: HashSet<PathBuf> = HashSet::new();
    let mut whole_root = false;
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            cwd.join(path)
        };
        let canonical = fs::canonicalize(&absolute).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                RunError::PathNotFound { path: path.clone() }
            } else {
                RunError::Io {
                    path: absolute.clone(),
                    source,
                }
            }
        })?;
        let relative = canonical
            .strip_prefix(root)
            .map_err(|_| RunError::PathOutsideRoot { path: path.clone() })?;
        let rel = normalize_rel_path(relative).ok_or_else(|| RunError::Io {
            path: path.clone(),
            source: io::Error::new(io::ErrorKind::InvalidData, "path is not UTF-8"),
        })?;
        if rel.is_empty() {
            whole_root = true;
        } else {
            resolved.insert(relative.to_path_buf());
        }
    }
    if whole_root {
        resolved.clear();
    }
    Ok(Scopes { paths: resolved })
}

/// Resolves a configured baseline name to an absolute path inside `root`.
///
/// `root` must already be canonical. Empty and absolute names are rejected.
/// An existing candidate is canonicalized whole, so symlink chains to the
/// outside are rejected by the prefix check. A missing candidate is allowed
/// only as an absent regular leaf: a dangling symlink leaf is rejected
/// without being followed, and the immediate parent is canonicalized, so a
/// missing parent stays an I/O failure. Nothing is created or written.
pub fn resolve_within_root(root: &Path, name: &str) -> Result<PathBuf, RunError> {
    let rel = Path::new(name);
    if name.is_empty() || rel.is_absolute() {
        return Err(RunError::BaselineOutsideRoot {
            path: PathBuf::from(name),
        });
    }
    let candidate = root.join(rel);
    match fs::canonicalize(&candidate) {
        Ok(resolved) => {
            if !resolved.starts_with(root) {
                return Err(RunError::BaselineOutsideRoot {
                    path: PathBuf::from(name),
                });
            }
            Ok(resolved)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::symlink_metadata(&candidate) {
                Ok(meta) if meta.file_type().is_symlink() => Err(RunError::BaselineOutsideRoot {
                    path: PathBuf::from(name),
                }),
                Ok(_) => Err(RunError::Io {
                    path: candidate.clone(),
                    source: error,
                }),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let parent = candidate.parent().unwrap_or(root);
                    let canonical = fs::canonicalize(parent).map_err(|source| RunError::Io {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                    let resolved = match candidate.file_name() {
                        Some(file) => canonical.join(file),
                        None => canonical,
                    };
                    if !resolved.starts_with(root) {
                        return Err(RunError::BaselineOutsideRoot {
                            path: PathBuf::from(name),
                        });
                    }
                    Ok(resolved)
                }
                Err(source) => Err(RunError::Io {
                    path: candidate.clone(),
                    source,
                }),
            }
        }
        Err(source) => Err(RunError::Io {
            path: candidate.clone(),
            source,
        }),
    }
}

/// Opens `path` only when it is a regular file, returning the handle and its size.
///
/// On unix the open itself is non-blocking and never adopts a terminal as the
/// controlling one, so a FIFO planted at the path
/// (even one swapped in after a directory listing) returns at once instead of
/// waiting for a writer; the opened handle is then inspected and anything but
/// a regular file is refused before a byte is read. Symlinks to regular files
/// are followed. A directory keeps the platform's own directory error.
pub fn open_regular(path: &Path) -> io::Result<(fs::File, u64)> {
    let file = open_for_read(path)?;
    let meta = file.metadata()?;
    require_regular(&file, &meta)?;
    Ok((file, meta.len()))
}

#[cfg(unix)]
fn open_for_read(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
}

#[cfg(not(unix))]
fn open_for_read(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}

/// Opens `path` without following a trailing symlink, returning the handle and its size.
///
/// Unix uses `O_NOFOLLOW`, so a symlink at the final component fails with a loop error
/// instead of being followed. Windows refuses a trailing symlink or reparse point before
/// opening. Configuration and baseline inputs keep using [`open_regular`], which follows
/// symlinks to regular files; only the cache uses this opener.
pub fn open_regular_nofollow(path: &Path) -> io::Result<(fs::File, u64)> {
    let file = open_for_read_nofollow(path)?;
    let meta = file.metadata()?;
    require_regular(&file, &meta)?;
    Ok((file, meta.len()))
}

#[cfg(unix)]
fn open_for_read_nofollow(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_for_read_nofollow(path: &Path) -> io::Result<fs::File> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
    }
    fs::File::open(path)
}

fn require_regular(file: &fs::File, meta: &fs::Metadata) -> io::Result<()> {
    if meta.is_file() {
        return Ok(());
    }
    if meta.is_dir() {
        let mut probe = file;
        let mut byte = [0u8; 1];
        let _ = probe.read(&mut byte)?;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "not a regular file",
    ))
}

/// Reads a whole regular file, never more than the size it had when opened.
///
/// Configuration and baseline inputs enforce ceilings via [`read_bounded_string`].
/// Trusted internal reads such as executable hashing use this reader. Streams such as
/// `/dev/zero` are refused as non-regular instead, and the read is bounded by
/// the opened file's own size, so a file that keeps growing fails with an
/// error rather than being read without end.
pub fn read_regular(path: &Path) -> io::Result<Vec<u8>> {
    let (file, len) = open_regular(path)?;
    read_bounded(file, len)
}

/// Reads a regular file of at most `max_len` bytes without following a trailing symlink.
///
/// The cache uses this so a link planted at a shard file is refused instead of being
/// followed. The length comes from the opened handle, so a file swapped in after any
/// earlier check cannot exceed the cap. Configuration and baseline inputs keep using
/// [`read_regular`].
pub fn read_regular_nofollow(path: &Path, max_len: u64) -> io::Result<Vec<u8>> {
    let (file, len) = open_regular_nofollow(path)?;
    if len > max_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file of {len} bytes exceeds the {max_len}-byte cap"),
        ));
    }
    read_bounded(file, len)
}

fn read_bounded(file: fs::File, len: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(len).unwrap_or(usize::MAX))
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::OutOfMemory,
                format!("file of {len} bytes does not fit in memory"),
            )
        })?;
    file.take(len.saturating_add(1)).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file grew while being read",
        ));
    }
    Ok(bytes)
}

/// Reads a regular file as UTF-8 text, enforcing `max_bytes` ceiling; see [`read_regular`].
pub fn read_bounded_string(path: &Path, max_bytes: u64) -> io::Result<String> {
    let (file, len) = open_regular(path)?;
    if len > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!("file of {len} bytes exceeds ceiling of {max_bytes} bytes"),
        ));
    }
    let bytes = read_bounded(file, len)?;
    String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        )
    })
}

/// Compiles glob patterns into one set, naming `key` in any error.
pub fn glob_set(patterns: &[String], key: &str) -> Result<GlobSet, ConfigError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| ConfigError::Glob {
            key: key.to_string(),
            glob: pattern.clone(),
            message: error.to_string(),
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| ConfigError::Glob {
        key: key.to_string(),
        glob: patterns.join(", "),
        message: error.to_string(),
    })
}

fn find_root(cwd: &Path) -> PathBuf {
    cwd.ancestors()
        .find(|dir| dir.join(CONFIG_FILE).is_file() || dir.join(".git").exists())
        .unwrap_or(cwd)
        .to_path_buf()
}

fn read_required(path: &Path) -> Result<String, RunError> {
    read_bounded_string(path, MAX_CONFIG_FILE_BYTES).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            RunError::PathNotFound {
                path: path.to_path_buf(),
            }
        } else {
            RunError::Io {
                path: path.to_path_buf(),
                source,
            }
        }
    })
}

fn origin_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || CONFIG_FILE.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn custom_rule_patterns(text: &str) -> Vec<String> {
    toml::from_str::<Peek>(text)
        .ok()
        .and_then(|peek| peek.project)
        .map(|project| project.custom_rule_files)
        .unwrap_or_default()
}

fn read_custom_sources(
    root: &Path,
    patterns: &[String],
) -> Result<Vec<(String, String)>, RunError> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    let matcher = glob_set(patterns, "project.custom_rule_files")?;
    let roots = custom_source_roots(root, patterns)?;
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let mut total_bytes: u64 = 0;
    for base in &roots {
        read_custom_root(root, base, &matcher, &mut sources, &mut total_bytes)?;
    }
    Ok(sources.into_iter().collect())
}

enum CustomSelection<'a> {
    Literal(&'a str),
    Prefix(&'a str),
}

fn custom_selection(pattern: &str) -> Result<CustomSelection<'_>, ConfigError> {
    if pattern.starts_with('/') || pattern.starts_with('\\') || Path::new(pattern).is_absolute() {
        return Err(ConfigError::Glob {
            key: "project.custom_rule_files".to_string(),
            glob: pattern.to_string(),
            message: "pattern must be relative".to_string(),
        });
    }
    if pattern.split(['/', '\\']).any(|part| part == "..") {
        return Err(ConfigError::Glob {
            key: "project.custom_rule_files".to_string(),
            glob: pattern.to_string(),
            message: "pattern cannot contain `..`".to_string(),
        });
    }
    let mut end = None;
    for (index, c) in pattern.char_indices() {
        if matches!(c, '*' | '?' | '[' | '{' | '\\') {
            end = Some(index);
            break;
        }
    }
    match end {
        None => Ok(CustomSelection::Literal(pattern)),
        Some(index) => match pattern[..index].rfind('/') {
            Some(pos) => Ok(CustomSelection::Prefix(&pattern[..index][..pos])),
            None => Ok(CustomSelection::Prefix("")),
        },
    }
}

fn custom_source_roots(root: &Path, patterns: &[String]) -> Result<BTreeSet<PathBuf>, ConfigError> {
    let mut roots = BTreeSet::new();
    for pattern in patterns {
        match custom_selection(pattern)? {
            CustomSelection::Literal(rel) => {
                roots.insert(root.join(rel));
            }
            CustomSelection::Prefix(dir) => {
                if dir.is_empty() {
                    roots.insert(root.to_path_buf());
                } else {
                    roots.insert(root.join(dir));
                }
            }
        }
    }
    Ok(roots)
}

fn read_custom_root(
    root: &Path,
    base: &Path,
    matcher: &GlobSet,
    sources: &mut BTreeMap<String, String>,
    total_bytes: &mut u64,
) -> Result<(), RunError> {
    let is_root = base == root;
    if !is_root {
        match has_dir_symlink(root, base) {
            Ok(true) => {
                let rel = base
                    .strip_prefix(root)
                    .ok()
                    .and_then(normalize_rel_path)
                    .unwrap_or_else(|| base.display().to_string());
                return Err(RunError::Config(ConfigError::Glob {
                    key: "project.custom_rule_files".to_string(),
                    glob: rel,
                    message: "custom rule file cannot be a symlink".to_string(),
                }));
            }
            Ok(false) => {}
            Err((path, source)) => return Err(RunError::Io { path, source }),
        }
    }
    match fs::symlink_metadata(base) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if !is_root {
                let rel = base
                    .strip_prefix(root)
                    .ok()
                    .and_then(normalize_rel_path)
                    .unwrap_or_else(|| base.display().to_string());
                return Err(RunError::Config(ConfigError::Glob {
                    key: "project.custom_rule_files".to_string(),
                    glob: rel,
                    message: "custom rule file cannot be a symlink".to_string(),
                }));
            }
        }
        Ok(_) => {}
        Err(source)
            if source.kind() == io::ErrorKind::NotFound
                || source.kind() == io::ErrorKind::NotADirectory =>
        {
            return Ok(());
        }
        Err(source) => {
            return Err(RunError::Io {
                path: base.to_path_buf(),
                source,
            });
        }
    }
    let mut builder = WalkBuilder::new(base);
    builder
        .standard_filters(false)
        .require_git(false)
        .follow_links(false);
    for result in builder.build() {
        let entry = result.map_err(|error| discover::walk_error(root, error))?;
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let Some(rel) = normalize_rel_path(relative) else {
            continue;
        };
        if rel.is_empty() || !matcher.is_match(rel.as_str()) || sources.contains_key(&rel) {
            continue;
        }
        if kind.is_symlink() {
            return Err(RunError::Config(ConfigError::Glob {
                key: "project.custom_rule_files".to_string(),
                glob: rel,
                message: "custom rule file cannot be a symlink".to_string(),
            }));
        }
        let (file, len) = open_regular_nofollow(entry.path()).map_err(|source| RunError::Io {
            path: entry.path().to_path_buf(),
            source,
        })?;
        if len > MAX_CONFIG_FILE_BYTES {
            return Err(RunError::Io {
                path: entry.path().to_path_buf(),
                source: io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    format!("file of {len} bytes exceeds ceiling of {MAX_CONFIG_FILE_BYTES} bytes"),
                ),
            });
        }
        let next_total = total_bytes.saturating_add(len);
        if next_total > MAX_CUSTOM_RULES_TOTAL_BYTES {
            return Err(RunError::Io {
                path: entry.path().to_path_buf(),
                source: io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    format!(
                        "custom rule files exceed total ceiling of {MAX_CUSTOM_RULES_TOTAL_BYTES} bytes"
                    ),
                ),
            });
        }
        let bytes = read_bounded(file, len).map_err(|source| RunError::Io {
            path: entry.path().to_path_buf(),
            source,
        })?;
        let text = String::from_utf8(bytes).map_err(|_| RunError::Io {
            path: entry.path().to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            ),
        })?;
        *total_bytes = next_total;
        sources.insert(rel, text);
    }
    Ok(())
}

fn has_dir_symlink(root: &Path, candidate: &Path) -> Result<bool, (PathBuf, io::Error)> {
    let mut current = candidate.parent();
    while let Some(dir) = current {
        if dir == root {
            break;
        }
        if !dir.starts_with(root) {
            return Ok(true);
        }
        match fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_symlink() => return Ok(true),
            Ok(_) => {}
            Err(source) => return Err((dir.to_path_buf(), source)),
        }
        current = dir.parent();
    }
    Ok(false)
}

fn combine(main: String, specs: &[CustomRuleSpec], origin: &str) -> Result<String, ConfigError> {
    if specs.is_empty() {
        return Ok(main);
    }
    let fragment = toml::to_string(&CustomFragment {
        custom_rules: specs,
    })
    .map_err(|error| ConfigError::Toml {
        origin: origin.to_string(),
        message: error.to_string(),
    })?;
    Ok(format!("{main}\n{fragment}"))
}
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::Scopes;

    #[test]
    fn non_utf8_skip_scope_membership_is_hermetic() {
        let valid = Path::new("src/lib.rs");
        let whole = Scopes::default();
        let src = Scopes::from_relative([PathBuf::from("src")]);
        let other = Scopes::from_relative([PathBuf::from("other")]);
        assert!(
            whole.contains(valid),
            "the whole scope contains the valid path"
        );
        assert!(src.contains(valid), "the src scope contains the valid path");
        assert!(
            !other.contains(valid),
            "the other scope excludes the valid path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bad = Path::new(std::ffi::OsStr::from_bytes(b"other/bad\xff.rs"));
            assert!(
                whole.contains(bad),
                "the whole scope contains the non-utf8 path"
            );
            assert!(
                !src.contains(bad),
                "the src scope excludes the non-utf8 path"
            );
            assert!(
                other.contains(bad),
                "the other scope contains the non-utf8 path"
            );
        }
        #[cfg(not(unix))]
        {
            let standin = Path::new("other/bad\u{FFFD}.rs");
            assert!(
                whole.contains(standin),
                "the whole scope contains the standin path"
            );
            assert!(
                !src.contains(standin),
                "the src scope excludes the standin path"
            );
            assert!(
                other.contains(standin),
                "the other scope contains the standin path"
            );
        }
    }

    #[test]
    fn lossy_equal_file_scope_is_hermetic() {
        let valid = Path::new("other/bad\u{FFFD}.rs");
        let link = Path::new("other/link.rs");
        let whole = Scopes::default();
        let explicit = Scopes::from_relative([valid.to_path_buf()]);
        let parent = Scopes::from_relative([PathBuf::from("other")]);
        assert!(
            whole.contains(valid),
            "the whole scope contains the valid path"
        );
        assert!(
            whole.contains(link),
            "the whole scope contains the link path"
        );
        assert!(
            explicit.contains(valid),
            "the explicit scope contains the valid path"
        );
        assert!(
            !explicit.contains(link),
            "the explicit scope excludes the link path"
        );
        assert!(
            parent.contains(valid),
            "the parent scope contains the valid path"
        );
        assert!(
            parent.contains(link),
            "the parent scope contains the link path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = Path::new(std::ffi::OsStr::from_bytes(b"other/bad\xff.rs"));
            assert_eq!(
                invalid.to_string_lossy(),
                valid.to_string_lossy(),
                "the names are lossy-equal"
            );
            assert!(
                whole.contains(invalid),
                "the whole scope contains the lossy-equal path"
            );
            assert!(
                !explicit.contains(invalid),
                "lossy-equal names share no explicit file scope"
            );
            assert!(
                parent.contains(invalid),
                "the parent scope contains the lossy-equal path"
            );
        }
    }

    #[test]
    fn lossy_equal_dir_scope_is_hermetic() {
        let pipe = Path::new("parent/scope\u{FFFD}/pipe.rs");
        let child = Path::new("parent/scope\u{FFFD}/b.rs");
        let whole = Scopes::default();
        let explicit = Scopes::from_relative([PathBuf::from("parent/scope\u{FFFD}")]);
        let parent = Scopes::from_relative([PathBuf::from("parent")]);
        assert!(
            whole.contains(pipe),
            "the whole scope contains the pipe path"
        );
        assert!(
            explicit.contains(pipe),
            "the explicit scope contains the pipe path"
        );
        assert!(
            !explicit.contains(Path::new("parent/unrelated/b.rs")),
            "the explicit scope excludes the unrelated path"
        );
        assert!(
            parent.contains(pipe),
            "the parent scope contains the pipe path"
        );
        assert!(
            whole.contains(child),
            "the whole scope contains the child path"
        );
        assert!(
            explicit.contains(child),
            "the explicit scope contains the child path"
        );
        assert!(
            parent.contains(child),
            "the parent scope contains the child path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid_dir = Path::new(std::ffi::OsStr::from_bytes(b"parent/scope\xff"));
            assert_eq!(
                invalid_dir.to_string_lossy(),
                "parent/scope\u{FFFD}",
                "the directories are lossy-equal"
            );
            let invalid_child = Path::new(std::ffi::OsStr::from_bytes(b"parent/scope\xff/b.rs"));
            assert!(
                whole.contains(invalid_child),
                "the whole scope contains the lossy child"
            );
            assert!(
                !explicit.contains(invalid_child),
                "lossy-equal directories share no explicit scope"
            );
            assert!(
                parent.contains(invalid_child),
                "the parent scope contains the lossy child"
            );
        }
    }

    #[test]
    fn boundary_non_utf8_skip_scope_membership_is_hermetic() {
        let valid = Path::new("src/bad\u{FFFD}.rs");
        let whole = Scopes::default();
        assert!(
            whole.contains(valid),
            "the whole scope contains the valid path"
        );
        let src = Scopes::from_relative([PathBuf::from("src")]);
        assert!(src.contains(valid), "the src scope contains the valid path");
        let other = Scopes::from_relative([PathBuf::from("other")]);
        assert!(
            !other.contains(valid),
            "the other scope excludes the valid path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bad = Path::new(std::ffi::OsStr::from_bytes(b"src/bad\xff.rs"));
            assert!(
                whole.contains(bad),
                "the whole scope contains the non-utf8 path"
            );
            assert!(
                src.contains(bad),
                "the src scope contains the non-utf8 path"
            );
            assert!(
                !other.contains(bad),
                "the other scope excludes the non-utf8 path"
            );
            let explicit = Scopes::from_relative([valid.to_path_buf()]);
            assert!(
                explicit.contains(valid),
                "the explicit scope contains the valid path"
            );
            assert!(
                !explicit.contains(bad),
                "lossy-equal names share no explicit file scope"
            );
        }
        #[cfg(not(unix))]
        {
            let second = Path::new("src/other\u{FFFD}.rs");
            let explicit = Scopes::from_relative([valid.to_path_buf()]);
            assert!(
                !explicit.contains(second),
                "distinct replacement-character names share no explicit file scope"
            );
        }
    }
}
