use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use sloplint_core::Finding;
use sloplint_core::baseline::Baseline;

use crate::{RunError, fileio, io_error, project};

pub(crate) fn load_baseline(root: &Path, name: &str) -> Result<Baseline, RunError> {
    let path = project::resolve_within_root(root, name)?;
    let mut text = match fileio::read_bounded_string(&path, project::MAX_BASELINE_BYTES) {
        Ok(text) => text,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(RunError::BaselineMissing { path });
        }
        Err(source) => return Err(RunError::Io { path, source }),
    };
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Baseline::from_json(&text).map_err(|error| RunError::BaselineCorrupt {
        path,
        message: error.to_string(),
    })
}

/// Reads a source file of at most `max_bytes` bytes without following a trailing symlink.
///
/// Returns `Ok(None)` when the file is over the bound, or when a nofollow open refuses the
/// path as a symlink, reparse point, or FIFO, device, or socket (see
/// [`fileio::refused_without_following`]); a directory and every other open or read error
/// is returned.
pub(crate) fn read_within_limit(path: &Path, max_bytes: u64) -> io::Result<Option<Vec<u8>>> {
    let (file, len) = match fileio::open_regular_nofollow(path) {
        Ok(opened) => opened,
        Err(source) if fileio::refused_without_following(path, &source) => return Ok(None),
        Err(source) => return Err(source),
    };
    if len > max_bytes {
        return Ok(None);
    }
    let cap = usize::try_from(len.min(max_bytes.saturating_add(1))).unwrap_or(usize::MAX);
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(cap).map_err(|_| {
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("file of {len} bytes does not fit in memory"),
        )
    })?;
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok((u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= max_bytes).then_some(bytes))
}

/// Reads a whole regular file as UTF-8 text, bounded by `max_bytes`.
///
/// Opens the path without following a trailing symlink and refuses FIFOs, devices, and
/// directories; returns an error for those, when the file grows past the bound, or when
/// the content is not valid UTF-8.
/// A leading byte-order mark is removed in place when present.
pub fn read_source(path: &Path, max_bytes: u64) -> Result<String, RunError> {
    let invalid =
        |message: &'static str| io_error(path, io::Error::new(io::ErrorKind::InvalidData, message));
    let bytes = read_within_limit(path, max_bytes)
        .map_err(|source| io_error(path, source))?
        .ok_or_else(|| {
            invalid("file is not regular, or grew past max_file_bytes while being linted")
        })?;
    let mut text =
        String::from_utf8(bytes).map_err(|_| invalid("stream did not contain valid UTF-8"))?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

/// Records `findings` at `path`, replacing only a prior baseline.
///
/// An existing regular file must already parse as a version-1 baseline, checked before
/// anything is written, so a configuration or source file named as the baseline is
/// refused and left as it was. A missing target is created. A directory, FIFO, or
/// device is left to the I/O refusal of [`write_replacing`], which never reads it.
pub(crate) fn record_baseline(path: &Path, findings: &[Finding]) -> Result<(), RunError> {
    ensure_prior_baseline(path)?;
    write_replacing(path, Baseline::from_findings(findings).to_json().as_bytes())
        .map_err(|source| io_error(path, source))
}

/// Refuses an existing regular file at `path` whose content does not parse as a baseline.
///
/// Only regular files are read; a missing target or a non-regular one returns `Ok`, so
/// the write path refuses a directory or FIFO with its existing I/O error.
fn ensure_prior_baseline(path: &Path) -> Result<(), RunError> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(path, source)),
    };
    if !meta.is_file() {
        return Ok(());
    }
    let bytes = fileio::read_regular(path).map_err(|source| io_error(path, source))?;
    let parses = std::str::from_utf8(&bytes).is_ok_and(|text| Baseline::from_json(text).is_ok());
    if parses {
        Ok(())
    } else {
        Err(RunError::NotABaseline {
            path: path.to_path_buf(),
        })
    }
}

/// Replaces `path` with `bytes` so no reader or crash ever sees a partial file.
///
/// The bytes go to an exclusively created sibling file that is renamed over
/// `path`; on any failure the sibling is removed and `path` is untouched. An
/// existing target must be a regular file writable by this user (a FIFO or
/// device is refused without blocking) and keeps its permissions. The
/// name is replaced, not followed, so a symlink swapped in after
/// `resolve_within_root` cannot redirect the write.
fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let existing = fs::metadata(path).map(Some).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        }
    })?;
    if let Some(meta) = &existing {
        if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        probe_writable(path)?;
    }
    let permissions = existing.map(|meta| meta.permissions());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.{stamp}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| match permissions {
            Some(permissions) => file.set_permissions(permissions),
            None => Ok(()),
        })
        .and_then(|()| file.sync_all());
    drop(file);
    written
        .and_then(|()| fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
}

/// Probes that an existing baseline target is writable without blocking on a FIFO.
///
/// Unix opens the path nonblocking with no controlling terminal, so a FIFO swapped in
/// after the metadata check fails at once instead of waiting for a reader; the opened
/// handle is then refused unless it is a regular file. A read-only regular file still
/// fails here, preserving the read-only rejection before any temporary file is made.
#[cfg(unix)]
fn probe_writable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn probe_writable(path: &Path) -> io::Result<()> {
    fs::OpenOptions::new().write(true).open(path).map(|_| ())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn a_symlink_at_a_source_path_is_a_skip_and_its_target_is_never_read() {
        use super::read_within_limit;
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let target = outside.path().join("target.rs");
        std::fs::write(&target, "pub fn outside() {}\n").unwrap_or_else(|e| panic!("write: {e}"));
        let link = tmp.path().join("link.rs");
        std::os::unix::fs::symlink(&target, &link).unwrap_or_else(|e| panic!("symlink: {e}"));
        let refused = read_within_limit(&link, 4096).unwrap_or_else(|e| {
            panic!("a symlink at a source path must be a counted skip, not an IO error: {e}")
        });
        assert_eq!(
            refused, None,
            "a symlink must never be followed to the outside file's bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_at_a_source_path_is_a_skip() {
        use super::read_within_limit;
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let fifo = tmp.path().join("pipe.rs");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap_or_else(|e| panic!("spawn mkfifo: {e}"));
        assert!(
            status.success(),
            "mkfifo {} failed: {status}",
            fifo.display()
        );
        let refused = read_within_limit(&fifo, 4096).unwrap_or_else(|e| {
            panic!("a FIFO at a source path must be a counted skip, not an IO error: {e}")
        });
        assert_eq!(
            refused, None,
            "a FIFO at a source path must be skipped, never read"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_at_a_source_path_keeps_its_platform_error() {
        use super::read_within_limit;
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let dir = tmp.path().join("dir.rs");
        std::fs::create_dir(&dir).unwrap_or_else(|e| panic!("mkdir: {e}"));
        assert!(
            read_within_limit(&dir, 4096).is_err(),
            "a directory keeps the platform's own error, not a counted skip"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_path_the_opener_cannot_name_keeps_its_error() {
        use super::read_within_limit;
        use std::os::unix::ffi::OsStrExt;
        let unnamable = std::path::Path::new(std::ffi::OsStr::from_bytes(b"bad\0name.rs"));
        assert!(
            read_within_limit(unnamable, 4096).is_err(),
            "a path the opener cannot name is an invalid-input error, not a counted skip"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_regular_source_at_a_source_path_is_read_in_full() {
        use super::read_within_limit;
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let file = tmp.path().join("inside.rs");
        std::fs::write(&file, "pub fn inside() {}\n").unwrap_or_else(|e| panic!("write: {e}"));
        let read = read_within_limit(&file, 4096)
            .unwrap_or_else(|e| panic!("a regular source must still read: {e}"));
        assert_eq!(
            read.as_deref(),
            Some(&b"pub fn inside() {}\n"[..]),
            "the regular file must be read in full"
        );
    }
}
