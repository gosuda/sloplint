//! Regression tests for config, custom rule, and baseline read ceilings,
//! pattern validation, and ancestor symlink checks.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use sloplint::{RunError, RunOptions, run};
use sloplint_core::ConfigError;
use sloplint_core::baseline::MAX_BASELINE_ENTRIES;
use sloplint_core::config::{Mode, Strictness};

const MAX_CONFIG_FILE_BYTES: u64 = 1_048_576;
const MAX_CUSTOM_RULES_TOTAL_BYTES: u64 = 16_777_216;
const MAX_BASELINE_BYTES: u64 = 536_870_912;

fn project(config: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create src");
    fs::write(src.join("lib.rs"), "pub fn handle() {}\n").expect("write lib.rs");
    fs::write(root.join("sloplint.toml"), config).expect("write sloplint.toml");
    (tmp, root)
}

fn options(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    }
}

#[test]
fn test_oversized_config_refused() {
    let (_tmp, root) = project("");
    let config_path = root.join("sloplint.toml");
    let file = File::create(&config_path).expect("create config");
    file.set_len(MAX_CONFIG_FILE_BYTES + 1).expect("set_len");
    drop(file);

    let opts = options(&root);
    let Err(error) = run(&opts) else {
        panic!("oversized sloplint.toml must be rejected");
    };
    assert_eq!(error.exit_code(), 3);
    let RunError::Io { path, source } = error else {
        panic!("expected RunError::Io, got: {error:?}");
    };
    assert_eq!(path, config_path);
    assert_eq!(source.kind(), std::io::ErrorKind::FileTooLarge);
    assert!(
        source
            .to_string()
            .contains(&MAX_CONFIG_FILE_BYTES.to_string()),
        "error must contain config limit"
    );
}

#[test]
fn test_oversized_custom_rule_file_refused() {
    let (_tmp, root) = project("[project]\ncustom_rule_files = [\"rules/*.toml\"]\n");
    let rules_dir = root.join("rules");
    fs::create_dir_all(&rules_dir).expect("create rules dir");
    let custom_path = rules_dir.join("big.toml");
    let file = File::create(&custom_path).expect("create big.toml");
    file.set_len(MAX_CONFIG_FILE_BYTES + 1).expect("set_len");
    drop(file);

    let opts = options(&root);
    let Err(error) = run(&opts) else {
        panic!("oversized custom rule file must be rejected");
    };
    assert_eq!(error.exit_code(), 3);
    let RunError::Io { path, source } = error else {
        panic!("expected RunError::Io, got: {error:?}");
    };
    assert_eq!(path, custom_path);
    assert_eq!(source.kind(), std::io::ErrorKind::FileTooLarge);
    assert!(
        source
            .to_string()
            .contains(&MAX_CONFIG_FILE_BYTES.to_string()),
        "error must contain custom file limit"
    );
}

#[test]
fn test_total_custom_ceiling_refused() {
    let (_tmp, root) = project("[project]\ncustom_rule_files = [\"rules/*.toml\"]\n");
    let rules_dir = root.join("rules");
    fs::create_dir_all(&rules_dir).expect("create rules dir");
    for i in 0..17 {
        let path = rules_dir.join(format!("rule_{i:02}.toml"));
        let file = File::create(&path).expect("create file");
        file.set_len(MAX_CONFIG_FILE_BYTES).expect("set_len");
        drop(file);
    }

    let opts = options(&root);
    let Err(error) = run(&opts) else {
        panic!("exceeding total custom rule ceiling must be rejected");
    };
    assert_eq!(error.exit_code(), 3);
    let RunError::Io { path, source } = error else {
        panic!("expected RunError::Io, got: {error:?}");
    };
    assert!(path.starts_with(&rules_dir), "path must be in rules dir");
    assert_eq!(source.kind(), std::io::ErrorKind::FileTooLarge);
    assert!(
        source
            .to_string()
            .contains(&MAX_CUSTOM_RULES_TOTAL_BYTES.to_string()),
        "error must contain total custom ceiling"
    );
}

#[test]
fn test_oversized_baseline_refused() {
    let (_tmp, root) = project("[project]\nmode = \"ratchet\"\n");
    let baseline_path = root.join("sloplint-baseline.json");
    let file = File::create(&baseline_path).expect("create baseline");
    file.set_len(MAX_BASELINE_BYTES + 1).expect("set_len");
    drop(file);

    let mut opts = options(&root);
    opts.mode = Some(Mode::Ratchet);
    let Err(error) = run(&opts) else {
        panic!("oversized baseline must be rejected");
    };
    assert_eq!(error.exit_code(), 3);
    let RunError::Io { path, source } = error else {
        panic!("expected RunError::Io, got: {error:?}");
    };
    assert_eq!(
        path,
        baseline_path
            .canonicalize()
            .expect("canonical baseline path")
    );
    assert_eq!(source.kind(), std::io::ErrorKind::FileTooLarge);
    assert!(
        source.to_string().contains(&MAX_BASELINE_BYTES.to_string()),
        "error must contain baseline byte limit"
    );
}

#[test]
fn test_over_count_baseline_refused() {
    let (_tmp, root) = project("[project]\nmode = \"ratchet\"\n");
    let baseline_path = root.join("sloplint-baseline.json");
    let mut file = File::create(&baseline_path).expect("create baseline");
    file.write_all(b"{\"version\":1,\"findings\":[")
        .expect("write header");
    let entry = b"{\"path\":\"a.rs\",\"rule\":\"test\",\"fingerprint\":\"0123456789abcdef0123456789abcdef\"}";
    let count = MAX_BASELINE_ENTRIES + 1;
    for i in 0..count {
        if i > 0 {
            file.write_all(b",").expect("write comma");
        }
        file.write_all(entry).expect("write entry");
    }
    file.write_all(b"]}").expect("write trailer");
    drop(file);

    let mut opts = options(&root);
    opts.mode = Some(Mode::Ratchet);
    let Err(error) = run(&opts) else {
        panic!("over-count baseline must be rejected");
    };
    assert_eq!(error.exit_code(), 2);
    let RunError::BaselineCorrupt { path, message } = error else {
        panic!("expected RunError::BaselineCorrupt, got: {error:?}");
    };
    assert_eq!(
        path,
        baseline_path
            .canonicalize()
            .expect("canonical baseline path")
    );
    assert!(
        message.contains(&MAX_BASELINE_ENTRIES.to_string()),
        "error must contain baseline entry limit"
    );
}

#[test]
fn test_absolute_and_dotdot_custom_rule_patterns_rejected() {
    for pattern in [
        "/abs/rules.toml",
        "/rules/*.toml",
        "../rules.toml",
        "rules/../../outside.toml",
    ] {
        let (_tmp, root) = project(&format!("[project]\ncustom_rule_files = [\"{pattern}\"]\n"));
        let opts = options(&root);
        let Err(error) = run(&opts) else {
            panic!("pattern `{pattern}` must be rejected");
        };
        assert_eq!(error.exit_code(), 2);
        let RunError::Config(ConfigError::Glob { key, glob, .. }) = error else {
            panic!("pattern `{pattern}`: expected ConfigError::Glob, got: {error:?}");
        };
        assert_eq!(key, "project.custom_rule_files");
        assert_eq!(glob, pattern);
    }
}

#[cfg(unix)]
#[test]
fn test_unreadable_ancestor_metadata_fails_closed() {
    use std::os::unix::fs::PermissionsExt;

    let (_tmp, root) = project("[project]\ncustom_rule_files = [\"forbidden/sub/deep/*.toml\"]\n");
    let forbidden = root.join("forbidden");
    let sub = forbidden.join("sub");
    let deep = sub.join("deep");
    fs::create_dir_all(&deep).expect("create forbidden/sub/deep");
    fs::write(deep.join("rule.toml"), "").expect("write rule.toml");

    fs::set_permissions(&forbidden, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    if fs::read_dir(&forbidden).is_ok() {
        let _ = fs::set_permissions(&forbidden, fs::Permissions::from_mode(0o755));
        return;
    }

    let opts = options(&root);
    let result = run(&opts);

    let _ = fs::set_permissions(&forbidden, fs::Permissions::from_mode(0o755));

    let Err(error) = result else {
        panic!("unreadable ancestor metadata must cause run to fail closed");
    };
    assert_eq!(error.exit_code(), 3);
    let RunError::Io { path, source } = error else {
        panic!("expected RunError::Io, got: {error:?}");
    };
    assert_eq!(path, sub);
    assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
}

#[cfg(unix)]
#[test]
fn test_custom_rule_literal_symlink_rejected() {
    let (_tmp, root) = project("[project]\ncustom_rule_files = [\"rules/link.toml\"]\n");
    let rules_dir = root.join("rules");
    fs::create_dir_all(&rules_dir).expect("create rules dir");
    let target = rules_dir.join("target.toml");
    fs::write(&target, "").expect("write target");
    std::os::unix::fs::symlink(&target, rules_dir.join("link.toml")).expect("symlink");

    let opts = options(&root);
    let Err(error) = run(&opts) else {
        panic!("literal symlink custom rule file must be rejected");
    };
    assert_eq!(error.exit_code(), 2);
    let RunError::Config(ConfigError::Glob { key, glob, .. }) = error else {
        panic!("expected ConfigError::Glob, got: {error:?}");
    };
    assert_eq!(key, "project.custom_rule_files");
    assert!(
        glob.contains("rules/link.toml"),
        "error must name literal symlink"
    );
}

#[cfg(unix)]
#[test]
fn test_custom_rule_glob_matching_symlink_rejected() {
    let (_tmp, root) = project("[project]\ncustom_rule_files = [\"rules/*.toml\"]\n");
    let rules_dir = root.join("rules");
    fs::create_dir_all(&rules_dir).expect("create rules dir");
    let target = rules_dir.join("target.txt");
    fs::write(&target, "").expect("write target");
    std::os::unix::fs::symlink(&target, rules_dir.join("link.toml")).expect("symlink");

    let opts = options(&root);
    let Err(error) = run(&opts) else {
        panic!("glob-matched symlink custom rule file must be rejected");
    };
    assert_eq!(error.exit_code(), 2);
    let RunError::Config(ConfigError::Glob { key, glob, .. }) = error else {
        panic!("expected ConfigError::Glob, got: {error:?}");
    };
    assert_eq!(key, "project.custom_rule_files");
    assert!(
        glob.contains("rules/link.toml"),
        "error must name matched symlink"
    );
}

#[test]
fn test_custom_rule_glob_regular_file_still_loads() {
    let rule_content = "[[custom_rules]]\nid = \"custom/test-rule\"\nlanguage = \"rust\"\nseverity = \"warn\"\nmessage = \"test\"\nwhy = \"test\"\nfix = \"test\"\nquery = \"(function_item) @match\"\n";
    let (_tmp, root) = project("[project]\ncustom_rule_files = [\"rules/*.toml\"]\n");
    let rules_dir = root.join("rules");
    fs::create_dir_all(&rules_dir).expect("create rules dir");
    fs::write(rules_dir.join("valid.toml"), rule_content).expect("write valid.toml");

    let opts = options(&root);
    let result = run(&opts);
    assert!(
        result.is_ok(),
        "regular custom rule file must load: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn test_dangling_implicit_config_symlink_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create src");
    fs::write(src.join("lib.rs"), "pub fn handle() {}\n").expect("write lib.rs");

    let link_path = root.join("sloplint.toml");
    std::os::unix::fs::symlink(root.join("nonexistent_config.toml"), &link_path).expect("symlink");

    let opts = options(&root);
    let Err(error) = run(&opts) else {
        panic!("dangling implicit sloplint.toml symlink must be rejected");
    };
    assert_eq!(error.exit_code(), 3);
    let RunError::Io { path, .. } = error else {
        panic!("expected RunError::Io, got: {error:?}");
    };
    assert_eq!(path, link_path);
}

#[test]
fn test_baseline_with_utf8_bom_accepted_in_ratchet() {
    let code =
        "pub fn handle(x: Option<i32>) { match x { Some(1) => {}, Some(2) => {}, _ => {} } }\n";
    let (_tmp, root) = project("[project]\nstrictness = \"strict\"\n");
    fs::write(root.join("src/lib.rs"), code).expect("write code");

    let mut threshold_opts = options(&root);
    threshold_opts.strictness = Some(Strictness::Strict);
    let report = run(&threshold_opts).expect("threshold run");
    assert!(
        !report.findings.is_empty(),
        "must produce findings under strict"
    );

    let mut findings = report.findings;
    sloplint_core::baseline::Baseline::assign_fingerprints(&mut findings);
    let baseline = sloplint_core::baseline::Baseline::from_findings(&findings);
    let json = baseline.to_json();
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(json.as_bytes());
    fs::write(root.join("sloplint-baseline.json"), &content).expect("write baseline with BOM");

    let mut ratchet_opts = options(&root);
    ratchet_opts.strictness = Some(Strictness::Strict);
    ratchet_opts.mode = Some(Mode::Ratchet);
    let result = run(&ratchet_opts);
    let Ok(ratchet_report) = result else {
        panic!("BOM-prefixed baseline must load successfully: {result:?}");
    };
    assert_eq!(ratchet_report.findings.len(), 0);
    assert!(
        ratchet_report.summary.baselined > 0,
        "baselined count must be positive"
    );
}
