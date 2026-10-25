//! `agent/colocated-tests`: modules with public functions and no nearby tests.

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{Pos, WorldFacts};
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};

use super::string_values;

const EXCLUDE: &[&str] = &["**/examples/**", "**/scripts/**"];

/// `agent/colocated-tests`: flags testable modules with no counterpart or inline tests.
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

/// Where a language looks for a module's tests.
#[derive(Clone, Copy)]
enum Candidates {
    /// File names `{prefix}{stem}.{ext}` or `{stem}{infix}.{ext}` anywhere; `None` keeps the file's own extension.
    Names(NameForm),
    /// `<dir>{stem}{tail}` next to the analyzed file.
    Dir(&'static str),
    /// Fixed root-relative paths, each with one `{stem}` placeholder or none.
    Root(&'static [&'static str]),
    /// Any file under a `tests/` directory named `<stem>.rs` or starting with `<stem>_`.
    RustTests,
    /// `.test.*`, `.spec.*`, and `__tests__/` forms next to the analyzed file.
    Web,
    /// Inline tests only.
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

fn conventions(lang: Lang) -> Candidates {
    const TABLE: &[(Lang, Candidates)] = &[
        (Lang::Rust, Candidates::RustTests),
        (Lang::TypeScript, Candidates::Web),
        (Lang::Tsx, Candidates::Web),
        (Lang::JavaScript, Candidates::Web),
        (Lang::Java, Candidates::Names(TYPED_SUFFIXES)),
        (Lang::Kotlin, Candidates::Names(TYPED_SUFFIXES)),
        (Lang::CSharp, Candidates::Names(TYPED_SUFFIXES)),
        (
            Lang::Scala,
            Candidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Spec", "Test", "Suite"],
                ext: Some("scala"),
            }),
        ),
        (
            Lang::Swift,
            Candidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Tests"],
                ext: Some("swift"),
            }),
        ),
        (
            Lang::ObjC,
            Candidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Tests"],
                ext: Some("m"),
            }),
        ),
        (
            Lang::Php,
            Candidates::Names(NameForm {
                prefixes: &[],
                infixes: &["Test"],
                ext: Some("php"),
            }),
        ),
        (
            Lang::PowerShell,
            Candidates::Names(NameForm {
                prefixes: &[],
                infixes: &[".Tests"],
                ext: Some("ps1"),
            }),
        ),
        (
            Lang::Python,
            Candidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("py"),
            }),
        ),
        (
            Lang::Bash,
            Candidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("sh"),
            }),
        ),
        (
            Lang::Ocaml,
            Candidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("ml"),
            }),
        ),
        (
            Lang::OcamlInterface,
            Candidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test"],
                ext: Some("ml"),
            }),
        ),
        (
            Lang::C,
            Candidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test", "_unittest"],
                ext: None,
            }),
        ),
        (
            Lang::Cpp,
            Candidates::Names(NameForm {
                prefixes: &["test_"],
                infixes: &["_test", "_unittest"],
                ext: None,
            }),
        ),
        (Lang::Go, Candidates::Dir("_test.go")),
        (Lang::Dart, Candidates::Root(&["test/{stem}_test.dart"])),
        (
            Lang::Ruby,
            Candidates::Root(&["spec/{stem}_spec.rb", "test/{stem}_test.rb"]),
        ),
        (Lang::R, Candidates::Root(&["tests/testthat/test-{stem}.R"])),
        (Lang::Zig, Candidates::None),
        (Lang::Odin, Candidates::None),
        (Lang::Starlark, Candidates::None),
    ];
    TABLE
        .iter()
        .find(|(candidate, _)| *candidate == lang)
        .map(|(_, candidates)| *candidates)
        .unwrap_or(Candidates::None)
}

/// Orders `elem` against the concatenation of `parts` without building that string.
fn cmp_composite(elem: &str, parts: &[&str]) -> std::cmp::Ordering {
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

/// The analyzed path set, indexed once per run so each module answers in `O(log n)`.
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
        Candidates::Names(form) => names_match(&index.names, path, &form),
        Candidates::Dir(tail) => dir_match(&index.paths, path, tail),
        Candidates::Root(templates) => root_match(&index.paths, templates, path),
        Candidates::RustTests => rust_match(&index.rust_tests, path),
        Candidates::Web => web_match(&index.paths, path),
        Candidates::None => false,
    }
}

fn names_match(names: &[&str], path: &str, form: &NameForm) -> bool {
    let (stem, own_ext) = stem_ext(path);
    let ext = form.ext.unwrap_or(own_ext);
    form.prefixes.iter().any(|prefix| {
        names
            .binary_search_by(|elem| cmp_composite(elem, &[prefix, stem, ".", ext]))
            .is_ok()
    }) || form.infixes.iter().any(|infix| {
        names
            .binary_search_by(|elem| cmp_composite(elem, &[stem, infix, ".", ext]))
            .is_ok()
    })
}

fn dir_match(paths: &[&str], path: &str, tail: &str) -> bool {
    let dir = dir_prefix(path);
    let stem = stem_ext(path).0;
    paths
        .binary_search_by(|elem| cmp_composite(elem, &[dir, stem, tail]))
        .is_ok()
}

fn root_match(paths: &[&str], templates: &[&str], path: &str) -> bool {
    let stem = stem_ext(path).0;
    templates
        .iter()
        .any(|template| match template.split_once("{stem}") {
            Some((prefix, suffix)) => paths
                .binary_search_by(|elem| cmp_composite(elem, &[prefix, stem, suffix]))
                .is_ok(),
            None => paths.binary_search(template).is_ok(),
        })
}

fn rust_match(test_names: &[&str], path: &str) -> bool {
    let stem = stem_ext(path).0;
    if test_names
        .binary_search_by(|elem| cmp_composite(elem, &[stem, ".rs"]))
        .is_ok()
    {
        return true;
    }
    let start = test_names.partition_point(|name| cmp_composite(name, &[stem, "_"]).is_lt());
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
            .binary_search_by(|elem| cmp_composite(elem, &[dir, stem, tail]))
            .is_ok()
    }) {
        return true;
    }
    let start = paths.partition_point(|candidate| {
        cmp_composite(candidate, &[dir, "__tests__/", stem, ".test."]).is_lt()
    });
    paths[start..].first().is_some_and(|candidate| {
        candidate
            .strip_prefix(dir)
            .and_then(|c| c.strip_prefix("__tests__/"))
            .and_then(|c| c.strip_prefix(stem))
            .is_some_and(|c| c.starts_with(".test."))
    })
}

/// The fix text names the canonical counterpart a module should gain.
#[derive(Clone, Copy)]
enum Fixup {
    Text(&'static str),
    DirStem(&'static str),
    Root(&'static str),
    Infix(&'static str, Option<&'static str>),
}

fn fixup(lang: Lang) -> Fixup {
    const TABLE: &[(Lang, Fixup)] = &[
        (
            Lang::Rust,
            Fixup::Text("a #[cfg(test)] mod tests in this file"),
        ),
        (Lang::TypeScript, Fixup::DirStem(".test.ts")),
        (Lang::Tsx, Fixup::DirStem(".test.ts")),
        (Lang::JavaScript, Fixup::DirStem(".test.js")),
        (Lang::Python, Fixup::Root("tests/test_{stem}.py")),
        (Lang::Go, Fixup::DirStem("_test.go")),
        (Lang::Dart, Fixup::Root("test/{stem}_test.dart")),
        (Lang::Ruby, Fixup::Root("spec/{stem}_spec.rb")),
        (Lang::R, Fixup::Root("tests/testthat/test-{stem}.R")),
        (Lang::Zig, Fixup::Text("a test block in this file")),
        (Lang::Odin, Fixup::Text("an @(test) procedure in this file")),
        (Lang::Java, Fixup::Infix("Test", None)),
        (Lang::Kotlin, Fixup::Infix("Test", None)),
        (Lang::CSharp, Fixup::Infix("Test", None)),
        (Lang::Scala, Fixup::Infix("Spec", Some("scala"))),
        (Lang::Swift, Fixup::Infix("Tests", Some("swift"))),
        (Lang::ObjC, Fixup::Infix("Tests", Some("m"))),
        (Lang::Php, Fixup::Infix("Test", Some("php"))),
        (Lang::PowerShell, Fixup::Infix(".Tests", Some("ps1"))),
        (Lang::Bash, Fixup::Infix("_test", Some("sh"))),
        (Lang::Ocaml, Fixup::Infix("_test", Some("ml"))),
        (Lang::OcamlInterface, Fixup::Infix("_test", Some("ml"))),
        (Lang::C, Fixup::Infix("_test", None)),
        (Lang::Cpp, Fixup::Infix("_test", None)),
        (Lang::Starlark, Fixup::Text("")),
    ];
    TABLE
        .iter()
        .find(|(candidate, _)| *candidate == lang)
        .map(|(_, fixup)| *fixup)
        .unwrap_or(Fixup::Text(""))
}

fn expected(path: &str, lang: Lang) -> String {
    let dir = dir_prefix(path);
    let (stem, ext) = stem_ext(path);
    match fixup(lang) {
        Fixup::Text(text) => text.to_string(),
        Fixup::DirStem(tail) => format!("{dir}{stem}{tail}"),
        Fixup::Root(template) => template.replace("{stem}", stem),
        Fixup::Infix(infix, fixed) => format!("{dir}{stem}{infix}.{}", fixed.unwrap_or(ext)),
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
        Finding::at_source(
            file.path,
            "",
            Pos { line: 1, byte: 0 },
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
