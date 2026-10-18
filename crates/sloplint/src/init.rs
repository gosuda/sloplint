//! `sloplint init`: writes a default `sloplint.toml` in the current directory.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use sloplint_core::config::{Config, LANGUAGE_KEYS};
use sloplint_core::{FailOn, Mode};

use crate::RunError;
use crate::project::CONFIG_FILE;

const HEADER: &str = "# sloplint configuration. Every value shown is the default. Run 'sloplint rules' for the effective rule settings; override one rule with [rules.\"<id>\"].";

/// Writes `sloplint.toml` into `cwd`, refusing to overwrite an existing file.
pub fn write_default(cwd: &Path) -> Result<(), RunError> {
    let path = cwd.join(CONFIG_FILE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| match source.kind() {
            io::ErrorKind::AlreadyExists => RunError::ConfigExists { path: path.clone() },
            _ => RunError::Io {
                path: path.clone(),
                source,
            },
        })?;
    file.write_all(default_text(&Config::default()).as_bytes())
        .map_err(|source| RunError::Io { path, source })
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
