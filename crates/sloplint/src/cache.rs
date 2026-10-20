use std::collections::HashMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sloplint_core::{Config, Finding, Suppression, WorldFacts};

use crate::project::{read_regular, read_regular_nofollow};
use crate::terminal::TerminalText;

const VERSION: u32 = 2;
const NAMESPACE: &str = "sloplint";
/// Directory holding the per-shard cache files `shard-<NN>.bin`.
const SHARD_DIR: &str = "cache-v2";

/// Everything one analyzed file contributes, reproduced verbatim on a cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOutput {
    /// Content hash the output was computed from.
    pub hash: [u8; 32],
    /// Whether the file parsed. A file with a syntax error carries only its
    /// `meta/parse-error` finding, so its suppressions cannot be judged unused.
    pub parsed: bool,
    /// Findings before suppression and fingerprinting.
    pub findings: Vec<Finding>,
    /// Suppression directives found in the file.
    pub suppressions: Vec<Suppression>,
    /// The file's contribution to cross-file rules.
    pub world: WorldFacts,
}

#[derive(Deserialize)]
struct Shard {
    version: u32,
    config_key: [u8; 32],
    files: Vec<(Box<str>, FileOutput)>,
}

#[derive(Serialize)]
struct ShardWrite<'a> {
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

/// The per-user cache of one project root.
///
/// The cache lives in the user's cache directory, never inside the checkout:
/// everything a repository can contain is attacker-controlled, and the key
/// below is computable from public inputs, so a cache file shipped in a tree
/// must never be trusted. Files under the checkout's `.sloplint` are not read,
/// written, or removed.
pub struct Cache {
    dir: PathBuf,
    key: [u8; 32],
}

/// Resolves, vets, and keys the cache for `root` under the user's cache directory `base`, or warns and returns `None`.
///
/// A cache is an optimization, so every problem here disables it instead of
/// failing the run. Nothing is created: directories that do not exist yet are
/// made by [`Cache::store`].
pub fn open(
    base: Option<PathBuf>,
    root: &Path,
    config: &Config,
    custom_sources: &[(String, String)],
) -> Option<Cache> {
    let Some(base) = base else {
        warn(
            "cache disabled: no per-user cache directory (set an absolute XDG_CACHE_HOME, or HOME / LOCALAPPDATA)"
                .to_string(),
        );
        return None;
    };
    let namespace = base.join(NAMESPACE);
    let dir = namespace.join(root_key(root));
    if inside(&dir, root) {
        warn(format!(
            "cache disabled: cache directory {} is inside the project root {}",
            dir.display(),
            root.display()
        ));
        return None;
    }
    for part in [&namespace, &dir] {
        if let Err(reason) = vet_existing(part) {
            warn(format!(
                "cache disabled: cache directory {} is unsafe: {reason}",
                part.display()
            ));
            return None;
        }
    }
    match config_key(config, custom_sources) {
        Ok(key) => Some(Cache { dir, key }),
        Err(error) => {
            warn(format!(
                "cache disabled: cannot read the running executable: {error}"
            ));
            None
        }
    }
}

impl Cache {
    /// Loads the cached outputs keyed by relative path from every shard present.
    ///
    /// Shards are read and decoded in parallel on the current rayon pool.
    /// A shard that cannot be read, fails to parse, or carries the wrong
    /// version or key contributes zero entries and never aborts the load. A
    /// symlinked shard directory yields an empty cache. No cache file is ever
    /// followed through a symlink or reparse point; only its vetted
    /// directories are trusted. Configuration and baseline inputs keep
    /// following symlinks to regular files.
    pub fn load(&self) -> HashMap<Box<str>, FileOutput> {
        let shards = self.dir.join(SHARD_DIR);
        if !fs::symlink_metadata(&shards).is_ok_and(|meta| meta.is_dir()) {
            return HashMap::new();
        }
        (0..=255u8)
            .into_par_iter()
            .filter_map(|byte| {
                let bytes = read_regular_nofollow(&shards.join(shard_file(byte))).ok()?;
                let shard = postcard::from_bytes::<Shard>(&bytes).ok()?;
                (shard.version == VERSION && shard.config_key == self.key).then_some(shard.files)
            })
            .flatten_iter()
            .collect()
    }

    /// Writes the cache atomically; a failure warns on stderr and never fails the run.
    pub fn store(&self, files: &[(&str, &FileOutput)], old_hashes: &HashMap<Box<str>, [u8; 32]>) {
        if let Err(error) = self.write_shards(files, old_hashes) {
            warn(format!(
                "cache not written in {}: {error}",
                self.dir.display()
            ));
        }
    }

    /// Rewrites or removes every shard whose file disagrees with this run's entries.
    ///
    /// A shard is settled when its file is present exactly when it has
    /// entries and its rel set and content hashes equal the loaded ones, so a
    /// warm run rewrites nothing. Every other shard is dirty: one with entries
    /// is rewritten, one without has its file removed. That covers deleted
    /// files and also stale-key or corrupt shards that loaded no entries: the
    /// store is the only prune.
    fn write_shards(
        &self,
        files: &[(&str, &FileOutput)],
        old_hashes: &HashMap<Box<str>, [u8; 32]>,
    ) -> io::Result<()> {
        let mut fresh: HashMap<u8, Vec<(&str, &FileOutput)>> = HashMap::new();
        for &(rel, output) in files {
            fresh
                .entry(shard_byte(rel))
                .or_default()
                .push((rel, output));
        }
        let mut stale: HashMap<u8, Vec<&str>> = HashMap::new();
        for rel in old_hashes.keys() {
            let rel: &str = rel;
            stale.entry(shard_byte(rel)).or_default().push(rel);
        }
        let shards = self.dir.join(SHARD_DIR);
        let dirty: Vec<u8> = (0..=255u8)
            .filter(|&byte| {
                let entries = fresh.get(&byte);
                let present = fs::symlink_metadata(shards.join(shard_file(byte)))
                    .is_ok_and(|meta| meta.is_file());
                present != entries.is_some()
                    || !same_set(
                        entries.map(Vec::as_slice),
                        stale.get(&byte).map(Vec::as_slice),
                        old_hashes,
                    )
            })
            .collect();
        if dirty.is_empty() {
            return Ok(());
        }
        self.write_dirty(&shards, &fresh, &dirty)
    }

    /// Creates the private directories and rewrites or removes each dirty shard.
    ///
    /// Every dirty shard is attempted even after one fails; the first error is returned.
    fn write_dirty(
        &self,
        shards: &Path,
        fresh: &HashMap<u8, Vec<(&str, &FileOutput)>>,
        dirty: &[u8],
    ) -> io::Result<()> {
        create_private(&self.dir)?;
        if let Some(namespace) = self.dir.parent() {
            vet_existing(namespace).map_err(unsafe_dir)?;
        }
        vet_existing(&self.dir).map_err(unsafe_dir)?;
        create_private(shards)?;
        vet_existing(shards).map_err(unsafe_dir)?;
        let mut first_error: Option<io::Error> = None;
        for &byte in dirty {
            let path = shards.join(shard_file(byte));
            let result = match fresh.get(&byte) {
                Some(entries) => self.write_shard(&path, entries),
                None => fs::remove_file(&path).or_else(|error| {
                    if error.kind() == io::ErrorKind::NotFound {
                        Ok(())
                    } else {
                        Err(error)
                    }
                }),
            };
            if let Err(error) = result
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Writes one shard through a per-process temporary file and rename.
    ///
    /// Directories are created `0700` on unix (see [`Cache::write_shards`]).
    /// They are vetted again after creation so a link swapped in between
    /// cannot be used, and the temporary file is created exclusively, so a
    /// pre-planted symlink at its path is left alone instead of being
    /// followed. Only the owned temporary file is renamed or removed, after
    /// its handle is closed.
    fn write_shard(&self, path: &Path, entries: &[(&str, &FileOutput)]) -> io::Result<()> {
        let bytes = postcard::to_allocvec(&ShardWrite {
            version: VERSION,
            config_key: &self.key,
            files: entries,
        })
        .map_err(io::Error::other)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("shard.bin");
        let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
        {
            let mut file = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
            {
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && fs::symlink_metadata(&tmp).is_ok_and(|meta| meta.is_file()) =>
                {
                    let _ = fs::remove_file(&tmp);
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&tmp)?
                }
                other => other?,
            };
            if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&tmp);
                return Err(error);
            }
        }
        fs::rename(&tmp, path).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

/// The shard holding `rel`: the first BLAKE3 byte of the relative path.
fn shard_byte(rel: &str) -> u8 {
    blake3::hash(rel.as_bytes()).as_bytes()[0]
}

/// File name of shard `byte`, e.g. `shard-3f.bin`: two lowercase hex chars.
fn shard_file(byte: u8) -> String {
    format!("shard-{byte:02x}.bin")
}

/// Whether a shard's new entries name exactly the loaded rels with the same content hashes.
fn same_set(
    entries: Option<&[(&str, &FileOutput)]>,
    rels: Option<&[&str]>,
    old_hashes: &HashMap<Box<str>, [u8; 32]>,
) -> bool {
    match (entries, rels) {
        (None, None) => true,
        (Some(entries), Some(rels)) => {
            entries.len() == rels.len()
                && entries.iter().all(|(rel, output)| {
                    old_hashes
                        .get(*rel)
                        .is_some_and(|hash| *hash == output.hash)
                })
        }
        _ => false,
    }
}

/// Hashes everything that shapes per-file output: the binary bytes, the effective config, and custom rule files.
///
/// The binary bytes hold every grammar, adapter, and parse choice, including the retries of a
/// failed `.h` parse as C++ and then Objective-C, so a changed grammar or fallback changes the
/// key and no entry stored by another binary is read. Within one binary a path fixes its
/// routed language and the bytes fix the retry outcome, so the path and content hash alone
/// identify an entry.
fn config_key(config: &Config, custom_sources: &[(String, String)]) -> io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let exe = std::env::current_exe()?;
    hasher.update(&read_regular(&exe)?);
    hasher.update(&sonic_rs::to_vec(config).map_err(io::Error::other)?);
    for (rel, text) in custom_sources {
        hasher.update(rel.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(text.as_bytes());
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Names a project's directory by the canonical root path: 32 lowercase hex characters.
fn root_key(root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        hasher.update(root.as_os_str().as_bytes());
    }
    #[cfg(not(unix))]
    hasher.update(root.to_string_lossy().as_bytes());
    hasher.finalize().to_hex().as_str()[..32].to_string()
}

/// Whether `dir`, resolved through its nearest existing ancestor, lies under `root`.
fn inside(dir: &Path, root: &Path) -> bool {
    let mut missing = Vec::new();
    let mut probe = dir;
    loop {
        if let Ok(real) = fs::canonicalize(probe) {
            let resolved = missing
                .iter()
                .rev()
                .fold(real, |path, part| path.join(part));
            return resolved.starts_with(root);
        }
        match (probe.parent(), probe.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                probe = parent;
            }
            _ => return false,
        }
    }
}

/// Refuses an existing directory that another party could have planted or can modify.
///
/// A missing path is fine. A symlink is never followed, so a link cannot
/// redirect the cache. On unix the directory must also not be writable by
/// group or others and must accept a file from this user; together those mean
/// only its owner (or root) can change it, so no other user can substitute a
/// cache.
fn vet_existing(dir: &Path) -> Result<(), String> {
    let meta = match fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot inspect it: {error}")),
    };
    if meta.file_type().is_symlink() {
        return Err("it is a symlink".to_string());
    }
    if !meta.is_dir() {
        return Err("it is not a directory".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err("it is writable by group or others".to_string());
        }
        probe_writable(dir)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & REPARSE_POINT != 0 {
            return Err("it is a reparse point".to_string());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn probe_writable(dir: &Path) -> Result<(), String> {
    let probe = dir.join(format!(".probe.{}.tmp", std::process::id()));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(error) => Err(format!("it is not writable by this user: {error}")),
    }
}

fn create_private(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

fn unsafe_dir(reason: String) -> io::Error {
    io::Error::other(format!(
        "refusing to use an unsafe cache directory: {reason}"
    ))
}

/// Prints one warning line; paths and OS error text are untrusted, so the whole message is escaped for the terminal.
fn warn(message: String) {
    let _ = writeln!(
        io::stderr(),
        "sloplint: warning: {}",
        TerminalText::line(&message)
    );
}
