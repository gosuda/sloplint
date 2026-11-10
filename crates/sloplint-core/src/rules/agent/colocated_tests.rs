use std::collections::HashSet;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::WorldFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{CodeRole, RolePolicy, Shift};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};
use crate::rules::{parse_options, string_values};

const EXCLUDE: &[&str] = &[
    "**/_*example/**",
    "**/ossfuzz/**",
    "**/main.rs",
    "**/main.go",
    "**/bin/**",
];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/colocated-tests",
    family: Family::Agent,
    summary: "modules with public functions and no tests nearby",
    why: "A module without nearby tests gives no executable specification to check an edit against.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Off,
        test: Shift::Off,
        support: Shift::Off,
        bench: Shift::Off,
        example: Shift::Off,
    },
    preset,
    build: Build::World(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    exclude: Vec<String>,
}

struct ColocatedTests {
    exclude: GlobSet,
}

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    let mut options = toml::Table::new();
    options.insert("exclude".to_string(), string_values(EXCLUDE));
    Preset { level, options }
}

fn build(options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    let parsed: Options = parse_options(options)?;
    let mut builder = GlobSetBuilder::new();
    for pattern in &parsed.exclude {
        let glob =
            Glob::new(pattern).map_err(|error| format!("invalid glob `{pattern}`: {error}"))?;
        builder.add(glob);
    }
    let exclude = builder.build().map_err(|error| error.to_string())?;
    Ok(Box::new(ColocatedTests { exclude }))
}

impl WorldRule for ColocatedTests {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        let tested = tested_packages(files);
        for file in files {
            if self.skipped(file) {
                continue;
            }
            if tested.contains(&(file.facts.lang.config_key(), file.facts.package.as_str())) {
                continue;
            }
            out.push(self.finding(file));
        }
    }
}

impl ColocatedTests {
    fn skipped(&self, file: &WorldFile<'_>) -> bool {
        let facts: &WorldFacts = file.facts;
        facts.role != CodeRole::Library
            || facts.public_functions == 0
            || facts.lang == Lang::Starlark
            || self.exclude.is_match(file.path)
    }

    fn finding(&self, file: &WorldFile<'_>) -> Finding {
        Finding::at_file(
            file.path,
            INFO.id,
            Severity::Info,
            format!(
                "module with {} public functions has no tests in package `{}`",
                file.facts.public_functions, file.facts.package
            ),
            format!("add tests covering package `{}`", file.facts.package),
        )
    }
}

fn tested_packages<'a>(files: &[WorldFile<'a>]) -> HashSet<(&'a str, &'a str)> {
    files
        .iter()
        .filter(|file| {
            file.facts.has_inline_tests || file.facts.role == CodeRole::Test || test_path(file.path)
        })
        .map(|file| (file.facts.lang.config_key(), file.facts.package.as_str()))
        .collect()
}

fn test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with("_test.go") || name.contains(".test.") || name.contains(".spec.") {
        return true;
    }
    path.split('/')
        .any(|segment| segment == "tests" || segment == "__tests__")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::WorldFacts;
    use globset::GlobSetBuilder;

    fn lib(lang: Lang, package: &str, public_functions: u32) -> WorldFacts {
        WorldFacts {
            lang,
            role: CodeRole::Library,
            regions: Vec::new(),
            package: package.to_string(),
            has_inline_tests: false,
            public_functions,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
        }
    }

    fn inline_test(lang: Lang, package: &str) -> WorldFacts {
        let mut facts = lib(lang, package, 0);
        facts.has_inline_tests = true;
        facts
    }

    fn support(lang: Lang, package: &str) -> WorldFacts {
        let mut facts = lib(lang, package, 0);
        facts.role = CodeRole::Support;
        facts
    }

    fn rule() -> ColocatedTests {
        ColocatedTests {
            exclude: GlobSetBuilder::new().build().unwrap(),
        }
    }

    fn check(rule: &ColocatedTests, files: &[WorldFile<'_>]) -> Vec<String> {
        let mut out = Vec::new();
        rule.check(files, &mut out);
        out.into_iter()
            .map(|finding| finding.path.to_string())
            .collect()
    }

    #[test]
    fn package_tests_cover_all_members_while_bare_packages_flag() {
        let rule = rule();
        let yaml = lib(Lang::Go, "yaml", 4);
        let decode_test = lib(Lang::Go, "yaml", 0);
        let error = lib(Lang::Rust, "anyhow", 2);
        let source_test = inline_test(Lang::Rust, "anyhow");
        let lonely = lib(Lang::Rust, "lonely", 1);
        let files = [
            WorldFile {
                path: "yaml.go",
                facts: &yaml,
            },
            WorldFile {
                path: "decode_test.go",
                facts: &decode_test,
            },
            WorldFile {
                path: "src/error.rs",
                facts: &error,
            },
            WorldFile {
                path: "tests/test_source.rs",
                facts: &source_test,
            },
            WorldFile {
                path: "src/lonely.rs",
                facts: &lonely,
            },
        ];
        assert_eq!(check(&rule, &files), ["src/lonely.rs".to_string()]);
    }

    #[test]
    fn support_and_prefixed_files_are_not_test_evidence() {
        let rule = rule();
        let lonely = lib(Lang::Rust, "lonely", 1);
        let helper = support(Lang::Rust, "lonely");
        let prefixed = lib(Lang::Rust, "lonely", 1);
        let files = [
            WorldFile {
                path: "src/lonely.rs",
                facts: &lonely,
            },
            WorldFile {
                path: "src/support.rs",
                facts: &helper,
            },
            WorldFile {
                path: "src/test_helpers.rs",
                facts: &prefixed,
            },
        ];
        let mut found = check(&rule, &files);
        found.sort();
        assert_eq!(
            found,
            [
                "src/lonely.rs".to_string(),
                "src/test_helpers.rs".to_string()
            ]
        );
    }

    #[test]
    fn default_excludes_skip_entry_only() {
        let preset = preset(Strictness::Strict);
        let rule = build(preset.options).expect("strict preset builds");
        let tool = lib(Lang::Go, "cmd/tool", 1);
        let app = lib(Lang::Rust, "app", 1);
        let main = lib(Lang::Rust, "binpkg", 1);
        let files = [
            WorldFile {
                path: "cmd/tool/lib.go",
                facts: &tool,
            },
            WorldFile {
                path: "src/app.rs",
                facts: &app,
            },
            WorldFile {
                path: "src/main.rs",
                facts: &main,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        let mut found: Vec<String> = out
            .into_iter()
            .map(|finding| finding.path.to_string())
            .collect();
        found.sort();
        assert_eq!(
            found,
            ["cmd/tool/lib.go".to_string(), "src/app.rs".to_string()]
        );
    }
}
