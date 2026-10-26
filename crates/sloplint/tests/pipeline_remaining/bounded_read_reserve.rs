//! A sparse file whose apparent length is within `max_file_bytes` must not abort the process
//! when its read buffer cannot be reserved: the run fails as an I/O error naming the file,
//! the way every other read failure is reported.

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::process::ExitStatusExt;
    use std::path::PathBuf;
    use std::process::Command;

    /// Address-space cap for the child, in KiB: far below the 1 GiB apparent length below.
    const ADDRESS_SPACE_KIB: u32 = 524_288;
    /// Apparent length of the sparse file: within `max_file_bytes`, beyond the cap.
    const APPARENT_LEN: u64 = 1 << 30;
    /// Exit status for I/O that stops the run.
    const IO_EXIT: i32 = 3;

    fn project() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let root =
            std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
        std::fs::write(
            root.join("sloplint.toml"),
            format!("[project]\nmax_file_bytes = {}\n", u32::MAX),
        )
        .unwrap_or_else(|e| panic!("write config: {e}"));
        let sparse = std::fs::File::create(root.join("big.rs"))
            .unwrap_or_else(|e| panic!("create sparse file: {e}"));
        sparse
            .set_len(APPARENT_LEN)
            .unwrap_or_else(|e| panic!("set_len: {e}"));
        (tmp, root)
    }

    #[test]
    fn sparse_file_that_cannot_be_buffered_fails_as_io_instead_of_aborting() {
        let (_tmp, root) = project();
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "ulimit -v {ADDRESS_SPACE_KIB} && exec \"$0\" \"$@\""
            ))
            .arg(env!("CARGO_BIN_EXE_sloplint"))
            .args(["check", ".", "--format", "json", "--no-cache", "-j", "1"])
            .current_dir(&root)
            .output()
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"));
        assert!(
            output.status.signal().is_none(),
            "the process must not die on a signal, got {:?}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.status.code(),
            Some(IO_EXIT),
            "the unbufferable read is an I/O failure; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("big.rs"),
            "the I/O failure names the file; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
