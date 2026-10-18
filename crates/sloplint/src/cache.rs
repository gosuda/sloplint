use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sloplint_core::{Config, Finding, Suppression, WorldFacts};

const VERSION: u32 = 1;
const DIR: &str = ".sloplint";
const FILE: &str = "cache-v1.bin";

/// Everything one analyzed file contributes, reproduced verbatim on a cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOutput {
    /// Content hash the output was computed from.
    pub hash: [u8; 32],
    /// Findings before suppression and fingerprinting.
    pub findings: Vec<Finding>,
    /// Suppression directives found in the file.
    pub suppressions: Vec<Suppression>,
    /// The file's contribution to cross-file rules.
    pub world: WorldFacts,
}

#[derive(Deserialize)]
struct CacheFile {
    version: u32,
    config_key: [u8; 32],
    files: Vec<(Box<str>, FileOutput)>,
}

#[derive(Serialize)]
struct CacheWrite<'a> {
    version: u32,
    config_key: &'a [u8; 32],
    files: &'a [(&'a str, &'a FileOutput)],
}

/// Hashes a file's BOM-stripped source; this is the cache's content key.
pub fn source_hash(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

/// Hashes an OCaml implementation with its sibling interface, whose presence changes the file's facts.
pub fn interface_hash(text: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(text.as_bytes());
    hasher.update(b"\0interface");
    *hasher.finalize().as_bytes()
}

/// Hashes everything that shapes per-file output: the tool version, the effective config, and custom rule files.
pub fn config_key(
    config: &Config,
    custom_sources: &[(String, String)],
) -> Result<[u8; 32], serde_json::Error> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(&serde_json::to_vec(config)?);
    for (rel, text) in custom_sources {
        hasher.update(rel.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(text.as_bytes());
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Loads the cached outputs keyed by relative path; any mismatch or read error yields an empty cache.
pub fn load(root: &Path, key: &[u8; 32]) -> HashMap<Box<str>, FileOutput> {
    let Ok(bytes) = fs::read(cache_path(root)) else {
        return HashMap::new();
    };
    match postcard::from_bytes::<CacheFile>(&bytes) {
        Ok(file) if file.version == VERSION && &file.config_key == key => {
            file.files.into_iter().collect()
        }
        _ => HashMap::new(),
    }
}

/// Writes the cache atomically via a per-process temporary file and rename.
pub fn store(root: &Path, key: &[u8; 32], files: &[(&str, &FileOutput)]) -> io::Result<()> {
    let dir = root.join(DIR);
    fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        fs::write(&ignore, "*\n")?;
    }
    let bytes = postcard::to_allocvec(&CacheWrite {
        version: VERSION,
        config_key: key,
        files,
    })
    .map_err(io::Error::other)?;
    let tmp = dir.join(format!("{FILE}.{}.tmp", std::process::id()));
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, dir.join(FILE)).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(DIR).join(FILE)
}
