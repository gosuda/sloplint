use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use sloplint_core::config::{Config, Mode, ProjectConfig};
use sloplint_core::finding::FailOn;
use sloplint_core::lang::ALL_CONFIG_KEYS;

use crate::RunError;
use crate::project::CONFIG_FILE;

const HEADER: &str = "# sloplint configuration. Every value shown is the default. Run 'sloplint rules' for the effective rule settings; override one rule with [rules.\"<id>\"].";

/// The config is published with a hard link, which fails when the name exists where a rename would not; filesystems without hard links fall back to an exclusive create.
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

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    written.inspect_err(|_| {
        let _ = fs::remove_file(path);
    })
}

fn default_text(config: &Config) -> String {
    let mut sections = project_lines(&config.project);
    sections.extend(language_lines(config));
    let mut text = sections.join("\n");
    text.push('\n');
    text
}

fn project_lines(project: &ProjectConfig) -> Vec<String> {
    vec![
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
            "# \"info\" | \"minor\" | \"major\" | \"critical\" | \"blocker\" | \"never\"",
        ),
        commented(
            &format!(
                "min_confidence = {}",
                quote(project.min_confidence.as_str())
            ),
            "# \"low\" | \"medium\" | \"high\"",
        ),
        format!("exclude = {}", array(&project.exclude)),
        format!("extend_exclude = {}", array(&project.extend_exclude)),
        format!("test_globs = {}", array(&project.test_globs)),
        format!("bench_globs = {}", array(&project.bench_globs)),
        format!("example_globs = {}", array(&project.example_globs)),
        format!("support_globs = {}", array(&project.support_globs)),
        format!("entry_globs = {}", array(&project.entry_globs)),
        format!("data_globs = {}", array(&project.data_globs)),
        format!("max_file_bytes = {}", project.max_file_bytes),
        format!("custom_rule_files = {}", array(&project.custom_rule_files)),
        String::new(),
    ]
}

fn language_lines(config: &Config) -> Vec<String> {
    let mut lines = Vec::new();
    for key in ALL_CONFIG_KEYS {
        if let Some(language) = config.languages.get(*key) {
            lines.push(format!("[languages.{key}]"));
            lines.push(format!("log_callees = {}", array(&language.log_callees)));
        }
    }
    lines
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
        FailOn::At(severity) => severity.as_str(),
        FailOn::Never => "never",
    }
}
