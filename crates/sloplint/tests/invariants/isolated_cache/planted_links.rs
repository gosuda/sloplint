//! Hostile links planted at the cache's own names: the load never follows them and the store
//! never writes through them. Unix covers symlinks; Windows covers file reparse points and
//! junctions. Creating a symlink needs a privilege the runner may lack; that skip goes through
//! [`super::skip_unless_fs_caps_required`], never silently.

use super::*;

/// A symlink planted at the cache file's own name must neither be read nor written through.
/// A valid cache planted behind the link must not be reused; the run must still succeed with
/// the cold findings. Whether the link is kept or replaced is not part of the contract.
#[cfg(unix)]
#[test]
fn planted_cache_symlink_is_never_written_through() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let (cold, files) = cold_shard(&home, &root);
    let planted = GENERIC_TEXT.replace("code", "mode");
    let (_outside, base) = tempdir();
    let planted_path = base.join("forged.bin");
    std::fs::copy(&files[0], &planted_path).unwrap_or_else(|e| panic!("plant rewrite failed: {e}"));
    plant(&planted_path, GENERIC_TEXT, &planted);
    let forged = std::fs::read(&planted_path).unwrap_or_else(|e| panic!("read forged: {e}"));
    std::fs::remove_file(&files[0]).unwrap_or_else(|e| panic!("remove cache: {e}"));
    std::os::unix::fs::symlink(&planted_path, &files[0])
        .unwrap_or_else(|e| panic!("symlink failed: {e}"));
    let out = home.output(&root, &[]);
    assert!(
        out.status.success(),
        "a planted link failed the run: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        cold,
        "the cache load followed a symlinked cache file"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(&planted),
        "a forged cache behind a symlink was reused: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let after = std::fs::read(&planted_path).unwrap_or_else(|e| panic!("read planted: {e}"));
    assert_eq!(
        after, forged,
        "the cache store wrote through a planted symlink"
    );
}

/// A symlink planted at the temporary file's name must be left alone and never written through.
/// The temporary name carries the child's pid, so a shell plants the link and then `exec`s the
/// binary: the shell's `$$` is the pid the binary keeps. The stored cache is removed first: a
/// warm hit stores nothing, so only a run that stores reaches the temporary file.
#[cfg(unix)]
#[test]
fn planted_temp_symlink_is_preserved_and_never_written_through() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let (cold, files) = cold_shard(&home, &root);
    let shard_dir = files[0]
        .parent()
        .unwrap_or_else(|| panic!("{} has no directory", files[0].display()))
        .to_path_buf();
    std::fs::remove_file(&files[0]).unwrap_or_else(|e| panic!("remove cache: {e}"));
    let (_outside, base) = tempdir();
    write(&base, "sentinel.txt", "sentinel");
    let marker = base.join("sentinel.txt");
    let shard = files[0]
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("{} has no file name", files[0].display()))
        .to_string();
    let script = r#"ln -s "$1" "$2/$3.$$.tmp" && shift 3 && exec "$@""#;
    let mut command = Command::new("sh");
    command
        .current_dir(&root)
        .args(["-c", script, "sh"])
        .arg(&marker)
        .arg(&shard_dir)
        .arg(&shard)
        .arg(env!("CARGO_BIN_EXE_sloplint"))
        .args(["check", "--strictness", "standard", "--fail-on", "never"]);
    let out = home
        .isolate(&mut command)
        .output()
        .unwrap_or_else(|e| panic!("spawn sh: {e}"));
    assert!(
        out.status.success(),
        "a planted temp link failed the run: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        cold,
        "a planted temp link changed the findings"
    );
    let planted: Vec<PathBuf> = std::fs::read_dir(&shard_dir)
        .unwrap_or_else(|e| panic!("shard listing failed {}: {e}", shard_dir.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("entry read failed: {e}"))
                .path()
        })
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("shard-") && name.ends_with(".tmp")
        })
        .collect();
    assert_eq!(
        planted.len(),
        1,
        "the planted temp link must remain: {planted:?}"
    );
    let target = std::fs::read_link(&planted[0])
        .unwrap_or_else(|e| panic!("{} is not a symlink: {e}", planted[0].display()));
    assert_eq!(target, marker, "the planted temp link was replaced");
    let after = std::fs::read_to_string(&marker).unwrap_or_else(|e| panic!("read sentinel: {e}"));
    assert_eq!(
        after, "sentinel",
        "the cache store wrote through a planted temp link"
    );
}

/// A reparse point planted at the cache file's own name must neither be read nor written
/// through. Mirrors [`planted_cache_symlink_is_never_written_through`] against the Windows
/// reparse-point refusal in `open_for_read_nofollow`. Creating a file symlink needs a
/// privilege the runner may lack; that skip goes through the capability gate, never silent.
#[cfg(windows)]
#[test]
fn planted_reparse_cache_is_never_read_or_written_through() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let (cold, files) = cold_shard(&home, &root);
    let planted_marker = GENERIC_TEXT.replace("code", "mode");
    let (_outside, base) = tempdir();
    let planted_path = base.join("forged.bin");
    std::fs::copy(&files[0], &planted_path).unwrap_or_else(|e| panic!("plant rewrite failed: {e}"));
    plant(&planted_path, GENERIC_TEXT, &planted_marker);
    let forged = std::fs::read(&planted_path).unwrap_or_else(|e| panic!("read forged: {e}"));
    std::fs::remove_file(&files[0]).unwrap_or_else(|e| panic!("remove cache: {e}"));
    match std::os::windows::fs::symlink_file(&planted_path, &files[0]) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(1314) => {
            skip_unless_fs_caps_required(
                "planted_reparse_cache_is_never_read_or_written_through: this account cannot create symlinks",
                &error,
            );
            return;
        }
        Err(error) => panic!("symlink file failed: {error}"),
    }
    let out = home.output(&root, &[]);
    assert!(
        out.status.success(),
        "a planted link failed the run: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        cold,
        "the cache load followed a symlinked cache file"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(&planted_marker),
        "a forged cache behind a symlink was reused: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let after = std::fs::read(&planted_path).unwrap_or_else(|e| panic!("read planted: {e}"));
    assert_eq!(
        after, forged,
        "the cache store wrote through a planted symlink"
    );
}

/// A directory junction planted at the shard directory must yield an empty cache: the load
/// never follows a reparse point to attacker-controlled shard bytes, and the store never
/// writes through it. Junctions need no privilege to create.
#[cfg(windows)]
#[test]
fn planted_junction_shard_dir_yields_empty_cache() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let (cold, files) = cold_shard(&home, &root);
    let planted_marker = GENERIC_TEXT.replace("code", "mode");
    let shard_dir = files[0]
        .parent()
        .unwrap_or_else(|| panic!("{} has no directory", files[0].display()))
        .to_path_buf();
    let shard_name = files[0]
        .file_name()
        .unwrap_or_else(|| panic!("{} has no file name", files[0].display()))
        .to_os_string();
    let (_outside, base) = tempdir();
    let target = base.join("attacker");
    mkdir(&target);
    let planted_path = target.join(&shard_name);
    std::fs::copy(&files[0], &planted_path)
        .unwrap_or_else(|e| panic!("plant forged shard failed: {e}"));
    plant(&planted_path, GENERIC_TEXT, &planted_marker);
    let forged = std::fs::read(&planted_path).unwrap_or_else(|e| panic!("read forged: {e}"));
    std::fs::remove_dir_all(&shard_dir).unwrap_or_else(|e| panic!("remove shards failed: {e}"));
    let link = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&shard_dir)
        .arg(&target)
        .output()
        .unwrap_or_else(|e| panic!("spawn mklink failed: {e}"));
    assert!(
        link.status.success(),
        "mklink junction failed: {}",
        String::from_utf8_lossy(&link.stderr)
    );
    let out = home.output(&root, &[]);
    assert!(
        out.status.success(),
        "a junction at the shard directory failed the run: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        cold,
        "the cache load followed a junctioned shard directory"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(&planted_marker),
        "a forged cache behind a junction was reused: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let after =
        std::fs::read(target.join(&shard_name)).unwrap_or_else(|e| panic!("read planted: {e}"));
    assert_eq!(after, forged, "the cache store wrote through the junction");
}
