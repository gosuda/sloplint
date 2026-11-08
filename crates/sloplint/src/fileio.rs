use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// On unix the open is non-blocking, so a planted FIFO cannot stall it or adopt a terminal; other platforms can stall.
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
    // Opens the reparse point itself so the handle's own metadata decides; a link swapped in later is refused, not followed.
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

// No nofollow open exists here: the pre-check is best-effort, so a link swapped in after it is still followed.
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
        // One read makes a directory fail with the platform's own error instead of a generic refusal.
        let mut probe = file;
        let mut byte = [0u8; 1];
        let _ = probe.read(&mut byte)?;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "not a regular file",
    ))
}

pub fn read_regular(path: &Path) -> io::Result<Vec<u8>> {
    let (file, len) = open_regular(path)?;
    read_bounded(file, len)
}

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

/// `len` is the size at open time; the read stops there, so a file that grows while being read is refused.
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

#[cfg(all(test, unix))]
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
