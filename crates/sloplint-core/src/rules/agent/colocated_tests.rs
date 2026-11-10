use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::WorldFacts;
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

use super::string_values;

const EXCLUDE: &[&str] = &["**/examples/**", "**/scripts/**"];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/colocated-tests",
    family: Family::Agent,
    summary: "modules with public functions and no tests nearby",
    why: "A module without nearby tests gives no executable specification to check an edit against.",
    exempt_tests: false,
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
        Strictness::Strict => Level::Warn,
    };
    let mut options = toml::Table::new();
    options.insert("exclude".to_string(), string_values(EXCLUDE));
    Preset { level, options }
}

fn build(options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    let mut builder = GlobSetBuilder::new();
    for pattern in &parsed.exclude {
        let glob =
            Glob::new(pattern).map_err(|error| format!("invalid glob `{pattern}`: {error}"))?;
        builder.add(glob);
    }
    let exclude = builder.build().map_err(|error| error.to_string())?;
    Ok(Box::new(ColocatedTests { exclude }))
}

#[derive(Clone, Copy)]
enum TestCandidates {
    Names(NameForm),
    Dir(&'static str),
    Root(&'static [&'static str]),
    RustTests,
    Web,
    None,
}

#[derive(Clone, Copy)]
struct NameForm {
    prefixes: &'static [&'static str],
    infixes: &'static [&'static str],
    ext: Option<&'static str>,
}

const TYPED_SUFFIXES: NameForm = NameForm {
    prefixes: &[],
    infixes: &["Test", "Tests"],
    ext: None,
};

fn conventions(lang: Lang) -> TestCandidates {
    const TABLE: &[(Lang, TestCandidates)] = &[
        (Lang::Rust, TestCandidates::RustTests),
        (Lang::TypeScript, TestCandidates::Web),
        (Lang::Tsx, TestCandidates::Web),
        (Lang::JavaScript, TestCandidates::Web),
        (Lang::Java, TestCandidates::Names(TYPED_SUFFIXES)),
        (Lang::Kotlin, TestCandidates::Names(TYPED_SUFFIXES)),
        (Lang::CSharp, TestCandidates::Names(TYPED_SUFFIXES)),
        (
            Lang::Scala,
            TestCandidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Spec", "Test", "Suite"],
                ext: Some("scala"),
            }),
        ),
        (
            Lang::Swift,
            TestCandidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Tests"],
                ext: Some("swift"),
            }),
        ),
        (
            Lang::ObjC,
            TestCandidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Tests"],
                ext: Some("m"),
            }),
        ),
        (
            Lang::Php,
            TestCandidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Test"],
                ext: Some("php"),
            }),
        ),
        (
            Lang::PowerShell,
            TestCandidates::Names(NameForm {
                prefixes: &[],
                infixes: &[".Tests"],
                ext: Some("ps1"),
            }),
        ),
        (
            Lang::Python,
            TestCandidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("py"),
            }),
        ),
        (
            Lang::Bash,
            TestCandidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("sh"),
            }),
        ),
        (
            Lang::Ocaml,
            TestCandidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("ml"),
            }),
        ),
        (
            Lang::OcamlInterface,
            TestCandidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("ml"),
            }),
        ),
        (
            Lang::C,
            TestCandidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test", "_unittest"],
                ext: None,
            }),
        ),
        (
            Lang::Cpp,
            TestCandidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test", "_unittest"],
                ext: None,
            }),
        ),
        (Lang::Go, TestCandidates::Dir("_test.go")),
        (Lang::Dart, TestCandidates::Root(&["test/{stem}_test.dart"])),
        (
            Lang::Ruby,
            TestCandidates::Root(&["spec/{stem}_spec.rb", "test/{stem}_test.rb"]),
        ),
        (
            Lang::R,
            TestCandidates::Root(&["tests/testthat/test-{stem}.R"]),
        ),
        (Lang::Zig, TestCandidates::None),
        (Lang::Odin, TestCandidates::None),
        (Lang::Starlark, TestCandidates::None),
    ];
    TABLE
        .iter()
        .find(|(candidate, _)| *candidate == lang)
        .map(|(_, candidates)| *candidates)
        .unwrap_or(TestCandidates::None)
}

fn cmp_against_joined(elem: &str, parts: &[&str]) -> std::cmp::Ordering {
    let mut elem_bytes = elem.as_bytes();
    for &part in parts {
        let part_bytes = part.as_bytes();
        let common = elem_bytes.len().min(part_bytes.len());
        match elem_bytes[..common].cmp(&part_bytes[..common]) {
            std::cmp::Ordering::Equal => {
                if elem_bytes.len() < part_bytes.len() {
                    return std::cmp::Ordering::Less;
                }
                elem_bytes = &elem_bytes[common..];
            }
            ord => return ord,
        }
    }
    if elem_bytes.is_empty() {
        std::cmp::Ordering::Equal
    } else {
        std::cmp::Ordering::Greater
    }
}

struct PathIndex<'a> {
    paths: Vec<&'a str>,
    names: Vec<&'a str>,
    rust_tests: Vec<&'a str>,
}

impl<'a> PathIndex<'a> {
    fn new(files: &[WorldFile<'a>]) -> PathIndex<'a> {
        let mut paths: Vec<&'a str> = files.iter().map(|file| file.path).collect();
        paths.sort_unstable();
        paths.dedup();
        let mut rust_tests: Vec<&'a str> = files
            .iter()
            .map(|file| file.path)
            .filter(|path| under_tests(path))
            .map(file_name)
            .collect();
        rust_tests.sort_unstable();
        rust_tests.dedup();
        let mut names: Vec<&'a str> = files.iter().map(|file| file_name(file.path)).collect();
        names.sort_unstable();
        names.dedup();
        PathIndex {
            paths,
            names,
            rust_tests,
        }
    }
}

fn has_counterpart(index: &PathIndex<'_>, path: &str, lang: Lang) -> bool {
    match conventions(lang) {
        TestCandidates::Names(form) => names_match(&index.names, path, &form),
        TestCandidates::Dir(tail) => dir_match(&index.paths, path, tail),
        TestCandidates::Root(templates) => root_match(&index.paths, templates, path),
        TestCandidates::RustTests => rust_match(&index.rust_tests, path),
        TestCandidates::Web => web_match(&index.paths, path),
        TestCandidates::None => false,
    }
}

fn names_match(names: &[&str], path: &str, form: &NameForm) -> bool {
    let (stem, own_ext) = stem_ext(path);
    let ext = form.ext.unwrap_or(own_ext);
    form.prefixes.iter().any(|prefix| {
        names
            .binary_search_by(|elem| cmp_against_joined(elem, &[prefix, stem, ".", ext]))
            .is_ok()
    }) || form.infixes.iter().any(|infix| {
        names
            .binary_search_by(|elem| cmp_against_joined(elem, &[stem, infix, ".", ext]))
            .is_ok()
    })
}

fn dir_match(paths: &[&str], path: &str, tail: &str) -> bool {
    let dir = dir_prefix(path);
    let stem = stem_ext(path).0;
    paths
        .binary_search_by(|elem| cmp_against_joined(elem, &[dir, stem, tail]))
        .is_ok()
}

fn root_match(paths: &[&str], templates: &[&str], path: &str) -> bool {
    let stem = stem_ext(path).0;
    templates
        .iter()
        .any(|template| match template.split_once("{stem}") {
            Some((prefix, suffix)) => paths
                .binary_search_by(|elem| cmp_against_joined(elem, &[prefix, stem, suffix]))
                .is_ok(),
            None => paths.binary_search(template).is_ok(),
        })
}

fn rust_match(test_names: &[&str], path: &str) -> bool {
    let stem = stem_ext(path).0;
    if test_names
        .binary_search_by(|elem| cmp_against_joined(elem, &[stem, ".rs"]))
        .is_ok()
    {
        return true;
    }
    let start = test_names.partition_point(|name| cmp_against_joined(name, &[stem, "_"]).is_lt());
    test_names[start..].first().is_some_and(|name| {
        name.strip_prefix(stem)
            .is_some_and(|rest| rest.starts_with('_'))
    })
}

fn under_tests(path: &str) -> bool {
    path.starts_with("tests/") || path.contains("/tests/")
}

const WEB_TAILS: &[&str] = &[
    ".test.ts",
    ".test.tsx",
    ".test.js",
    ".test.jsx",
    ".test.mjs",
    ".test.cjs",
    ".spec.ts",
    ".spec.tsx",
    ".spec.js",
    ".spec.jsx",
    ".spec.mjs",
    ".spec.cjs",
];

fn web_match(paths: &[&str], path: &str) -> bool {
    let dir = dir_prefix(path);
    let stem = stem_ext(path).0;
    if WEB_TAILS.iter().any(|tail| {
        paths
            .binary_search_by(|elem| cmp_against_joined(elem, &[dir, stem, tail]))
            .is_ok()
    }) {
        return true;
    }
    let start = paths.partition_point(|candidate| {
        cmp_against_joined(candidate, &[dir, "__tests__/", stem, ".test."]).is_lt()
    });
    paths[start..].first().is_some_and(|candidate| {
        candidate
            .strip_prefix(dir)
            .and_then(|c| c.strip_prefix("__tests__/"))
            .and_then(|c| c.strip_prefix(stem))
            .is_some_and(|c| c.starts_with(".test."))
    })
}

#[derive(Clone, Copy)]
enum Counterpart {
    Text(&'static str),
    DirStem(&'static str),
    Root(&'static str),
    Infix(&'static str, Option<&'static str>),
}

fn counterpart(lang: Lang) -> Counterpart {
    const TABLE: &[(Lang, Counterpart)] = &[
        (
            Lang::Rust,
            Counterpart::Text("a #[cfg(test)] mod tests in this file"),
        ),
        (Lang::TypeScript, Counterpart::DirStem(".test.ts")),
        (Lang::Tsx, Counterpart::DirStem(".test.ts")),
        (Lang::JavaScript, Counterpart::DirStem(".test.js")),
        (Lang::Python, Counterpart::Root("tests/test_{stem}.py")),
        (Lang::Go, Counterpart::DirStem("_test.go")),
        (Lang::Dart, Counterpart::Root("test/{stem}_test.dart")),
        (Lang::Ruby, Counterpart::Root("spec/{stem}_spec.rb")),
        (Lang::R, Counterpart::Root("tests/testthat/test-{stem}.R")),
        (Lang::Zig, Counterpart::Text("a test block in this file")),
        (
            Lang::Odin,
            Counterpart::Text("an @(test) procedure in this file"),
        ),
        (Lang::Java, Counterpart::Infix("Test", None)),
        (Lang::Kotlin, Counterpart::Infix("Test", None)),
        (Lang::CSharp, Counterpart::Infix("Test", None)),
        (Lang::Scala, Counterpart::Infix("Spec", Some("scala"))),
        (Lang::Swift, Counterpart::Infix("Tests", Some("swift"))),
        (Lang::ObjC, Counterpart::Infix("Tests", Some("m"))),
        (Lang::Php, Counterpart::Infix("Test", Some("php"))),
        (Lang::PowerShell, Counterpart::Infix(".Tests", Some("ps1"))),
        (Lang::Bash, Counterpart::Infix("_test", Some("sh"))),
        (Lang::Ocaml, Counterpart::Infix("_test", Some("ml"))),
        (
            Lang::OcamlInterface,
            Counterpart::Infix("_test", Some("ml")),
        ),
        (Lang::C, Counterpart::Infix("_test", None)),
        (Lang::Cpp, Counterpart::Infix("_test", None)),
        (Lang::Starlark, Counterpart::Text("")),
    ];
    TABLE
        .iter()
        .find(|(candidate, _)| *candidate == lang)
        .map(|(_, counterpart)| *counterpart)
        .unwrap_or(Counterpart::Text(""))
}

fn expected(path: &str, lang: Lang) -> String {
    let dir = dir_prefix(path);
    let (stem, ext) = stem_ext(path);
    match counterpart(lang) {
        Counterpart::Text(text) => text.to_string(),
        Counterpart::DirStem(tail) => format!("{dir}{stem}{tail}"),
        Counterpart::Root(template) => template.replace("{stem}", stem),
        Counterpart::Infix(infix, fixed) => format!("{dir}{stem}{infix}.{}", fixed.unwrap_or(ext)),
    }
}

fn dir_prefix(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..=index],
        None => "",
    }
}

fn stem_ext(path: &str) -> (&str, &str) {
    let name = file_name(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, ext),
        None => (name, ""),
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl WorldRule for ColocatedTests {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        let index = PathIndex::new(files);
        for file in files {
            if self.skipped(file) || file.facts.has_inline_tests {
                continue;
            }
            if has_counterpart(&index, file.path, file.facts.lang) {
                continue;
            }
            out.push(self.finding(file));
        }
    }
}

impl ColocatedTests {
    fn skipped(&self, file: &WorldFile<'_>) -> bool {
        let facts: &WorldFacts = file.facts;
        facts.is_test_file
            || facts.is_entry_file
            || facts.public_functions == 0
            || facts.lang == Lang::Starlark
            || self.exclude.is_match(file.path)
    }

    fn finding(&self, file: &WorldFile<'_>) -> Finding {
        let expected_path = expected(file.path, file.facts.lang);
        Finding::at_file(
            file.path,
            INFO.id,
            Severity::Warn,
            format!(
                "module with {} public functions has no tests",
                file.facts.public_functions
            ),
            format!("add {expected_path}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::WorldFacts;
    use globset::GlobSetBuilder;

    fn module(public_functions: u32, has_inline_tests: bool) -> WorldFacts {
        WorldFacts {
            lang: Lang::Rust,
            is_test_file: false,
            is_entry_file: false,
            has_inline_tests,
            public_functions,
            definitions: Vec::new(),
            abstractions: Vec::new(),
            impls: Vec::new(),
            barrel: None,
        }
    }

    #[test]
    fn findings_follow_file_order_and_prefix_counterparts_suppress() {
        let rule = ColocatedTests {
            exclude: GlobSetBuilder::new().build().unwrap(),
        };
        let zed = module(1, false);
        let ay = module(1, false);
        let widget = module(1, false);
        let counterpart = module(0, false);
        let files = [
            WorldFile {
                path: "z.rs",
                facts: &zed,
            },
            WorldFile {
                path: "a.rs",
                facts: &ay,
            },
            WorldFile {
                path: "w.rs",
                facts: &widget,
            },
            WorldFile {
                path: "tests/w_extra.rs",
                facts: &counterpart,
            },
        ];
        let mut out = Vec::new();
        rule.check(&files, &mut out);
        let order: Vec<String> = out.iter().map(|finding| finding.path.to_string()).collect();
        assert_eq!(order, ["z.rs".to_string(), "a.rs".to_string()]);
    }
}
