//! Source-language identity, path detection, and per-language entry-file rules.

use serde::{Deserialize, Serialize};

/// One of the twenty-four grammars sloplint analyzes; `TypeScript`/`Tsx` share the `typescript` config key and `Ocaml`/`OcamlInterface` share `ocaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Lang {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Go,
    Java,
    Kotlin,
    CSharp,
    C,
    Cpp,
    ObjC,
    Php,
    Ruby,
    Bash,
    PowerShell,
    Swift,
    R,
    Scala,
    Dart,
    Zig,
    Ocaml,
    OcamlInterface,
    Odin,
    Starlark,
}

/// The extension that C, C++, and Objective-C headers share; the table routes it to C.
const HEADER_EXTENSION: &str = ".h";

/// The grammars a `.h` file that fails as C is parsed with, in order; see [`Lang::parse_fallbacks`].
const HEADER_FALLBACKS: &[Lang] = &[Lang::Cpp, Lang::ObjC];

/// Every extension rule in priority order; the first matching entry wins and matching is case-sensitive.
const EXTENSIONS: &[(&str, Lang)] = &[
    (".rs", Lang::Rust),
    (".ts", Lang::TypeScript),
    (".mts", Lang::TypeScript),
    (".cts", Lang::TypeScript),
    (".tsx", Lang::Tsx),
    (".js", Lang::JavaScript),
    (".jsx", Lang::JavaScript),
    (".mjs", Lang::JavaScript),
    (".cjs", Lang::JavaScript),
    (".py", Lang::Python),
    (".pyi", Lang::Python),
    (".go", Lang::Go),
    (".java", Lang::Java),
    (".kt", Lang::Kotlin),
    (".kts", Lang::Kotlin),
    (".cs", Lang::CSharp),
    (".c", Lang::C),
    (HEADER_EXTENSION, Lang::C),
    (".cc", Lang::Cpp),
    (".cpp", Lang::Cpp),
    (".cxx", Lang::Cpp),
    (".c++", Lang::Cpp),
    (".C", Lang::Cpp),
    (".hh", Lang::Cpp),
    (".hpp", Lang::Cpp),
    (".hxx", Lang::Cpp),
    (".h++", Lang::Cpp),
    (".ipp", Lang::Cpp),
    (".m", Lang::ObjC),
    (".php", Lang::Php),
    (".rb", Lang::Ruby),
    (".rake", Lang::Ruby),
    (".gemspec", Lang::Ruby),
    (".ru", Lang::Ruby),
    (".sh", Lang::Bash),
    (".bash", Lang::Bash),
    (".bats", Lang::Bash),
    (".ps1", Lang::PowerShell),
    (".psm1", Lang::PowerShell),
    (".swift", Lang::Swift),
    (".r", Lang::R),
    (".R", Lang::R),
    (".scala", Lang::Scala),
    (".sc", Lang::Scala),
    (".dart", Lang::Dart),
    (".zig", Lang::Zig),
    (".ml", Lang::Ocaml),
    (".mli", Lang::OcamlInterface),
    (".odin", Lang::Odin),
    (".bzl", Lang::Starlark),
    (".star", Lang::Starlark),
    (".BUILD", Lang::Starlark),
    (".bazel", Lang::Starlark),
];

/// Exact file-name rules applied when no extension rule matched.
const SPECIAL_NAMES: &[(&str, Lang)] = &[
    ("Rakefile", Lang::Ruby),
    ("Gemfile", Lang::Ruby),
    ("Guardfile", Lang::Ruby),
    ("Capfile", Lang::Ruby),
    ("Podfile", Lang::Ruby),
    ("Vagrantfile", Lang::Ruby),
    ("Berksfile", Lang::Ruby),
    ("Thorfile", Lang::Ruby),
    ("Fastfile", Lang::Ruby),
    ("Appfile", Lang::Ruby),
    ("config.ru", Lang::Ruby),
    (".bashrc", Lang::Bash),
    (".bash_profile", Lang::Bash),
    (".profile", Lang::Bash),
    (".bash_logout", Lang::Bash),
    (".bash_aliases", Lang::Bash),
    ("profile.ps1", Lang::PowerShell),
    ("BUILD", Lang::Starlark),
    ("BUILD.bazel", Lang::Starlark),
    ("WORKSPACE", Lang::Starlark),
    ("WORKSPACE.bazel", Lang::Starlark),
    ("MODULE.bazel", Lang::Starlark),
    ("Tiltfile", Lang::Starlark),
    ("BUCK", Lang::Starlark),
];

/// Every variant in declaration order; the one list consumers iterate instead of keeping a copy.
pub const ALL_LANGS: &[Lang] = &[
    Lang::Rust,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::JavaScript,
    Lang::Python,
    Lang::Go,
    Lang::Java,
    Lang::Kotlin,
    Lang::CSharp,
    Lang::C,
    Lang::Cpp,
    Lang::ObjC,
    Lang::Php,
    Lang::Ruby,
    Lang::Bash,
    Lang::PowerShell,
    Lang::Swift,
    Lang::R,
    Lang::Scala,
    Lang::Dart,
    Lang::Zig,
    Lang::Ocaml,
    Lang::OcamlInterface,
    Lang::Odin,
    Lang::Starlark,
];

/// The 23 config keys in `[project].languages` order; `typescript` and `ocaml` each cover two variants.
pub const ALL_CONFIG_KEYS: &[&str] = &[
    "rust",
    "typescript",
    "javascript",
    "python",
    "go",
    "java",
    "kotlin",
    "csharp",
    "c",
    "cpp",
    "objc",
    "php",
    "ruby",
    "bash",
    "powershell",
    "swift",
    "r",
    "scala",
    "dart",
    "zig",
    "ocaml",
    "odin",
    "starlark",
];

/// Config keys paired with the primary variant that answers for the key.
const PRIMARY_KEY_LANGS: &[(&str, Lang)] = &[
    ("rust", Lang::Rust),
    ("typescript", Lang::TypeScript),
    ("javascript", Lang::JavaScript),
    ("python", Lang::Python),
    ("go", Lang::Go),
    ("java", Lang::Java),
    ("kotlin", Lang::Kotlin),
    ("csharp", Lang::CSharp),
    ("c", Lang::C),
    ("cpp", Lang::Cpp),
    ("objc", Lang::ObjC),
    ("php", Lang::Php),
    ("ruby", Lang::Ruby),
    ("bash", Lang::Bash),
    ("powershell", Lang::PowerShell),
    ("swift", Lang::Swift),
    ("r", Lang::R),
    ("scala", Lang::Scala),
    ("dart", Lang::Dart),
    ("zig", Lang::Zig),
    ("ocaml", Lang::Ocaml),
    ("odin", Lang::Odin),
    ("starlark", Lang::Starlark),
];

#[derive(Clone, Copy)]
struct EntrySpec {
    files: &'static [&'static str],
    segments: &'static [&'static str],
    parts: &'static [&'static [&'static str]],
    any_config: bool,
}

const fn entry(
    files: &'static [&'static str],
    segments: &'static [&'static str],
    parts: &'static [&'static [&'static str]],
    any_config: bool,
) -> EntrySpec {
    EntrySpec {
        files,
        segments,
        parts,
        any_config,
    }
}

const RUBY_ENTRY_FILES: &[&str] = &[
    "Rakefile",
    "Gemfile",
    "Guardfile",
    "Capfile",
    "Podfile",
    "Vagrantfile",
    "Berksfile",
    "Thorfile",
    "Fastfile",
    "Appfile",
    "config.ru",
    "main.rb",
];

const STARLARK_ENTRY_FILES: &[&str] = &[
    "BUILD",
    "BUILD.bazel",
    "WORKSPACE",
    "WORKSPACE.bazel",
    "MODULE.bazel",
    "Tiltfile",
];

fn file_name_of(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path)
}

impl Lang {
    /// Detects the language of a `/`-separated relative path by extension first, then by exact file name; `.blade.php` files are never linted.
    pub fn from_path(rel_path: &str) -> Option<Lang> {
        if rel_path.ends_with(".blade.php") {
            return None;
        }
        let base = file_name_of(rel_path);
        EXTENSIONS
            .iter()
            .find(|(ext, _)| base.ends_with(ext))
            .map(|(_, lang)| *lang)
            .or_else(|| {
                SPECIAL_NAMES
                    .iter()
                    .find(|(name, _)| *name == base)
                    .map(|(_, lang)| *lang)
            })
    }

    /// The grammars to parse a file with, in order, after its parse as `self` has a syntax error; empty when no second parse applies.
    ///
    /// A `.h` file routes to C, yet C++ and Objective-C headers share the extension, so a `.h` file that fails as C is tried as C++ and then as Objective-C. The first grammar that parses it without error wins, so the order decides a header that both accept. C++ comes first because the Objective-C grammar also accepts some C++ headers, such as a `namespace` holding a template-typed member, so trying it first would claim them, while no sampled header that uses `@interface`, `@protocol`, `@class`, or blocks parses as C++. C++ first also keeps the earlier C++ fallback unchanged. No other path falls back, and the routed language stays the file's language for selection, budgets, and statistics.
    pub fn parse_fallbacks(self, rel_path: &str) -> &'static [Lang] {
        let header = self == Lang::C && file_name_of(rel_path).ends_with(HEADER_EXTENSION);
        if header { HEADER_FALLBACKS } else { &[] }
    }

    /// The `[languages.<key>]` config key for this variant.
    pub fn config_key(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::TypeScript | Lang::Tsx => "typescript",
            Lang::JavaScript => "javascript",
            Lang::Python => "python",
            Lang::Go => "go",
            Lang::Java => "java",
            Lang::Kotlin => "kotlin",
            Lang::CSharp => "csharp",
            Lang::C => "c",
            Lang::Cpp => "cpp",
            Lang::ObjC => "objc",
            Lang::Php => "php",
            Lang::Ruby => "ruby",
            Lang::Bash => "bash",
            Lang::PowerShell => "powershell",
            Lang::Swift => "swift",
            Lang::R => "r",
            Lang::Scala => "scala",
            Lang::Dart => "dart",
            Lang::Zig => "zig",
            Lang::Ocaml | Lang::OcamlInterface => "ocaml",
            Lang::Odin => "odin",
            Lang::Starlark => "starlark",
        }
    }

    /// Resolves a config key to its primary variant; the shared keys answer `TypeScript` and `Ocaml`.
    pub fn from_config_key(key: &str) -> Option<Lang> {
        PRIMARY_KEY_LANGS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, lang)| *lang)
    }

    /// Reports whether the path is an entry file for this language, per the per-language entry rules.
    pub fn is_entry_file(self, rel_path: &str) -> bool {
        let spec = self.entry_spec();
        let base = file_name_of(rel_path);
        let segments: Vec<&str> = rel_path.split('/').collect();
        spec.files.contains(&base)
            || (spec.any_config && base.contains(".config."))
            || spec.segments.iter().any(|seg| segments.contains(seg))
            || spec
                .parts
                .iter()
                .any(|p| has_consecutive_parts(&segments, p))
    }

    fn entry_spec(self) -> EntrySpec {
        match self {
            Lang::Rust => entry(
                &["main.rs", "lib.rs", "mod.rs", "build.rs"],
                &[],
                &[&["src", "bin"]],
                false,
            ),
            Lang::TypeScript => entry(&["index.ts"], &[], &[], true),
            Lang::Tsx => entry(&["index.tsx"], &[], &[], true),
            Lang::JavaScript => entry(
                &["index.js", "index.jsx", "index.mjs", "index.cjs"],
                &["bin"],
                &[],
                true,
            ),
            Lang::Python => entry(
                &[
                    "__init__.py",
                    "__main__.py",
                    "conftest.py",
                    "manage.py",
                    "setup.py",
                ],
                &[],
                &[],
                false,
            ),
            Lang::Go => entry(&["main.go"], &["cmd"], &[], false),
            Lang::Java => entry(&["Main.java"], &[], &[&["src", "main", "java"]], false),
            Lang::Kotlin => entry(&["Main.kt", "main.kt"], &[], &[], false),
            Lang::CSharp => entry(&["Program.cs", "Startup.cs"], &[], &[], false),
            Lang::C => entry(&["main.c"], &[], &[], false),
            Lang::Cpp => entry(&["main.cc", "main.cpp", "main.cxx"], &[], &[], false),
            Lang::ObjC => entry(&["main.m"], &[], &[], false),
            Lang::Php => entry(&["index.php", "artisan"], &["public", "config"], &[], false),
            Lang::Ruby => entry(RUBY_ENTRY_FILES, &["bin", "exe"], &[], false),
            Lang::Bash => entry(
                &["install.sh", "setup.sh", "bootstrap.sh", "entrypoint.sh"],
                &["bin", "scripts"],
                &[],
                false,
            ),
            Lang::PowerShell => entry(&["profile.ps1"], &["bin", "scripts"], &[], false),
            Lang::Swift => entry(&["main.swift", "Package.swift"], &[], &[], false),
            Lang::R => entry(&["main.R", "app.R", "run.R"], &[], &[], false),
            Lang::Scala => entry(
                &["Main.scala", "Main.sc"],
                &[],
                &[&["src", "main", "scala"]],
                false,
            ),
            Lang::Dart => entry(&["main.dart"], &["bin"], &[], false),
            Lang::Zig => entry(&["main.zig", "build.zig"], &[], &[], false),
            Lang::Ocaml => entry(&["main.ml"], &["bin"], &[], false),
            Lang::OcamlInterface => entry(&[], &[], &[], false),
            Lang::Odin => entry(&["main.odin"], &[], &[], false),
            Lang::Starlark => entry(STARLARK_ENTRY_FILES, &[], &[], false),
        }
    }
}

fn has_consecutive_parts(segments: &[&str], parts: &[&str]) -> bool {
    segments.windows(parts.len()).any(|w| w == parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_extension_case_sensitively() {
        assert_eq!(Lang::from_path("src/lib.rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_path("a.d.ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_path("c/main.C"), Some(Lang::Cpp));
        assert_eq!(Lang::from_path("c/main.c"), Some(Lang::C));
        assert_eq!(Lang::from_path("c/main.h"), Some(Lang::C));
        assert_eq!(Lang::from_path("x.mli"), Some(Lang::OcamlInterface));
        assert_eq!(Lang::from_path("a.R"), Some(Lang::R));
        assert_eq!(Lang::from_path("a.rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_path("a.RS"), None);
        assert_eq!(Lang::from_path("a.vue"), None);
    }

    #[test]
    fn rejects_templates_and_matches_special_names() {
        assert_eq!(Lang::from_path("resources/views/x.blade.php"), None);
        assert_eq!(Lang::from_path("Rakefile"), Some(Lang::Ruby));
        assert_eq!(Lang::from_path("app/config.ru"), Some(Lang::Ruby));
        assert_eq!(Lang::from_path("home/.bashrc"), Some(Lang::Bash));
        assert_eq!(Lang::from_path("BUILD"), Some(Lang::Starlark));
        assert_eq!(Lang::from_path("tools/Tiltfile"), Some(Lang::Starlark));
        assert_eq!(Lang::from_path("etc/profile.ps1"), Some(Lang::PowerShell));
    }

    #[test]
    fn only_dot_h_files_routed_to_c_fall_back_to_cpp_then_objc() {
        let order = [Lang::Cpp, Lang::ObjC];
        assert_eq!(Lang::C.parse_fallbacks("a.h"), order);
        assert_eq!(Lang::C.parse_fallbacks("include/sub/a.h"), order);
        assert_eq!(Lang::C.parse_fallbacks("a.c"), []);
        assert_eq!(Lang::C.parse_fallbacks("a.hh"), []);
        assert_eq!(Lang::C.parse_fallbacks("a.H"), []);
        assert_eq!(Lang::C.parse_fallbacks("dir.h/a.c"), []);
        assert_eq!(Lang::Cpp.parse_fallbacks("a.h"), []);
        assert_eq!(Lang::ObjC.parse_fallbacks("a.h"), []);
        for lang in ALL_LANGS.iter().copied().filter(|lang| *lang != Lang::C) {
            assert_eq!(lang.parse_fallbacks("a.h"), [], "{lang:?} never falls back");
        }
    }

    #[test]
    fn the_fallback_applies_exactly_to_paths_the_table_routes_to_c_through_dot_h() {
        for path in ["a.h", "x/y/a.h", "a.c", "a.hpp", "a.h++", "a.H", "a.ipp"] {
            let falls_back =
                Lang::from_path(path).is_some_and(|lang| !lang.parse_fallbacks(path).is_empty());
            assert_eq!(falls_back, path.ends_with(".h"), "{path}");
        }
    }

    #[test]
    fn shares_config_keys() {
        assert_eq!(ALL_CONFIG_KEYS.len(), 23);
        for key in ALL_CONFIG_KEYS {
            assert!(
                Lang::from_config_key(key).is_some(),
                "config key {key:?} should resolve to a language"
            );
            assert_eq!(Lang::from_config_key(key).unwrap().config_key(), *key);
        }
        assert_eq!(Lang::Tsx.config_key(), "typescript");
        assert_eq!(Lang::OcamlInterface.config_key(), "ocaml");
        assert_eq!(ALL_LANGS.len(), 25);
    }

    #[test]
    fn flags_entry_files() {
        assert!(
            Lang::Rust.is_entry_file("src/bin/serve.rs"),
            "src/bin/serve.rs should be a Rust entry file"
        );
        assert!(
            Lang::Rust.is_entry_file("build.rs"),
            "build.rs should be a Rust entry file"
        );
        assert!(
            !Lang::Rust.is_entry_file("src/util.rs"),
            "src/util.rs should not be a Rust entry file"
        );
        assert!(
            Lang::JavaScript.is_entry_file("vite.config.js"),
            "vite.config.js should be a JavaScript entry file"
        );
        assert!(
            !Lang::TypeScript.is_entry_file("src/config.ts"),
            "src/config.ts should not be a TypeScript entry file"
        );
        assert!(
            Lang::TypeScript.is_entry_file("index.ts"),
            "index.ts should be a TypeScript entry file"
        );
        assert!(
            Lang::JavaScript.is_entry_file("scripts/bin/run.js"),
            "scripts/bin/run.js should be a JavaScript entry file"
        );
        assert!(
            Lang::Go.is_entry_file("cmd/api/main.go"),
            "cmd/api/main.go should be a Go entry file"
        );
        assert!(
            Lang::Python.is_entry_file("pkg/__init__.py"),
            "pkg/__init__.py should be a Python entry file"
        );
        assert!(
            Lang::Ocaml.is_entry_file("bin/tool.ml"),
            "bin/tool.ml should be an OCaml entry file"
        );
        assert!(
            !Lang::OcamlInterface.is_entry_file("main.mli"),
            "main.mli should not be an OCaml entry file"
        );
        assert!(
            Lang::Swift.is_entry_file("Package.swift"),
            "Package.swift should be a Swift entry file"
        );
        assert!(
            Lang::Php.is_entry_file("routes/config/loader.php"),
            "routes/config/loader.php should be a PHP entry file"
        );
        assert!(
            Lang::Ruby.is_entry_file("exe/tool.rb"),
            "exe/tool.rb should be a Ruby entry file"
        );
        assert!(
            Lang::Bash.is_entry_file("scripts/setup.sh"),
            "scripts/setup.sh should be a Bash entry file"
        );
        assert!(
            Lang::PowerShell.is_entry_file("bin/profile.ps1"),
            "bin/profile.ps1 should be a PowerShell entry file"
        );
        assert!(
            Lang::Starlark.is_entry_file("BUILD"),
            "BUILD should be a Starlark entry file"
        );
        assert!(
            Lang::Dart.is_entry_file("bin/app.dart"),
            "bin/app.dart should be a Dart entry file"
        );
        assert!(
            !Lang::Dart.is_entry_file("lib/app.dart"),
            "lib/app.dart should not be a Dart entry file"
        );
    }
}
