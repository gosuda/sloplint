//! Custom rule files: explicit selection loads hidden and ignored files.

#[cfg(unix)]
use sloplint::{RunError, run};
#[cfg(unix)]
use sloplint_core::ConfigError;

use super::{custom_rule, run_ok, standard, tempdir, write};
#[cfg(unix)]
use super::{mkdir, symlink};

#[test]
fn audit_custom_files_hidden_and_ignored_are_loaded() {
    let (_tmp, root) = tempdir();
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(
        &root,
        ".hidden.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        "ignored.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        "ignored-dir/src.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        ".gitignore",
        "ignored-rules.toml\nignored.rs\nignored-dir/\n",
    );
    write(
        &root,
        ".hidden-rules.toml",
        &custom_rule("custom/hidden-hit"),
    );
    write(
        &root,
        "ignored-rules.toml",
        &custom_rule("custom/ignored-hit"),
    );
    write(&root, ".rules/a.toml", &custom_rule("custom/glob-hit"));
    write(
        &root,
        ".rules/nested/deep.toml",
        &custom_rule("custom/deep-hit"),
    );
    write(
        &root,
        "ignored-dir/b.toml",
        &custom_rule("custom/ignored-dir-hit"),
    );
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\".hidden-rules.toml\", \"ignored-rules.toml\", \".rules/*.toml\", \".rules/a.toml\", \"ignored-dir/*.toml\", \"empty-dir/*.toml\"]\n",
    );
    let report = run_ok(&standard(&root));
    for id in [
        "custom/hidden-hit",
        "custom/ignored-hit",
        "custom/glob-hit",
        "custom/deep-hit",
        "custom/ignored-dir-hit",
    ] {
        assert!(
            report
                .findings
                .iter()
                .any(|f| &*f.rule == id && &*f.path == "src/main.rs"),
            "explicit selection {id} produced no finding"
        );
    }
    assert!(
        report.findings.iter().all(|f| {
            &*f.path != ".hidden.rs" && &*f.path != "ignored.rs" && &*f.path != "ignored-dir/src.rs"
        }),
        "hidden or ignored sources must stay unlinted"
    );
}

#[test]
fn audit_custom_files_empty_match_is_allowed() {
    let (_tmp, root) = tempdir();
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(
        &root,
        "src/lib.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"empty-dir/*.toml\"]\n",
    );
    let report = run_ok(&standard(&root));
    assert!(
        report
            .findings
            .iter()
            .all(|f| !f.rule.starts_with("custom/")),
        "empty selection must yield no custom findings"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "grep/generic-name" && &*f.path == "src/lib.rs"),
        "run must still report ordinary findings alongside the empty selection"
    );
}

#[cfg(unix)]
#[test]
fn audit_custom_files_dir_symlink_is_not_followed() {
    let (_tmp, base) = tempdir();
    write(&base, "outside/evil.toml", &custom_rule("custom/evil-hit"));
    let root = base.join("proj");
    mkdir(&root.join("src"));
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(&root, "ok.toml", &custom_rule("custom/ok-hit"));
    symlink(base.join("outside"), root.join("link"));
    symlink(root.join("ok.toml"), root.join("alias.toml"));
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"ok.toml\", \"alias.toml\", \"link/*.toml\", \"link/evil.toml\"]\n",
    );
    let Err(error) = run(&standard(&root)) else {
        panic!("symlinked custom rule file must fail closed");
    };
    assert_eq!(error.exit_code(), 2);
    let RunError::Config(ConfigError::Glob { key, glob, .. }) = error else {
        panic!("expected ConfigError::Glob, got: {error:?}");
    };
    assert_eq!(key, "project.custom_rule_files");
    assert!(glob.contains("alias.toml"), "error must name alias.toml");
}

#[test]
fn audit_custom_files_leading_wildcard_skips_git() {
    let (_tmp, root) = tempdir();
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(&root, "ok.sloplint.toml", &custom_rule("custom/ok-hit"));
    write(
        &root,
        "sub/nested.sloplint.toml",
        &custom_rule("custom/ok-nested-hit"),
    );
    write(
        &root,
        ".git/evil.sloplint.toml",
        &custom_rule("custom/evil-hit"),
    );
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"**/*.sloplint.toml\"]\n",
    );
    let report = run_ok(&standard(&root));
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "custom/ok-nested-hit" && &*f.path == "src/main.rs"),
        "leading wildcard must load nested rules while skipping .git"
    );
    assert!(
        report
            .findings
            .iter()
            .all(|f| &*f.rule != "custom/evil-hit"),
        ".git rule files must stay unloaded"
    );
}

#[cfg(unix)]
#[test]
fn audit_custom_files_project_root_symlink_is_followed() {
    let (_tmp, base) = tempdir();
    let real = base.join("real");
    mkdir(&real.join("src"));
    write(&real, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(&real, "ok.toml", &custom_rule("custom/ok-hit"));
    write(
        &real,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"ok.toml\"]\n",
    );
    let linked = base.join("linked");
    symlink(&real, &linked);
    let report = run_ok(&standard(&linked));
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "custom/ok-hit" && &*f.path == "src/main.rs"),
        "project root symlink must still load its selections"
    );
}
