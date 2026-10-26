use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Opens `path` only when it is a regular file, returning the handle and its size.
///
/// On unix the open itself is non-blocking and never adopts a terminal as the
/// controlling one, so a FIFO planted at the path
/// (even one swapped in after a directory listing) returns at once instead of
/// waiting for a writer; the opened handle is then inspected and anything but
/// a regular file is refused before a byte is read. On other platforms the open
/// is a plain open: std has no non-blocking open for a planted named pipe, so
/// there such a path can stall the open itself instead of returning at once.
/// Symlinks to regular files are followed. A directory keeps the platform's
/// own directory error.
pub fn open_regular(path: &Path) -> io::Result<(fs::File, u64)> {
    let file = open_for_read(path)?;
    let meta = file.metadata()?;
    require_regular(&file, &meta)?;
    Ok((file, meta.len()))
}

#[cfg(unix)]
fn open_for_read(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
}

#[cfg(not(unix))]
fn open_for_read(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}

/// Opens `path` without following a trailing symlink, returning the handle and its size.
///
/// Unix uses `O_NOFOLLOW`, so a symlink at the final component fails with a loop error
/// instead of being followed. Windows opens the final component with reparse-point
/// processing disabled and refuses the opened handle whose own metadata still shows a
/// reparse point, so a link swapped in after any earlier inspection is refused, never
/// followed. Configuration and baseline inputs keep using [`open_regular`], which follows
/// symlinks to regular files; the cache and analyzed sources use this opener.
pub fn open_regular_nofollow(path: &Path) -> io::Result<(fs::File, u64)> {
    let file = open_for_read_nofollow(path)?;
    let meta = file.metadata()?;
    require_regular(&file, &meta)?;
    Ok((file, meta.len()))
}

#[cfg(unix)]
fn open_for_read_nofollow(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_for_read_nofollow(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    // The flag opens the final component itself instead of its target, so the
    // handle's own metadata decides: a link swapped in after an earlier
    // directory listing or path inspection is refused, not followed.
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if is_reparse_point(&file.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(file)
}

/// Fallback for platforms with neither unix nor windows open options. It can only
/// pre-check the path, so a link swapped in after the check is still followed.
#[cfg(not(any(unix, windows)))]
fn open_for_read_nofollow(path: &Path) -> io::Result<fs::File> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    if is_reparse_point(&meta) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    fs::File::open(path)
}

/// Whether a failed nofollow open of `path` was refused because the path is a symlink, a
/// Windows reparse point, or a FIFO, device, or socket.
///
/// Unix reports a trailing symlink as `ELOOP`. A unix socket cannot be opened at all: its
/// open fails with `ENXIO` on Linux and `EOPNOTSUPP` on macOS, and either counts only when
/// the path, inspected without following links, is a socket. A non-regular file is refused
/// as `InvalidInput`, and that counts
/// only when the path, inspected without following links, is neither a regular file nor a
/// directory, or is a reparse point. A directory keeps its platform error, and so does any
/// other failure, such as permissions or a path that cannot be named.
pub fn refused_without_following(path: &Path, error: &io::Error) -> bool {
    if is_symlink_loop(error) {
        return true;
    }
    if refused_socket_open(path, error) {
        return true;
    }
    error.kind() == io::ErrorKind::InvalidInput
        && fs::symlink_metadata(path)
            .is_ok_and(|meta| (!meta.is_file() && !meta.is_dir()) || is_reparse_point(&meta))
}

#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn is_symlink_loop(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
fn is_symlink_loop(_error: &io::Error) -> bool {
    false
}

#[cfg(unix)]
fn refused_socket_open(path: &Path, error: &io::Error) -> bool {
    use std::os::unix::fs::FileTypeExt;
    let errno = error.raw_os_error();
    let socket_open_errno = errno == Some(libc::ENXIO) || errno == Some(libc::EOPNOTSUPP);
    socket_open_errno && fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
}

#[cfg(not(unix))]
fn refused_socket_open(_path: &Path, _error: &io::Error) -> bool {
    false
}

fn require_regular(file: &fs::File, meta: &fs::Metadata) -> io::Result<()> {
    if meta.is_file() {
        return Ok(());
    }
    if meta.is_dir() {
        let mut probe = file;
        let mut byte = [0u8; 1];
        let _ = probe.read(&mut byte)?;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "not a regular file",
    ))
}

/// Reads a whole regular file, never more than the size it had when opened.
///
/// Configuration and baseline inputs enforce ceilings via [`read_bounded_string`].
/// Trusted internal reads such as executable hashing use this reader. Streams such as
/// `/dev/zero` are refused as non-regular instead, and the read is bounded by
/// the opened file's own size, so a file that keeps growing fails with an
/// error rather than being read without end.
pub fn read_regular(path: &Path) -> io::Result<Vec<u8>> {
    let (file, len) = open_regular(path)?;
    read_bounded(file, len)
}

/// Reads a regular file of at most `max_len` bytes without following a trailing symlink.
///
/// The cache uses this so a link planted at a shard file is refused instead of being
/// followed. The length comes from the opened handle, so a file swapped in after any
/// earlier check cannot exceed the cap. Configuration and baseline inputs keep using
/// [`read_regular`].
pub fn read_regular_nofollow(path: &Path, max_len: u64) -> io::Result<Vec<u8>> {
    let (file, len) = open_regular_nofollow(path)?;
    if len > max_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file of {len} bytes exceeds the {max_len}-byte cap"),
        ));
    }
    read_bounded(file, len)
}

pub fn read_bounded(file: fs::File, len: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(len).unwrap_or(usize::MAX))
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::OutOfMemory,
                format!("file of {len} bytes does not fit in memory"),
            )
        })?;
    file.take(len.saturating_add(1)).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file grew while being read",
        ));
    }
    Ok(bytes)
}

/// Reads a regular file as UTF-8 text, enforcing `max_bytes` ceiling; see [`read_regular`].
pub fn read_bounded_string(path: &Path, max_bytes: u64) -> io::Result<String> {
    let (file, len) = open_regular(path)?;
    if len > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!("file of {len} bytes exceeds ceiling of {max_bytes} bytes"),
        ));
    }
    let bytes = read_bounded(file, len)?;
    String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::refused_without_following;
    use std::io;

    #[cfg(unix)]
    fn socket_open_errnos() -> [i32; 2] {
        [libc::ENXIO, libc::EOPNOTSUPP]
    }

    /// Every errno a platform reports when a unix socket is opened must count as the
    /// non-regular refusal, but only when the path really is a socket.
    #[cfg(unix)]
    #[test]
    fn socket_open_errnos_are_refusals_for_a_socket() {
        use std::os::unix::net::UnixListener;
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let sock = tmp.path().join("sock.rs");
        let _listener = UnixListener::bind(&sock).unwrap_or_else(|e| panic!("bind socket: {e}"));
        for errno in socket_open_errnos() {
            let error = io::Error::from_raw_os_error(errno);
            assert!(
                refused_without_following(&sock, &error),
                "os error {errno} on a socket must be the counted refusal"
            );
        }
    }

    /// The socket errnos must never launder an unsupported-operation failure on anything
    /// that is not a socket: a regular file keeps its own error, as does a missing path
    /// whose inspection finds no socket at all.
    #[cfg(unix)]
    #[test]
    fn socket_open_errnos_on_a_regular_file_still_surface() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let regular = tmp.path().join("regular.rs");
        std::fs::write(&regular, "pub fn regular() {}\n").unwrap_or_else(|e| panic!("write: {e}"));
        let missing = tmp.path().join("missing.rs");
        for errno in socket_open_errnos() {
            let error = io::Error::from_raw_os_error(errno);
            assert!(
                !refused_without_following(&regular, &error),
                "os error {errno} on a regular file must surface, never count as a refusal"
            );
            assert!(
                !refused_without_following(&missing, &error),
                "os error {errno} with no socket present must surface, never count as a refusal"
            );
        }
    }
}
