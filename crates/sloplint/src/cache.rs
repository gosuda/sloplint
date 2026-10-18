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

/// Hashes everything that shapes per-file output: the binary bytes, the effective config, and custom rule files.
pub fn config_key(config: &Config, custom_sources: &[(String, String)]) -> io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let exe = std::env::current_exe()?;
    hasher.update(&fs::read(&exe)?);
    hasher.update(&sonic_rs::to_vec(config).map_err(io::Error::other)?);
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
///
/// A symlinked `.sloplint` refuses the write so an ordinary check never
/// stores outside the root. The directory is checked twice for distinct
/// reasons: before creation, so `create_dir_all` cannot materialize a
/// dangling link's outside target; after creation, so a link swapped in
/// between cannot be used. `.gitignore` and the temporary file are created
/// exclusively, so a pre-planted symlink at either path is left alone
/// instead of being followed. Only the owned temporary file is renamed or
/// removed, after its handle is closed.
pub fn store(root: &Path, key: &[u8; 32], files: &[(&str, &FileOutput)]) -> io::Result<()> {
    let dir = root.join(DIR);
    if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::other(format!(
            "refusing to write cache through symlink {}",
            dir.display()
        )));
    }
    fs::create_dir_all(&dir)?;
    if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::other(format!(
            "refusing to write cache through symlink {}",
            dir.display()
        )));
    }
    let ignore = dir.join(".gitignore");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ignore)
    {
        Ok(mut file) => {
            use std::io::Write as _;
            file.write_all(b"*\n")?;
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let bytes = postcard::to_allocvec(&CacheWrite {
        version: VERSION,
        config_key: key,
        files,
    })
    .map_err(io::Error::other)?;
    let tmp = dir.join(format!("{FILE}.{}.tmp", std::process::id()));
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        use std::io::Write as _;
        if let Err(error) = file.write_all(&bytes) {
            drop(file);
            let _ = fs::remove_file(&tmp);
            return Err(error);
        }
    }
    fs::rename(&tmp, dir.join(FILE)).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(DIR).join(FILE)
}
