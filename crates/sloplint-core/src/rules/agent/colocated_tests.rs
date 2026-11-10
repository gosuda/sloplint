use std::collections::{HashMap, HashSet};

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
        let imported = import_covered(files);
        let mut evidenced: HashSet<(Lang, &str)> = HashSet::new();
        for file in files {
            if is_evidence(file) {
                evidenced.insert((file.facts.lang, file.facts.package.as_str()));
            }
        }
        for file in files {
            if self.skipped(file) {
                continue;
            }
            if !evidenced.contains(&(file.facts.lang, file.facts.package.as_str())) {
                continue;
            }
            if imported.contains(file.path) {
                continue;
            }
            out.push(self.finding(file));
        }
    }
}

impl ColocatedTests {
    fn skipped(&self, file: &WorldFile<'_>) -> bool {
        let facts: &WorldFacts = file.facts;
        if facts.role != CodeRole::Library
            || facts.public_functions == 0
            || self.exclude.is_match(file.path)
        {
            return true;
        }
        crate::module_keys::library_keys(facts.lang, file.path).is_empty()
    }

    fn finding(&self, file: &WorldFile<'_>) -> Finding {
        let package = file.facts.package.as_str();
        let scope = if package.is_empty() {
            String::new()
        } else {
            format!(" `{package}`")
        };
        Finding::at_file(
            file.path,
            INFO.id,
            Severity::Info,
            format!(
                "module with {} public functions is not named by any test in its package{scope} (no test file or test import shares its module name)",
                file.facts.public_functions
            ),
            if package.is_empty() {
                "add tests covering this module".to_string()
            } else {
                format!("add tests covering package `{package}`")
            },
        )
    }
}

fn is_evidence(file: &WorldFile<'_>) -> bool {
    file.facts.has_inline_tests
        || matches!(file.facts.role, CodeRole::Test | CodeRole::Bench)
        || file
            .facts
            .regions
            .iter()
            .any(|region| matches!(region.role, CodeRole::Test | CodeRole::Bench))
}

fn import_covered<'a>(files: &[WorldFile<'a>]) -> HashSet<&'a str> {
    let mut keys: HashMap<(&'a str, String), usize> = HashMap::new();
    for file in files {
        if !is_evidence(file) {
            continue;
        }
        record_library_keys(&mut keys, file.facts.lang, file.path);
        for spec in &file.facts.imports {
            record_import_keys(&mut keys, file.facts.lang, file.path, spec);
        }
    }
    if keys.is_empty() {
        return HashSet::new();
    }
    let mut covered: HashSet<&'a str> = HashSet::new();
    for file in files {
        if file.facts.role != CodeRole::Library || file.facts.public_functions == 0 {
            continue;
        }
        if is_covered(&keys, file.facts.lang, file.path) {
            covered.insert(file.path);
        }
    }
    covered
}

fn key_len(key: &str) -> usize {
    key.split('/').count()
}

fn record_library_keys(keys: &mut HashMap<(&str, String), usize>, lang: Lang, path: &str) {
    let owned = crate::module_keys::test_keys(lang, path);
    let longest = owned.iter().map(|key| key_len(key)).max().unwrap_or(0);
    if longest == 0 {
        return;
    }
    let scope = lang.config_key();
    for key in owned {
        keys.entry((scope, key))
            .and_modify(|need| *need = (*need).min(longest))
            .or_insert(longest);
    }
}

fn record_import_keys(
    keys: &mut HashMap<(&str, String), usize>,
    lang: Lang,
    importer: &str,
    spec: &str,
) {
    let owned = crate::module_keys::import_keys(lang, importer, spec);
    let longest = owned.iter().map(|key| key_len(key)).max().unwrap_or(0);
    if longest == 0 {
        return;
    }
    let scope = lang.config_key();
    for key in owned {
        keys.entry((scope, key))
            .and_modify(|need| *need = (*need).min(longest))
            .or_insert(longest);
    }
}

fn is_covered(keys: &HashMap<(&str, String), usize>, lang: Lang, path: &str) -> bool {
    let owned = crate::module_keys::library_keys(lang, path);
    let longest = owned.iter().map(|key| key_len(key)).max().unwrap_or(0);
    let scope = lang.config_key();
    owned.iter().any(|key| {
        keys.get(&(scope, key.clone()))
            .is_some_and(|&need| need <= key_len(key) || key_len(key) == longest)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::WorldFacts;
    use crate::lang::Lang;
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
            imports: Vec::new(),
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
    fn evidence(lang: Lang, package: &str, imports: &[&str]) -> WorldFacts {
        WorldFacts {
            lang,
            role: CodeRole::Test,
            regions: Vec::new(),
            package: package.to_string(),
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
            imports: imports.iter().map(|spec| Box::<str>::from(*spec)).collect(),
        }
    }
    fn test_file(lang: Lang, package: &str) -> WorldFacts {
        WorldFacts {
            lang,
            role: CodeRole::Test,
            regions: Vec::new(),
            package: package.to_string(),
            has_inline_tests: false,
            public_functions: 0,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
            imports: Vec::new(),
        }
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
    fn same_stem_test_covers_while_bare_files_flag() {
        let rule = rule();
        let yaml = lib(Lang::Go, "yaml", 4);
        let decode_test = test_file(Lang::Go, "yaml");
        let error = lib(Lang::Rust, "anyhow", 2);
        let source_test = inline_test(Lang::Rust, "anyhow");
        let lonely = lib(Lang::Rust, "lonely", 1);
        let lonely_probe = test_file(Lang::Rust, "lonely");
        let files = [
            WorldFile {
                path: "yaml/yaml.go",
                facts: &yaml,
            },
            WorldFile {
                path: "yaml/decode_test.go",
                facts: &decode_test,
            },
            WorldFile {
                path: "src/error.rs",
                facts: &error,
            },
            WorldFile {
                path: "src/error_test.rs",
                facts: &source_test,
            },
            WorldFile {
                path: "src/lonely.rs",
                facts: &lonely,
            },
            WorldFile {
                path: "tests/other.rs",
                facts: &lonely_probe,
            },
        ];
        assert_eq!(
            check(&rule, &files),
            ["src/lonely.rs".to_string()],
            "affix-stripped stems share keys, bare files share none"
        );
    }

    #[test]
    fn support_and_prefixed_files_are_not_test_evidence() {
        let rule = rule();
        let lonely = lib(Lang::Rust, "lonely", 1);
        let helper = support(Lang::Rust, "lonely");
        let prefixed = lib(Lang::Rust, "lonely", 1);
        let other_test = test_file(Lang::Rust, "lonely");
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
            WorldFile {
                path: "tests/test_other.rs",
                facts: &other_test,
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
        let go_test = test_file(Lang::Go, "cmd/tool");
        let rs_test = test_file(Lang::Rust, "app");
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
            WorldFile {
                path: "cmd/other/other_test.go",
                facts: &go_test,
            },
            WorldFile {
                path: "tests/test_other.rs",
                facts: &rs_test,
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

    #[test]
    fn silence_without_any_test_evidence() {
        let rule = rule();
        let service = lib(Lang::Go, "service", 1);
        let render = lib(Lang::Rust, "render", 1);
        let files = [
            WorldFile {
                path: "service.go",
                facts: &service,
            },
            WorldFile {
                path: "src/render.rs",
                facts: &render,
            },
        ];
        assert_eq!(
            check(&rule, &files),
            Vec::<String>::new(),
            "a run with no test evidence must stay silent"
        );
    }

    #[test]
    fn hyphenated_nested_rust_import_covers_only_named_module() {
        let rule = rule();
        let lexer = lib(Lang::Rust, "my-parser", 1);
        let other = lib(Lang::Rust, "my-parser", 1);
        let test = evidence(Lang::Rust, "my-parser", &["my_parser::rules::lexer"]);
        let files = [
            WorldFile {
                path: "libs/my-parser/src/rules/lexer.rs",
                facts: &lexer,
            },
            WorldFile {
                path: "libs/my-parser/src/rules/other.rs",
                facts: &other,
            },
            WorldFile {
                path: "crates/e2e/tests/t.rs",
                facts: &test,
            },
        ];
        assert_eq!(
            check(&rule, &files),
            ["libs/my-parser/src/rules/other.rs".to_string()],
            "hyphen maps to underscore and nested segments resolve"
        );
    }

    #[test]
    fn python_double_dot_resolves_to_parent_package() {
        let rule = rule();
        let target = lib(Lang::Python, "tests", 1);
        let sibling = lib(Lang::Python, "app", 1);
        let test = evidence(Lang::Python, "tests", &["..core"]);
        let sibling_probe = test_file(Lang::Python, "app");
        let files = [
            WorldFile {
                path: "tests/core.py",
                facts: &target,
            },
            WorldFile {
                path: "app/unit/core.py",
                facts: &sibling,
            },
            WorldFile {
                path: "tests/unit/test_x.py",
                facts: &test,
            },
            WorldFile {
                path: "app/test_other.py",
                facts: &sibling_probe,
            },
        ];
        assert_eq!(
            check(&rule, &files),
            ["app/unit/core.py".to_string()],
            "two dots pop one directory before matching"
        );
    }

    #[test]
    fn production_import_does_not_cover() {
        let rule = rule();
        let core = lib(Lang::Python, "pkg", 1);
        let other = lib(Lang::Python, "pkg", 1);
        let mut prod = lib(Lang::Python, "pkg", 0);
        prod.imports = vec![Box::<str>::from("pkg/core")];
        let test = evidence(Lang::Python, "pkg", &["pkg/other"]);
        let files = [
            WorldFile {
                path: "pkg/core.py",
                facts: &core,
            },
            WorldFile {
                path: "pkg/other.py",
                facts: &other,
            },
            WorldFile {
                path: "pkg/user.py",
                facts: &prod,
            },
            WorldFile {
                path: "tests/test_other.py",
                facts: &test,
            },
        ];
        assert_eq!(
            check(&rule, &files),
            ["pkg/core.py".to_string()],
            "an import from non-test code must not cover"
        );
    }

    #[test]
    fn evidence_path_keys_do_not_cover_twin_modules() {
        let rule = rule();
        let tested = inline_test(Lang::Rust, "pkga");
        let twin = lib(Lang::Rust, "pkgb", 1);
        let twin_probe = test_file(Lang::Rust, "pkgb");
        let files = [
            WorldFile {
                path: "pkga/util/core.rs",
                facts: &tested,
            },
            WorldFile {
                path: "pkgb/util/core.rs",
                facts: &twin,
            },
            WorldFile {
                path: "pkgb/tests/other.rs",
                facts: &twin_probe,
            },
        ];
        assert_eq!(
            check(&rule, &files),
            ["pkgb/util/core.rs".to_string()],
            "a tested file's own path keys must not cover a same-suffix module elsewhere"
        );
    }
}
