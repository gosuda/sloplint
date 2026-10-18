//! `agent/ambient-global-state`: mutable globals, global writes, and environment reads.

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{AmbientKind, FileFacts};
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

use super::string_values;

const ALLOW_FILES: &[&str] = &[
    "**/main.rs",
    "**/build.rs",
    "**/bin/**",
    "**/main.go",
    "**/cmd/**",
    "**/__main__.py",
    "**/config.*",
    "**/settings.*",
    "**/conftest.py",
    "**/Program.cs",
    "**/Startup.cs",
    "**/Config.cs",
    "**/Settings.cs",
    "**/main.dart",
    "**/Main.scala",
    "**/main.zig",
    "**/build.zig",
    "**/main.odin",
    "**/Tiltfile",
    "**/main.ml",
    "**/main.swift",
    "**/index.php",
    "**/artisan",
    "**/main.rb",
    "**/Rakefile",
    "**/Gemfile",
    "**/main.R",
    "**/main.c",
    "**/main.cc",
    "**/main.cpp",
    "**/main.m",
    "**/profile.ps1",
    "**/scripts/**",
];

const OWNED_FIX: &str = "move this state into a value owned by the caller and pass it explicitly";
const ENV_MESSAGE: &str = "process environment read outside entry or config code";
const ENV_FIX: &str = "read configuration once at the entry point and pass it in as a parameter";

/// `agent/ambient-global-state`: flags hidden global inputs and environment reads.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/ambient-global-state",
    family: Family::Agent,
    summary: "mutable globals, global writes, and environment reads outside entry code",
    why: "Hidden global inputs make behavior depend on state the signature does not show.",
    exempt_tests: true,
    preset,
    build: Build::File(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow_files: Vec<String>,
}

struct AmbientGlobalState {
    allow_files: GlobSet,
}

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic => Level::Warn,
        Strictness::Standard | Strictness::Strict => Level::Deny,
    };
    let mut options = toml::Table::new();
    options.insert("allow_files".to_string(), string_values(ALLOW_FILES));
    Preset { level, options }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    let mut builder = GlobSetBuilder::new();
    for pattern in &parsed.allow_files {
        let glob =
            Glob::new(pattern).map_err(|error| format!("invalid glob `{pattern}`: {error}"))?;
        builder.add(glob);
    }
    let allow_files = builder.build().map_err(|error| error.to_string())?;
    Ok(Box::new(AmbientGlobalState { allow_files }))
}

impl FileRule for AmbientGlobalState {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if self.allow_files.is_match(facts.path) {
            return;
        }
        for site in &facts.ambient {
            if site.kind == AmbientKind::EnvRead && in_main(facts, site.in_function) {
                continue;
            }
            let (message, fix) = describe(site.kind, site.name);
            out.push(Finding::at(
                facts,
                site.pos,
                INFO.id,
                Severity::Deny,
                message,
                fix,
            ));
        }
    }
}

fn in_main(facts: &FileFacts<'_>, in_function: Option<u32>) -> bool {
    in_function
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| facts.functions.get(index))
        .is_some_and(|function| matches!(function.name, "main" | "Main"))
}

fn describe(kind: AmbientKind, name: &str) -> (String, String) {
    match kind {
        AmbientKind::GlobalMutable => (format!("mutable global `{name}`"), OWNED_FIX.to_string()),
        AmbientKind::GlobalWrite => (
            format!("function writes global `{name}`"),
            OWNED_FIX.to_string(),
        ),
        AmbientKind::EnvRead => (ENV_MESSAGE.to_string(), ENV_FIX.to_string()),
    }
}
