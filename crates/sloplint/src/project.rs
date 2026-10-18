use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
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
    /// Configuration file, which may be missing.
    pub config_path: PathBuf,
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
pub fn locate(cwd: &Path, root: Option<&Path>, config: Option<&Path>) -> Location {
    let config_path = config.map(|path| cwd.join(path));
    match (root, config_path) {
        (Some(root), config_path) => Location {
            config_path: config_path.unwrap_or_else(|| root.join(CONFIG_FILE)),
            root: root.to_path_buf(),
        },
        (None, Some(config_path)) => Location {
            root: config_path
                .parent()
                .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf),
            config_path,
        },
        (None, None) => {
            let root = find_root(cwd);
            Location {
                config_path: root.join(CONFIG_FILE),
                root,
            }
        }
    }
}

/// Reads and merges the configuration, including every `custom_rule_files` match, then applies overrides.
pub fn load(location: &Location, overrides: Overrides) -> Result<Loaded, RunError> {
    let main = read_optional(&location.config_path)?;
    let origin = origin_name(&location.config_path);
    let custom_sources = read_custom_sources(&location.root, &custom_rule_patterns(&main))?;
    let mut specs = Vec::new();
    for (rel, text) in &custom_sources {
        let file: CustomRuleFile = toml::from_str(text).map_err(|error| ConfigError::Toml {
            origin: rel.clone(),
            message: error.message().to_string(),
        })?;
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

/// Resolves the PATH arguments to root-relative scopes; an empty result means the whole root.
pub fn scopes(root: &Path, cwd: &Path, paths: &[PathBuf]) -> Result<Vec<String>, RunError> {
    let mut scopes = Vec::new();
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            cwd.join(path)
        };
        if !absolute.exists() {
            return Err(RunError::PathNotFound { path: path.clone() });
        }
        let canonical = fs::canonicalize(&absolute).map_err(|source| RunError::Io {
            path: absolute.clone(),
            source,
        })?;
        let relative = canonical
            .strip_prefix(root)
            .map_err(|_| RunError::PathOutsideRoot { path: path.clone() })?;
        let rel = normalize_rel_path(relative).ok_or_else(|| RunError::Io {
            path: path.clone(),
            source: io::Error::new(io::ErrorKind::InvalidData, "path is not UTF-8"),
        })?;
        if rel.is_empty() {
            return Ok(Vec::new());
        }
        scopes.push(rel);
    }
    Ok(scopes)
}

/// Whether `path` lies under one of the scopes; empty scopes cover everything.
pub fn in_scope(scopes: &[String], path: &str) -> bool {
    scopes.is_empty()
        || scopes.iter().any(|scope| {
            path == scope
                || (path.starts_with(scope.as_str())
                    && path.as_bytes().get(scope.len()) == Some(&b'/'))
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

fn read_optional(path: &Path) -> Result<String, RunError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(RunError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
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
    let mut sources = Vec::new();
    for result in discover::walk(root, GlobSet::empty()) {
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
        if matcher.is_match(rel.as_str()) {
            let text = fs::read_to_string(entry.path()).map_err(|source| RunError::Io {
                path: entry.path().to_path_buf(),
                source,
            })?;
            sources.push((rel, text));
        }
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(sources)
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
