use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
pub(super) fn owner_ok(dir_uid: u32, probe_uid: u32) -> bool {
    dir_uid == probe_uid
}
/// Refuses a directory another party could have planted or can modify, so no other user can substitute a cache.
pub(super) fn vet_existing(dir: &Path) -> Result<(), String> {
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
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if meta.permissions().mode() & 0o022 != 0 {
            return Err("it is writable by group or others".to_string());
        }
        if !owner_ok(meta.uid(), probe_uid(dir)?) {
            return Err("it is not owned by this user".to_string());
        }
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
pub(super) fn probe_uid(dir: &Path) -> Result<u32, String> {
    use std::os::unix::fs::MetadataExt;
    let probe = dir.join(format!(".probe.{}.tmp", std::process::id()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| format!("it is not writable by this user: {error}"))?;
    let uid = file
        .metadata()
        .map_err(|error| format!("cannot inspect it: {error}"))?
        .uid();
    drop(file);
    let _ = fs::remove_file(&probe);
    Ok(uid)
}

pub(super) fn create_private(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

pub(super) fn unsafe_dir(reason: String) -> io::Error {
    io::Error::other(format!(
        "refusing to use an unsafe cache directory: {reason}"
    ))
}
