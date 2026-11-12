use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sloplint_core::{
    Config, CustomRuleSpec, Finding, LanguageConfig, Strictness, Suppression, WorldFacts,
};

use crate::fileio::{Capped, Links, read_capped};
use crate::terminal::TerminalText;
#[path = "cache_dirs.rs"]
mod dirs;
use dirs::{create_private, unsafe_dir, vet_existing};

const VERSION: u32 = 9;
const NAMESPACE: &str = "sloplint";
const SHARD_DIR: &str = "cache-v2";

/// Caps one shard's read and write so a planted cache file cannot force an unbounded allocation; realistic shards stay well under 4 MiB.
const MAX_SHARD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOutput {
    pub hash: [u8; 32],
    pub parsed: bool,
    /// Stored before suppression and fingerprinting; both re-run after a cache hit.
    pub findings: Vec<Finding>,
    pub suppressions: Vec<Suppression>,
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

struct Slot {
    hash: [u8; 32],
    output: Option<FileOutput>,
}

type ShardEntries = HashMap<Box<str>, Slot>;

/// What decode_shard saw around one read; unlink_if_same deletes only while a fresh stat still matches both fields.
struct Observed {
    len: u64,
    mtime: SystemTime,
}

pub struct Loaded {
    shards: Vec<Mutex<ShardEntries>>,
    present: [bool; 256],
    entries: usize,
}

impl Loaded {
    pub fn empty() -> Self {
        Self::new((0..=255u8).map(|_| HashMap::new()).collect(), [false; 256])
    }

    fn new(shards: Vec<ShardEntries>, present: [bool; 256]) -> Self {
        Self {
            entries: shards.iter().map(HashMap::len).sum(),
            shards: shards.into_iter().map(Mutex::new).collect(),
            present,
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries
    }

    pub fn take(&self, rel: &str, hash: &[u8; 32]) -> Option<FileOutput> {
        let mut shard = self.shard(shard_byte(rel));
        let slot = shard.get_mut(rel)?;
        if slot.hash == *hash {
            slot.output.take()
        } else {
            None
        }
    }

    fn shard(&self, byte: u8) -> MutexGuard<'_, ShardEntries> {
        self.shards[usize::from(byte)]
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn same_set(&self, byte: u8, entries: Option<&[(&str, &FileOutput)]>) -> bool {
        let old = self.shard(byte);
        match entries {
            None => old.is_empty(),
            Some(entries) => {
                entries.len() == old.len()
                    && entries.iter().all(|(rel, output)| {
                        old.get(*rel).is_some_and(|slot| slot.hash == output.hash)
                    })
            }
        }
    }
}

pub fn source_hash(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

pub fn interface_hash(text: &str) -> [u8; 32] {
    blake3::derive_key("sloplint interface-shadowed source", text.as_bytes())
}

/// Per-user cache for one project root; repo contents are attacker-controlled and this key is derivable from public inputs, so a cache file inside a checkout is never trusted.
pub struct Cache {
    dir: PathBuf,
    key: [u8; 32],
}

pub fn open(
    base: Option<PathBuf>,
    root: &Path,
    config: &Config,
    custom_sources: &[(String, String)],
) -> Option<Cache> {
    let Some(base) = base else {
        warn("cache disabled: no per-user cache directory was given".to_string());
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
    /// Prunes undecodable and non-regular shard files here even on warm runs, where the store writes nothing.
    pub fn load(&self) -> Loaded {
        let shards = self.dir.join(SHARD_DIR);
        if vet_existing(&shards).is_err()
            || !fs::symlink_metadata(&shards).is_ok_and(|meta| meta.is_dir())
        {
            return Loaded::empty();
        }
        let Ok(read_dir) = fs::read_dir(&shards) else {
            return Loaded::empty();
        };
        let mut present = [false; 256];
        let mut on_disk = Vec::new();
        for entry in read_dir.flatten() {
            let Some(byte) = parse_shard_file_name(&entry.file_name()) else {
                continue;
            };
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                present[usize::from(byte)] = true;
                on_disk.push(byte);
            } else {
                let path = entry.path();
                if fs::symlink_metadata(&path).is_ok_and(|meta| !meta.is_file()) {
                    let _ = fs::remove_file(path);
                }
            }
        }
        let decoded: Vec<(u8, ShardEntries)> = on_disk
            .into_par_iter()
            .filter_map(|byte| match self.decode_shard(&shards, byte) {
                (Some(entries), _) => Some((byte, entries)),
                (None, Some(observed)) => {
                    unlink_if_same(&shards.join(shard_file(byte)), &observed);
                    None
                }
                (None, None) => None,
            })
            .collect();
        let mut shards_vec: Vec<ShardEntries> = (0..=255u8).map(|_| HashMap::new()).collect();
        for (byte, entries) in decoded {
            shards_vec[usize::from(byte)] = entries;
        }
        Loaded::new(shards_vec, present)
    }

    fn decode_shard(&self, shards: &Path, byte: u8) -> (Option<ShardEntries>, Option<Observed>) {
        let path = shards.join(shard_file(byte));
        let stat_before_read = fs::symlink_metadata(&path).ok();
        let mtime = stat_before_read
            .as_ref()
            .and_then(|meta| meta.modified().ok());
        let Ok(Capped::Bytes(bytes, _)) = read_capped(&path, MAX_SHARD_BYTES, Links::NoFollow)
        else {
            let len = stat_before_read
                .as_ref()
                .map(|meta| meta.len())
                .unwrap_or(0);
            return (None, mtime.map(|mtime| Observed { len, mtime }));
        };
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let observed = mtime.map(|mtime| Observed { len, mtime });
        let Some((digest, payload)) = bytes.split_first_chunk::<32>() else {
            return (None, observed);
        };
        if blake3::hash(payload).as_bytes() != digest {
            return (None, observed);
        }
        let Ok(shard) = postcard::from_bytes::<Shard>(payload) else {
            return (None, observed);
        };
        if shard.version != VERSION || shard.config_key != self.key {
            return (None, observed);
        }
        let entries: ShardEntries = shard
            .files
            .into_iter()
            .map(|(rel, output)| {
                let hash = output.hash;
                (
                    rel,
                    Slot {
                        hash,
                        output: Some(output),
                    },
                )
            })
            .collect();
        (Some(entries), observed)
    }

    pub fn store(&self, files: &[(&str, &FileOutput)], old: &Loaded) {
        if let Err(error) = self.write_shards(files, old) {
            warn(format!(
                "cache not written in {}: {error}",
                self.dir.display()
            ));
        }
    }

    fn write_shards(&self, files: &[(&str, &FileOutput)], old: &Loaded) -> io::Result<()> {
        let mut fresh: HashMap<u8, Vec<(&str, &FileOutput)>> = HashMap::new();
        for &(rel, output) in files {
            fresh
                .entry(shard_byte(rel))
                .or_default()
                .push((rel, output));
        }
        let shards = self.dir.join(SHARD_DIR);
        let dirty: Vec<u8> = (0..=255u8)
            .filter(|&byte| old.present[usize::from(byte)] || fresh.contains_key(&byte))
            .filter(|&byte| {
                let entries = fresh.get(&byte);
                let present = old.present[usize::from(byte)];
                present != entries.is_some() || !old.same_set(byte, entries.map(Vec::as_slice))
            })
            .collect();
        if dirty.is_empty() {
            return Ok(());
        }
        self.write_dirty(&shards, &fresh, &dirty)
    }

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
        let errors = dirty
            .par_iter()
            .filter_map(|&byte| self.write_dirty_shard(shards, fresh, byte).err())
            .collect::<Vec<io::Error>>();
        errors.into_iter().next().map_or(Ok(()), Err)
    }

    fn write_dirty_shard(
        &self,
        shards: &Path,
        fresh: &HashMap<u8, Vec<(&str, &FileOutput)>>,
        byte: u8,
    ) -> io::Result<()> {
        let path = shards.join(shard_file(byte));
        match fresh.get(&byte) {
            Some(entries) => self.write_shard(&path, entries),
            None => fs::remove_file(&path).or_else(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            }),
        }
    }

    /// A link swapped into a directory after its creation is re-vetted away before any write.
    fn write_shard(&self, path: &Path, entries: &[(&str, &FileOutput)]) -> io::Result<()> {
        let bytes = postcard::to_allocvec(&ShardWrite {
            version: VERSION,
            config_key: &self.key,
            files: entries,
        })
        .map_err(io::Error::other)?;
        if u64::try_from(bytes.len().saturating_add(32)).unwrap_or(u64::MAX) > MAX_SHARD_BYTES {
            return Err(io::Error::other(format!(
                "shard of {} bytes exceeds the {MAX_SHARD_BYTES}-byte cap",
                bytes.len()
            )));
        }
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
            if let Err(error) = stamp_and_sync(&mut file, &bytes) {
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

fn stamp_and_sync(file: &mut fs::File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(blake3::hash(bytes).as_bytes())
        .and_then(|()| file.write_all(bytes))
        .and_then(|()| file.sync_all())
}

/// Leaves a shard a concurrent writer replaced: an identical-length, identical-mtime swap costs one cold rebuild, never a wrong result.
fn unlink_if_same(path: &Path, observed: &Observed) {
    let same = fs::symlink_metadata(path).is_ok_and(|meta| {
        meta.is_file()
            && meta.len() == observed.len
            && meta.modified().is_ok_and(|mtime| mtime == observed.mtime)
    });
    if same {
        let _ = fs::remove_file(path);
    }
}

fn shard_byte(rel: &str) -> u8 {
    blake3::hash(rel.as_bytes()).as_bytes()[0]
}

fn shard_file(byte: u8) -> String {
    format!("shard-{byte:02x}.bin")
}

fn parse_shard_file_name(name: &std::ffi::OsStr) -> Option<u8> {
    let hex = name
        .to_str()?
        .strip_prefix("shard-")?
        .strip_suffix(".bin")?;
    (hex.len() == 2
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then(|| u8::from_str_radix(hex, 16).ok())?
}

#[derive(Serialize)]
struct AnalysisKey<'a> {
    strictness: Strictness,
    languages: &'a [String],
    language_tables: &'a BTreeMap<String, LanguageConfig>,
    exclude: &'a [String],
    extend_exclude: &'a [String],
    test_globs: &'a [String],
    bench_globs: &'a [String],
    example_globs: &'a [String],
    support_globs: &'a [String],
    entry_globs: &'a [String],
    data_globs: &'a [String],
    max_file_bytes: u64,
    rules: &'a BTreeMap<String, toml::Table>,
    custom_rules: &'a [CustomRuleSpec],
}

fn analysis_key(config: &Config) -> AnalysisKey<'_> {
    let project = &config.project;
    AnalysisKey {
        strictness: project.strictness,
        languages: &project.languages,
        language_tables: &config.languages,
        exclude: &project.exclude,
        extend_exclude: &project.extend_exclude,
        test_globs: &project.test_globs,
        bench_globs: &project.bench_globs,
        example_globs: &project.example_globs,
        support_globs: &project.support_globs,
        entry_globs: &project.entry_globs,
        data_globs: &project.data_globs,
        max_file_bytes: project.max_file_bytes,
        rules: &config.rules,
        custom_rules: &config.custom_rules,
    }
}
fn config_key(config: &Config, custom_sources: &[(String, String)]) -> io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap_rayon(std::env::current_exe()?)?;
    hasher.update(&sonic_rs::to_vec(&analysis_key(config)).map_err(io::Error::other)?);
    for (rel, text) in custom_sources {
        for part in [rel, text] {
            hasher.update(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
            hasher.update(part.as_bytes());
        }
    }
    Ok(*hasher.finalize().as_bytes())
}

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

/// Warning text embeds untrusted paths and OS errors, so the whole line is escaped for the terminal.
fn warn(message: String) {
    let _ = writeln!(
        io::stderr(),
        "sloplint: warning: {}",
        TerminalText::line(&message)
    );
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
