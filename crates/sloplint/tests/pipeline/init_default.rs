//! `sloplint init` writes the default project table: parsing it back equals the default.

use std::path::Path;

use sloplint_core::config::{ProjectConfig, parse_config};

fn text(root: &Path) -> String {
    std::fs::read_to_string(root.join("sloplint.toml"))
        .unwrap_or_else(|e| panic!("read config: {e}"))
}

#[test]
fn init_writes_the_default_project_table() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(tmp.path())
        .arg("init")
        .output()
        .unwrap_or_else(|e| panic!("spawn init: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "init must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed =
        parse_config(&text(tmp.path()), "sloplint.toml").unwrap_or_else(|e| panic!("parse: {e}"));
    let expected = toml::to_string(&ProjectConfig::default())
        .unwrap_or_else(|e| panic!("serialize default: {e}"));
    let actual =
        toml::to_string(&parsed.project).unwrap_or_else(|e| panic!("serialize parsed: {e}"));
    assert_eq!(
        actual, expected,
        "init must write the default project table"
    );
}
