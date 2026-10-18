//! `agent/colocated-tests`: modules with public functions and no nearby tests.

use std::collections::HashSet;

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
    /// Fixed root-relative paths with a `{stem}` placeholder.
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

fn has_counterpart(paths: &HashSet<&str>, names: &HashSet<&str>, path: &str, lang: Lang) -> bool {
    match conventions(lang) {
        Candidates::Names(form) => names_match(names, path, &form),
        Candidates::Dir(tail) => dir_match(paths, path, tail),
        Candidates::Root(templates) => root_match(paths, templates, path),
        Candidates::RustTests => rust_match(paths, path),
        Candidates::Web => web_match(paths, path),
        Candidates::None => false,
    }
}

fn names_match(names: &HashSet<&str>, path: &str, form: &NameForm) -> bool {
    let (stem, own_ext) = stem_ext(path);
    let ext = form.ext.unwrap_or(own_ext);
    form.prefixes
        .iter()
        .any(|prefix| names.contains(format!("{prefix}{stem}.{ext}").as_str()))
        || form
            .infixes
            .iter()
            .any(|infix| names.contains(format!("{stem}{infix}.{ext}").as_str()))
}

fn dir_match(paths: &HashSet<&str>, path: &str, tail: &str) -> bool {
    let dir = dir_prefix(path);
    let stem = stem_ext(path).0;
    paths.contains(format!("{dir}{stem}{tail}").as_str())
}

fn root_match(paths: &HashSet<&str>, templates: &[&str], path: &str) -> bool {
    let stem = stem_ext(path).0;
    templates
        .iter()
        .any(|template| paths.contains(template.replace("{stem}", stem).as_str()))
}

fn rust_match(paths: &HashSet<&str>, path: &str) -> bool {
    let stem = stem_ext(path).0;
    let exact = format!("{stem}.rs");
    let prefix = format!("{stem}_");
    paths.iter().any(|candidate| {
        let name = file_name(candidate);
        under_tests(candidate) && (name == exact.as_str() || name.starts_with(prefix.as_str()))
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

fn web_match(paths: &HashSet<&str>, path: &str) -> bool {
    let dir = dir_prefix(path);
    let stem = stem_ext(path).0;
    WEB_TAILS
        .iter()
        .any(|tail| paths.contains(format!("{dir}{stem}{tail}").as_str()))
        || paths
            .iter()
            .any(|candidate| candidate.starts_with(&format!("{dir}__tests__/{stem}.test.")))
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
        let paths: HashSet<&str> = files.iter().map(|file| file.path).collect();
        let names: HashSet<&str> = files.iter().map(|file| file_name(file.path)).collect();
        for file in files {
            if self.skipped(file) || file.facts.has_inline_tests {
                continue;
            }
            if has_counterpart(&paths, &names, file.path, file.facts.lang) {
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
