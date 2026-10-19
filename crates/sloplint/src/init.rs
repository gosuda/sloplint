//! `sloplint init`: writes a default `sloplint.toml` in the current directory.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use sloplint_core::config::{Config, LANGUAGE_KEYS};
use sloplint_core::{FailOn, Mode};

use crate::RunError;
use crate::project::CONFIG_FILE;

const HEADER: &str = "# sloplint configuration. Every value shown is the default. Run 'sloplint rules' for the effective rule settings; override one rule with [rules.\"<id>\"].";

/// Writes `sloplint.toml` into `cwd`, refusing to overwrite an existing file.
///
/// The text is written to a sibling temporary file and published with a hard
/// link, which fails if the name exists, so the configuration appears whole or
/// not at all and an existing file or symlink (even a dangling one) is never
/// touched. When the link cannot be made for any reason other than the name
/// existing (for example a filesystem without hard links), it falls back to
/// creating the final name exclusively; if writing then fails, the file this
/// call created is removed. Either way a failure leaves no empty or partial
/// configuration that later runs would load and later `init` calls would
/// refuse to replace.
pub fn write_default(cwd: &Path) -> Result<(), RunError> {
    let path = cwd.join(CONFIG_FILE);
    let exists = || RunError::ConfigExists { path: path.clone() };
    if fs::symlink_metadata(&path).is_ok() {
        return Err(exists());
    }
    let text = default_text(&Config::default());
    let tmp = cwd.join(format!(".{CONFIG_FILE}.{}.tmp", std::process::id()));
    write_new(&tmp, text.as_bytes()).map_err(|source| RunError::Io {
        path: path.clone(),
        source,
    })?;
    let linked = fs::hard_link(&tmp, &path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(exists()),
        Err(_) => write_new(&path, text.as_bytes()).map_err(|source| match source.kind() {
            io::ErrorKind::AlreadyExists => exists(),
            _ => RunError::Io {
                path: path.clone(),
                source,
            },
        }),
    }
}

/// Creates `path` exclusively and writes `bytes`; a failed write removes the file this call created.
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    written.inspect_err(|_| {
        let _ = fs::remove_file(path);
    })
}

fn default_text(config: &Config) -> String {
    let project = &config.project;
    let mut lines = vec![
        HEADER.to_string(),
        String::new(),
        "[project]".to_string(),
        commented(
            &format!("strictness = {}", quote(project.strictness.as_str())),
            "# \"minimal\" | \"basic\" | \"standard\" | \"strict\"",
        ),
        format!("languages = {}", array(&project.languages)),
        commented(
            &format!("mode = {}", quote(mode_name(project.mode))),
            "# \"threshold\" | \"ratchet\"",
        ),
        format!("baseline = {}", quote(&project.baseline)),
        commented(
            &format!("fail_on = {}", quote(fail_on_name(project.fail_on))),
            "# \"warn\" | \"deny\" | \"never\"",
        ),
        format!("exclude = {}", array(&project.exclude)),
        format!("extend_exclude = {}", array(&project.extend_exclude)),
        format!("test_globs = {}", array(&project.test_globs)),
        format!("max_file_bytes = {}", project.max_file_bytes),
        format!("custom_rule_files = {}", array(&project.custom_rule_files)),
        String::new(),
    ];
    for key in LANGUAGE_KEYS {
        if let Some(language) = config.languages.get(key) {
            lines.push(format!("[languages.{key}]"));
            lines.push(format!("log_callees = {}", array(&language.log_callees)));
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn commented(assignment: &str, comment: &str) -> String {
    format!("{assignment:<35}{comment}")
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn array(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| quote(item)).collect();
    format!("[{}]", quoted.join(", "))
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Threshold => "threshold",
        Mode::Ratchet => "ratchet",
    }
}

fn fail_on_name(fail_on: FailOn) -> &'static str {
    match fail_on {
        FailOn::Warn => "warn",
        FailOn::Deny => "deny",
        FailOn::Never => "never",
    }
}
