use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::error::{RunError, io_error};
use crate::{fileio, project};
use sloplint_core::Finding;
use sloplint_core::baseline::{Baseline, MAX_BASELINE_ENTRIES, SkippedEntry};

pub(crate) fn load_baseline(root: &Path, name: &str) -> Result<Baseline, RunError> {
    let path = project::resolve_within_root(root, name)?;
    let mut text =
        match fileio::read_capped_string(&path, project::MAX_BASELINE_BYTES, fileio::Links::Follow)
        {
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

pub fn read_source(path: &Path, max_bytes: u64) -> Result<String, RunError> {
    let invalid =
        |message: &'static str| io_error(path, io::Error::new(io::ErrorKind::InvalidData, message));
    let bytes = match fileio::read_capped(path, max_bytes, fileio::Links::NoFollow) {
        Ok(fileio::Capped::Bytes(bytes, _)) => bytes,
        Ok(_) => {
            return Err(invalid(
                "file is not regular, or grew past max_file_bytes while being linted",
            ));
        }
        Err(source) => return Err(io_error(path, source)),
    };
    let mut text =
        String::from_utf8(bytes).map_err(|_| invalid("stream did not contain valid UTF-8"))?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

pub(crate) fn record_baseline(
    path: &Path,
    findings: &[Finding],
    skipped: &[SkippedEntry],
) -> Result<(), RunError> {
    ensure_prior_baseline(path)?;
    let baseline = Baseline::from_findings_and_skipped(findings, skipped);
    let json = baseline.to_json();
    validate_baseline_limits(
        path,
        baseline
            .entries
            .len()
            .saturating_add(baseline.skipped.len()),
        json.len(),
        MAX_BASELINE_ENTRIES,
        project::MAX_BASELINE_BYTES,
    )?;
    write_replacing(path, json.as_bytes()).map_err(|source| io_error(path, source))
}

fn validate_baseline_limits(
    path: &Path,
    entries: usize,
    bytes: usize,
    max_entries: usize,
    max_bytes: u64,
) -> Result<(), RunError> {
    if entries > max_entries {
        return Err(RunError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("baseline has {entries} entries, more than the limit of {max_entries}"),
            ),
        });
    }
    let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
    if bytes > max_bytes {
        return Err(RunError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("baseline of {bytes} bytes exceeds ceiling of {max_bytes} bytes"),
            ),
        });
    }
    Ok(())
}

fn ensure_prior_baseline(path: &Path) -> Result<(), RunError> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(path, source)),
    };
    if !meta.is_file() {
        return Ok(());
    }
    let refused = || RunError::NotABaseline {
        path: path.to_path_buf(),
    };
    let bytes =
        match fileio::read_capped(path, project::MAX_BASELINE_BYTES, fileio::Links::NoFollow) {
            Ok(fileio::Capped::Bytes(bytes, _)) => bytes,
            Ok(_) => return Err(refused()),
            Err(source) => return Err(io_error(path, source)),
        };
    let parses = std::str::from_utf8(&bytes)
        .map(|text| text.strip_prefix('\u{feff}').unwrap_or(text))
        .is_ok_and(|text| Baseline::from_json(text).is_ok());
    if parses { Ok(()) } else { Err(refused()) }
}

// sync_all before the rename: a crash never exposes a partially written baseline.
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

// O_NONBLOCK | O_NOCTTY: a FIFO swapped in after the metadata check fails at once instead of blocking on a reader.
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
        use crate::fileio::{Capped, Links, read_capped};
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let target = outside.path().join("target.rs");
        std::fs::write(&target, "pub fn outside() {}\n").unwrap_or_else(|e| panic!("write: {e}"));
        let link = tmp.path().join("link.rs");
        std::os::unix::fs::symlink(&target, &link).unwrap_or_else(|e| panic!("symlink: {e}"));
        let refused = read_capped(&link, 4096, Links::NoFollow).unwrap_or_else(|e| {
            panic!("a symlink at a source path must be a counted skip, not an IO error: {e}")
        });
        assert!(
            matches!(refused, Capped::NotRegular(_)),
            "a symlink must never be followed to the outside file's bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_at_a_source_path_is_a_skip() {
        use crate::fileio::{Capped, Links, read_capped};
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
        let refused = read_capped(&fifo, 4096, Links::NoFollow).unwrap_or_else(|e| {
            panic!("a FIFO at a source path must be a counted skip, not an IO error: {e}")
        });
        assert!(
            matches!(refused, Capped::NotRegular(_)),
            "a FIFO at a source path must be skipped, never read"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_at_a_source_path_keeps_its_platform_error() {
        use crate::fileio::{Links, read_capped};
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let dir = tmp.path().join("dir.rs");
        std::fs::create_dir(&dir).unwrap_or_else(|e| panic!("mkdir: {e}"));
        assert!(
            read_capped(&dir, 4096, Links::NoFollow).is_err(),
            "a directory keeps the platform's own error, not a counted skip"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_path_the_opener_cannot_name_keeps_its_error() {
        use crate::fileio::{Links, read_capped};
        use std::os::unix::ffi::OsStrExt;
        let unnamable = std::path::Path::new(std::ffi::OsStr::from_bytes(b"bad\0name.rs"));
        assert!(
            read_capped(unnamable, 4096, Links::NoFollow).is_err(),
            "a path the opener cannot name is an invalid-input error, not a counted skip"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_regular_source_at_a_source_path_is_read_in_full() {
        use crate::fileio::{Capped, Links, read_capped};
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let file = tmp.path().join("inside.rs");
        std::fs::write(&file, "pub fn inside() {}\n").unwrap_or_else(|e| panic!("write: {e}"));
        let read = read_capped(&file, 4096, Links::NoFollow)
            .unwrap_or_else(|e| panic!("a regular source must still read: {e}"));
        let Capped::Bytes(bytes, _) = read else {
            panic!("a regular source must still read in full");
        };
        assert_eq!(
            bytes, b"pub fn inside() {}\n",
            "the regular file must be read in full"
        );
    }

    #[test]
    fn a_baseline_document_at_the_limits_is_accepted_and_past_them_refused() {
        use super::validate_baseline_limits;
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = tmp.path().join("baseline.json");
        let (max_entries, max_bytes) = (2usize, 64u64);
        validate_baseline_limits(&path, max_entries, 64, max_entries, max_bytes)
            .unwrap_or_else(|e| panic!("a document at both limits must record: {e}"));
        let over_entries =
            validate_baseline_limits(&path, max_entries + 1, 8, max_entries, max_bytes)
                .expect_err("past the entry limit must be refused");
        assert!(
            format!("{over_entries:?}").contains("FileTooLarge"),
            "the entry refusal must be a ceiling error: {over_entries:?}"
        );
        let over_bytes = validate_baseline_limits(&path, 1, 65, max_entries, max_bytes)
            .expect_err("past the byte ceiling must be refused");
        assert!(
            format!("{over_bytes:?}").contains("FileTooLarge"),
            "the byte refusal must be a ceiling error: {over_bytes:?}"
        );
    }

    #[test]
    fn a_recorded_baseline_roundtrips_through_the_load_parser_and_deduplicates() {
        use super::{Baseline, record_baseline};
        use sloplint_core::finding::{Confidence, Finding, Severity};
        let finding = Finding {
            rule: "generic-name".into(),
            severity: Severity::Minor,
            confidence: Confidence::Medium,
            path: "first.rs".into(),
            line: 1,
            column: 7,
            message: "rename handle".to_string(),
            fix: String::new(),
            context: "fn handle".into(),
            fingerprint: "0123456789abcdef0123456789abcdef".into(),
            context_pending: false,
            file_wide: false,
        };
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = tmp.path().join("baseline.json");
        record_baseline(&path, &[finding.clone(), finding.clone()], &[])
            .unwrap_or_else(|e| panic!("record: {e}"));
        let stored = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read: {e}"));
        let parsed = Baseline::from_json(&stored)
            .unwrap_or_else(|e| panic!("the recorded baseline must reload: {e}"));
        assert_eq!(parsed.entries.len(), 1, "identical identities deduplicate");
        assert_eq!(
            parsed,
            Baseline::from_findings(&[finding]),
            "the recorded document must roundtrip through the load parser"
        );
        record_baseline(&path, &[], &[]).unwrap_or_else(|e| panic!("re-record: {e}"));
        let replaced = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read: {e}"));
        assert!(
            Baseline::from_json(&replaced)
                .unwrap_or_else(|e| panic!("the replacement must reload: {e}"))
                .entries
                .is_empty(),
            "re-recording must replace the prior baseline"
        );
    }
}
