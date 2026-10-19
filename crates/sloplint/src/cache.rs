use std::collections::HashMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sloplint_core::{Config, Finding, Suppression, WorldFacts};

use crate::project::{read_regular, read_regular_nofollow};
use crate::terminal::TerminalText;

const VERSION: u32 = 1;
const NAMESPACE: &str = "sloplint";
const FILE: &str = "cache-v1.bin";

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
    /// Loads the cached outputs keyed by relative path; any mismatch or read error yields an empty cache.
    ///
    /// The cache file itself is never followed through a symlink or reparse point; only its
    /// vetted directories are trusted. Configuration and baseline inputs keep following
    /// symlinks to regular files.
    pub fn load(&self) -> HashMap<Box<str>, FileOutput> {
        let Ok(bytes) = read_regular_nofollow(&self.dir.join(FILE)) else {
            return HashMap::new();
        };
        match postcard::from_bytes::<CacheFile>(&bytes) {
            Ok(file) if file.version == VERSION && file.config_key == self.key => {
                file.files.into_iter().collect()
            }
            _ => HashMap::new(),
        }
    }

    /// Writes the cache atomically; a failure warns on stderr and never fails the run.
    pub fn store(&self, files: &[(&str, &FileOutput)]) {
        if let Err(error) = self.write(files) {
            warn(format!(
                "cache not written in {}: {error}",
                self.dir.display()
            ));
        }
    }

    /// Creates the private directories, then writes through a per-process temporary file and rename.
    ///
    /// Directories are created `0700` on unix. They are vetted again after
    /// creation so a link swapped in between cannot be used, and the
    /// temporary file is created exclusively, so a pre-planted symlink at its
    /// path is left alone instead of being followed. Only the owned temporary
    /// file is renamed or removed, after its handle is closed.
    fn write(&self, files: &[(&str, &FileOutput)]) -> io::Result<()> {
        create_private(&self.dir)?;
        if let Some(namespace) = self.dir.parent() {
            vet_existing(namespace).map_err(unsafe_dir)?;
        }
        vet_existing(&self.dir).map_err(unsafe_dir)?;
        let bytes = postcard::to_allocvec(&CacheWrite {
            version: VERSION,
            config_key: &self.key,
            files,
        })
        .map_err(io::Error::other)?;
        let tmp = self.dir.join(format!("{FILE}.{}.tmp", std::process::id()));
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            if let Err(error) = file.write_all(&bytes) {
                drop(file);
                let _ = fs::remove_file(&tmp);
                return Err(error);
            }
        }
        fs::rename(&tmp, self.dir.join(FILE)).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

/// Hashes everything that shapes per-file output: the binary bytes, the effective config, and custom rule files.
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
