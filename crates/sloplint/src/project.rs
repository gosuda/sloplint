use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sloplint_core::config::{Config, ConfigError, Mode, Strictness, parse_config};
use sloplint_core::text::normalize_rel_path;
use sloplint_core::{CustomRuleSpec, FailOn};

use crate::RunError;
use crate::discover;

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

/// Resolves the project root and configuration path: explicit values first, then the nearest `sloplint.toml`, then the nearest `.git`.
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
            match fs::read_to_string(&path) {
                Ok(text) => (text, origin_name(&path)),
                Err(source) if source.kind() == io::ErrorKind::NotFound => {
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
    paths: HashSet<String>,
}

impl Scopes {
    /// Whether `path` is one of the scopes or lies under one, matching whole path components; no scopes cover the whole root.
    pub fn contains(&self, path: &str) -> bool {
        self.paths.is_empty()
            || self.paths.contains(path)
            || path
                .match_indices('/')
                .any(|(index, _)| self.paths.contains(&path[..index]))
    }
}

/// Resolves the PATH arguments to root-relative scopes; no scopes means the whole root.
///
/// Every PATH is validated, including those after one that names the root itself.
pub fn scopes(root: &Path, cwd: &Path, paths: &[PathBuf]) -> Result<Scopes, RunError> {
    let mut resolved = HashSet::new();
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
            resolved.insert(rel);
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
    let ancestors: Vec<&Path> = cwd.ancestors().collect();
    ancestors
        .iter()
        .copied()
        .find(|dir| dir.join(CONFIG_FILE).is_file())
        .or_else(|| {
            ancestors
                .iter()
                .copied()
                .find(|dir| dir.join(".git").exists())
        })
        .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf)
}

fn read_required(path: &Path) -> Result<String, RunError> {
    fs::read_to_string(path).map_err(|source| {
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
    let roots = custom_source_roots(root, patterns);
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    for base in &roots {
        read_custom_root(root, base, &matcher, &mut sources)?;
    }
    Ok(sources.into_iter().collect())
}

enum CustomSelection<'a> {
    Skip,
    Literal(&'a str),
    Prefix(&'a str),
}

fn custom_selection(pattern: &str) -> CustomSelection<'_> {
    if Path::new(pattern).is_absolute() {
        return CustomSelection::Skip;
    }
    if pattern.split('/').any(|part| part == "..") {
        return CustomSelection::Skip;
    }
    let mut end = None;
    for (index, c) in pattern.char_indices() {
        if matches!(c, '*' | '?' | '[' | '{' | '\\') {
            end = Some(index);
            break;
        }
    }
    match end {
        None => CustomSelection::Literal(pattern),
        Some(index) => match pattern[..index].rfind('/') {
            Some(pos) => CustomSelection::Prefix(&pattern[..index][..pos]),
            None => CustomSelection::Prefix(""),
        },
    }
}

fn custom_source_roots(root: &Path, patterns: &[String]) -> BTreeSet<PathBuf> {
    let mut roots = BTreeSet::new();
    for pattern in patterns {
        match custom_selection(pattern) {
            CustomSelection::Skip => {}
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
    roots
}

fn read_custom_root(
    root: &Path,
    base: &Path,
    matcher: &GlobSet,
    sources: &mut BTreeMap<String, String>,
) -> Result<(), RunError> {
    let is_root = base == root;
    if !is_root && has_dir_symlink(root, base) {
        return Ok(());
    }
    match fs::symlink_metadata(base) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if !is_root {
                return Ok(());
            }
        }
        Ok(meta) if meta.file_type().is_dir() || meta.file_type().is_file() => {}
        Ok(_) => return Ok(()),
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
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
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
        let text = fs::read_to_string(entry.path()).map_err(|source| RunError::Io {
            path: entry.path().to_path_buf(),
            source,
        })?;
        sources.insert(rel, text);
    }
    Ok(())
}

fn has_dir_symlink(root: &Path, candidate: &Path) -> bool {
    let mut current = candidate.parent();
    while let Some(dir) = current {
        if dir == root {
            break;
        }
        if !dir.starts_with(root) {
            return true;
        }
        match fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
        current = dir.parent();
    }
    false
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
