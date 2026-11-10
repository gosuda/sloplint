use std::collections::HashSet;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{AmbientKind, FileFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{parse_options, string_values};

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

const ENV_OBJECTS: &[&str] = &["os.environ", "process.env"];

const OWNED_FIX: &str = "move this state into a value owned by the caller and pass it explicitly";
const ENV_MESSAGE: &str = "process environment read outside entry or config code";
const ENV_FIX: &str = "read configuration once at the entry point and pass it in as a parameter";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/ambient-global-state",
    family: Family::Agent,
    summary: "mutable globals, global writes, and environment reads outside entry code",
    why: "Hidden global inputs make behavior depend on state the signature does not show.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Off,
        test: Shift::Off,
        support: Shift::Lower1,
        bench: Shift::Off,
        example: Shift::Off,
    },
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
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    let mut options = toml::Table::new();
    options.insert("allow_files".to_string(), string_values(ALLOW_FILES));
    Preset { level, options }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = parse_options(options)?;
    let mut builder = GlobSetBuilder::new();
    for pattern in &parsed.allow_files {
        let glob =
            Glob::new(pattern).map_err(|error| format!("invalid glob `{pattern}`: {error}"))?;
        builder.add(glob);
    }
    let allow_files = builder.build().map_err(|error| error.to_string())?;
    Ok(Box::new(AmbientGlobalState { allow_files }))
}

struct Joins<'a> {
    assigned: HashSet<&'a str>,
    storing: HashSet<u32>,
}

fn joins<'a>(facts: &'a FileFacts<'_>) -> Joins<'a> {
    let mut assigned: HashSet<&str> = HashSet::new();
    let mut declared: HashSet<&str> = HashSet::new();
    let mut storing: HashSet<u32> = HashSet::new();
    for site in &facts.ambient {
        if site.name == "_" {
            continue;
        }
        match site.kind {
            AmbientKind::GlobalMutable => {
                declared.insert(site.name);
            }
            AmbientKind::GlobalWrite => {
                if site.in_function.is_some() {
                    assigned.insert(site.name);
                    declared.insert(site.name);
                }
            }
            AmbientKind::EnvRead => {}
        }
    }
    for site in &facts.ambient {
        if site.kind != AmbientKind::GlobalWrite {
            continue;
        }
        let Some(index) = site.in_function else {
            continue;
        };
        if !ENV_OBJECTS.contains(&site.name) && declared.contains(site.name) {
            storing.insert(index);
        }
    }
    Joins { assigned, storing }
}

impl FileRule for AmbientGlobalState {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if self.allow_files.is_match(facts.path) {
            return;
        }
        let joins = joins(facts);
        let mut seen: HashSet<&str> = HashSet::new();
        for site in &facts.ambient {
            if site.name == "_" {
                continue;
            }
            match site.kind {
                AmbientKind::GlobalMutable => {
                    if joins.assigned.contains(site.name) && seen.insert(site.name) {
                        out.push(Finding::at(
                            facts,
                            site.pos,
                            INFO.id,
                            Severity::Major,
                            format!("mutable global `{}`", site.name),
                            OWNED_FIX.to_string(),
                        ));
                    }
                }
                AmbientKind::GlobalWrite => {
                    if ENV_OBJECTS.contains(&site.name) && seen.insert(site.name) {
                        out.push(Finding::at(
                            facts,
                            site.pos,
                            INFO.id,
                            Severity::Major,
                            format!("writes shared environment `{}`", site.name),
                            ENV_FIX.to_string(),
                        ));
                    }
                }
                AmbientKind::EnvRead => {
                    if site
                        .in_function
                        .is_some_and(|index| joins.storing.contains(&index))
                    {
                        out.push(Finding::at(
                            facts,
                            site.pos,
                            INFO.id,
                            Severity::Major,
                            ENV_MESSAGE.to_string(),
                            ENV_FIX.to_string(),
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Ambient, Function, Pos, Visibility};
    use crate::finding::line_starts;
    use crate::lang::Lang;

    fn stub(line: u32) -> Function<'static> {
        Function {
            name: "holder",
            pos: Pos { line, byte: 0 },
            doc: Vec::new(),
            doc_pos: None,
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            has_value_return: true,
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
            end_line: 100,
            sole_call: None,
        }
    }

    fn facts<'s>(source: &'s str, ambient: Vec<Ambient<'s>>) -> FileFacts<'s> {
        FileFacts {
            lang: Lang::Go,
            path: "pkg/store.go",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count: 0,
            package: String::new(),
            is_generated: false,
            functions: vec![stub(1), stub(2)],
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient,
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn messages(rule: &AmbientGlobalState, source: &str, ambient: Vec<Ambient<'_>>) -> Vec<String> {
        let mut out = Vec::new();
        rule.check(&facts(source, ambient), &mut out);
        out.into_iter().map(|finding| finding.message).collect()
    }

    fn rule() -> AmbientGlobalState {
        AmbientGlobalState {
            allow_files: GlobSetBuilder::new().build().unwrap(),
        }
    }

    fn site(kind: AmbientKind, name: &str, line: u32, in_function: Option<u32>) -> Ambient<'_> {
        Ambient {
            pos: Pos { line, byte: 0 },
            kind,
            name,
            in_function,
        }
    }

    #[test]
    fn initializer_only_global_stays_silent_while_assigned_global_flags() {
        let source = "package store\n\nvar cache = newCache()\n\nvar counter int\n";
        let found = messages(
            &rule(),
            source,
            vec![
                site(AmbientKind::GlobalMutable, "cache", 3, None),
                site(AmbientKind::GlobalMutable, "counter", 5, None),
                site(AmbientKind::GlobalWrite, "counter", 9, Some(0)),
            ],
        );
        assert_eq!(found, ["mutable global `counter`".to_string()]);
    }
    #[test]
    fn shadow_binding_without_global_write_stays_silent() {
        let source = "package p\n";
        let mut checked = facts(
            source,
            vec![site(AmbientKind::GlobalMutable, "counter", 3, None)],
        );
        checked.bindings.push(crate::facts::MutBinding {
            name: "counter",
            pos: Pos { line: 5, byte: 0 },
            last_write_line: 5,
            first_write_line: 5,
            write_lines: 0,
            loop_depth: 0,
        });
        let mut out = Vec::new();
        rule().check(&checked, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn blank_interface_assert_is_dropped_even_with_a_write() {
        let source = "package core\nvar _ = 0\n";
        let found = messages(
            &rule(),
            source,
            vec![
                site(AmbientKind::GlobalMutable, "_", 3, None),
                site(AmbientKind::GlobalWrite, "_", 8, Some(0)),
            ],
        );
        assert!(found.is_empty());
    }

    #[test]
    fn returned_env_read_stays_silent_but_stored_read_flags() {
        let source = "package config\n";
        let found = messages(
            &rule(),
            source,
            vec![
                site(AmbientKind::GlobalMutable, "config", 4, None),
                site(AmbientKind::EnvRead, "os.Getenv", 10, Some(0)),
                site(AmbientKind::EnvRead, "os.Getenv", 20, Some(1)),
                site(AmbientKind::GlobalWrite, "config", 21, Some(1)),
            ],
        );
        assert_eq!(
            found,
            [
                "mutable global `config`".to_string(),
                ENV_MESSAGE.to_string(),
            ]
        );
    }

    #[test]
    fn environment_object_write_flags_while_plain_global_write_stays_silent() {
        let source = "import os\n";
        let found = messages(
            &rule(),
            source,
            vec![
                site(AmbientKind::GlobalWrite, "os.environ", 8, Some(0)),
                site(AmbientKind::GlobalWrite, "cache", 9, Some(0)),
            ],
        );
        assert_eq!(
            found,
            ["writes shared environment `os.environ`".to_string()]
        );
    }
    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "ambient-global-state ships at low confidence"
        );
        assert_eq!(
            preset(Strictness::Minimal).level,
            Level::Off,
            "minimal stays off"
        );
        assert_eq!(
            preset(Strictness::Basic).level,
            Level::Off,
            "basic stays off"
        );
        assert_eq!(
            preset(Strictness::Standard).level,
            Level::Off,
            "standard stays off"
        );
        let strict_preset = preset(Strictness::Strict);
        assert_eq!(
            strict_preset.level,
            Level::At(Severity::Info),
            "strict reports at info"
        );
        let rule =
            build(strict_preset.options, &Settings::default()).expect("strict preset builds");
        let source = "package store\n\nvar counter int\n";
        let ambient = vec![
            site(AmbientKind::GlobalMutable, "counter", 3, None),
            site(AmbientKind::GlobalWrite, "counter", 9, Some(0)),
        ];
        let mut out = Vec::new();
        rule.check(&facts(source, ambient), &mut out);
        assert_eq!(
            out.len(),
            1,
            "strict flags the assigned global on a positive input"
        );
    }
}
