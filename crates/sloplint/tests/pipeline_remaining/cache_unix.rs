//! Unix permission and executable-access cases for the per-user cache.

use super::{
    CHECK, UNCACHED, cache_files, command, isolated, names, project, spawn_forgiving,
    succeeded_with_finding, temp, text,
};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Puts a directory's mode back on drop so the temporary tree can be removed.
struct Restore(PathBuf);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("chmod {mode:o} {}: {e}", path.display()));
}

#[test]
fn a_symlinked_cache_namespace_is_refused_and_never_followed() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    let (_outside_guard, outside) = temp();
    std::os::unix::fs::symlink(&outside, home.join("sloplint"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));

    let out = isolated(&root, &CHECK, &home);

    succeeded_with_finding(&out);
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("cache disabled") && stderr.contains("symlink"),
        "stderr: {stderr}"
    );
    assert_eq!(
        names(&outside),
        Vec::<String>::new(),
        "the outside target was written"
    );
}

#[test]
fn a_namespace_writable_by_others_is_not_trusted() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    let namespace = home.join("sloplint");
    std::fs::create_dir(&namespace).unwrap_or_else(|e| panic!("mkdir: {e}"));
    chmod(&namespace, 0o777);

    let out = isolated(&root, &CHECK, &home);

    succeeded_with_finding(&out);
    assert!(
        text(&out.stderr).contains("writable by group or others"),
        "stderr: {}",
        text(&out.stderr)
    );
    assert!(
        cache_files(&home).is_empty(),
        "a cache was written into the unsafe directory"
    );
}

#[test]
fn a_file_blocking_the_namespace_disables_the_cache() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    std::fs::write(home.join("sloplint"), "blocker").unwrap_or_else(|e| panic!("write: {e}"));

    let out = isolated(&root, &CHECK, &home);

    succeeded_with_finding(&out);
    assert!(
        text(&out.stderr).contains("not a directory"),
        "stderr: {}",
        text(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(home.join("sloplint")).unwrap_or_default(),
        "blocker",
        "the blocking file must be left exactly as it was"
    );
}

#[test]
fn an_unwritable_cache_base_warns_and_the_lint_still_reports() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    chmod(&home, 0o555);
    let _restore = Restore(home.clone());
    assert!(
        std::fs::File::create(home.join("probe")).is_err(),
        "permission bits were bypassed; run these tests as an unprivileged user, not root"
    );

    let out = isolated(&root, &CHECK, &home);

    succeeded_with_finding(&out);
    assert!(
        text(&out.stderr).contains("cache not written in"),
        "stderr: {}",
        text(&out.stderr)
    );
}

#[test]
fn control_characters_in_a_cache_path_are_escaped_in_warnings() {
    let (_tmp, root) = project();
    let hostile = root.join("ca\u{1b}che");

    let out = isolated(&root, &CHECK, &hostile);

    succeeded_with_finding(&out);
    let stderr = text(&out.stderr);
    assert!(
        !stderr.contains('\u{1b}'),
        "a raw escape reached the terminal: {stderr:?}"
    );
    assert!(
        stderr.contains("\\x1b"),
        "the escape must stay visible: {stderr:?}"
    );
}

#[test]
fn an_unreadable_executable_cannot_break_a_no_cache_run() {
    let (_tmp, root) = project();
    let (_home_guard, home) = temp();
    let copy = home.join("sloplint-copy");
    let staging = home.join("sloplint-copy.tmp");
    {
        let mut source = std::fs::File::open(env!("CARGO_BIN_EXE_sloplint"))
            .unwrap_or_else(|e| panic!("open source: {e}"));
        let mut dest =
            std::fs::File::create(&staging).unwrap_or_else(|e| panic!("create staging: {e}"));
        std::io::copy(&mut source, &mut dest).unwrap_or_else(|e| panic!("copy: {e}"));
        dest.sync_all().unwrap_or_else(|e| panic!("fsync: {e}"));
    }
    chmod(&staging, 0o111);
    // A sibling test's fork can inherit the destination fd and hold it
    // open for write past our own close, so exec'ing the copy fails with
    // ETXTBSY (os error 26) until that sibling's exec drops it. fsync and
    // rename land bytes and name atomically but do not close the forked
    // fd; spawn sites retry the exec for a bounded window.
    std::fs::rename(&staging, &copy).unwrap_or_else(|e| panic!("rename: {e}"));
    assert!(
        std::fs::File::open(&copy).is_err(),
        "permission bits were bypassed; run these tests as an unprivileged user, not root"
    );

    let uncached = spawn_forgiving(command(copy.as_os_str(), &root, &UNCACHED));
    succeeded_with_finding(&uncached);
    assert!(
        uncached.stderr.is_empty(),
        "stderr: {}",
        text(&uncached.stderr)
    );

    let mut cached = command(copy.as_os_str(), &root, &CHECK);
    cached.env("XDG_CACHE_HOME", home.join("cache"));
    let cached = spawn_forgiving(cached);
    succeeded_with_finding(&cached);
    assert!(
        text(&cached.stderr).contains("cannot read the running executable"),
        "an unusable cache disables the optimization with a warning: {}",
        text(&cached.stderr)
    );
}
