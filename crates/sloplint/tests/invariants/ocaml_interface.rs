//! A sibling `.mli` hides the `.ml` publics: the lone file reports
//! `grep/generic-name` while the paired project does not, through the cold
//! library run and through the cached binary run.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{fired, run_ok, standard, tempdir, write};

const LIB_ML: &str = "let handle x = x\n";
const LIB_MLI: &str = "val handle : 'a -> 'a\n";

/// A project holding only `src/lib.ml`.
fn lone_project() -> (tempfile::TempDir, PathBuf) {
    project_impl(false)
}

/// A project holding `src/lib.ml` beside its `src/lib.mli`.
fn paired_project() -> (tempfile::TempDir, PathBuf) {
    project_impl(true)
}

fn project_impl(paired: bool) -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(&root, "src/lib.ml", LIB_ML);
    if paired {
        write(&root, "src/lib.mli", LIB_MLI);
    }
    (tmp, root)
}

#[test]
fn sibling_interface_hides_publics_cold() {
    let (_lone_tmp, lone) = lone_project();
    let lone_report = run_ok(&standard(&lone));
    assert!(
        fired(&lone_report, "grep/generic-name"),
        "the lone lib.ml must expose `handle`: {:?}",
        lone_report.findings,
    );

    let (_paired_tmp, paired) = paired_project();
    let paired_report = run_ok(&standard(&paired));
    assert!(
        !fired(&paired_report, "grep/generic-name"),
        "lib.mli must hide `handle`: {:?}",
        paired_report.findings,
    );
    assert_ne!(
        lone_report.findings, paired_report.findings,
        "the paired run must report different findings",
    );
}

/// Temp dirs standing in for the user's cache and home directories, so the
/// cached binary run below never touches the runner's real cache.
struct CacheHome {
    _tmp: tempfile::TempDir,
    base: PathBuf,
}

impl CacheHome {
    fn new() -> CacheHome {
        let (tmp, base) = tempdir();
        for name in ["xdg", "home", "local"] {
            let dir = base.join(name);
            std::fs::create_dir_all(&dir)
                .unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
        }
        CacheHome { _tmp: tmp, base }
    }

    /// Runs `sloplint check` in `root` and returns stdout; a stderr warning fails the test.
    fn check(&self, root: &Path) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sloplint"));
        command
            .current_dir(root)
            .args(["check", "--strictness", "standard", "--fail-on", "never"])
            .env("XDG_CACHE_HOME", self.base.join("xdg"))
            .env("HOME", self.base.join("home"))
            .env("LOCALAPPDATA", self.base.join("local"));
        let out = command
            .output()
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success() && stderr.is_empty(),
            "sloplint check: {:?}: {stderr}",
            out.status,
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

#[test]
fn sibling_interface_hides_publics_through_cache() {
    let (_tmp, root) = paired_project();
    let home = CacheHome::new();
    let cold = home.check(&root);
    assert!(
        !cold.contains("[grep/generic-name]"),
        "the cold run must hide `handle`: {cold}",
    );
    let warm = home.check(&root);
    assert_eq!(warm, cold, "the warm run must reuse the paired findings");

    let (_lone_tmp, lone) = lone_project();
    let lone_out = CacheHome::new().check(&lone);
    assert!(
        lone_out.contains("[grep/generic-name]"),
        "the lone lib.ml must expose `handle`: {lone_out}",
    );
}
