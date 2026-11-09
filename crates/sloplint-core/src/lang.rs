use serde::{Deserialize, Serialize};

struct LangIdentity {
    key: &'static str,
    extensions: &'static [&'static str],
}

macro_rules! languages {
    ($($variant:ident => $key:literal, $extensions:expr;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum Lang {
            $($variant,)*
        }

        impl Lang {
            const fn identity(self) -> LangIdentity {
                match self {
                    $(Lang::$variant => LangIdentity { key: $key, extensions: $extensions },)*
                }
            }
        }

        pub const ALL_LANGS: &[Lang] = &[$(Lang::$variant,)*];
    };
}

languages! {
    Rust => "rust", &[".rs"];
    TypeScript => "typescript", &[".ts", ".mts", ".cts"];
    Tsx => "typescript", &[".tsx"];
    JavaScript => "javascript", &[".js", ".jsx", ".mjs", ".cjs"];
    Python => "python", &[".py", ".pyi"];
    Go => "go", &[".go"];
    Java => "java", &[".java"];
    Kotlin => "kotlin", &[".kt", ".kts"];
    CSharp => "csharp", &[".cs"];
    C => "c", &[".c", HEADER_EXTENSION];
    Cpp => "cpp", &[".cc", ".cpp", ".cxx", ".c++", ".C", ".hh", ".hpp", ".hxx", ".h++", ".ipp"];
    ObjC => "objc", &[".m"];
    Php => "php", &[".php"];
    Ruby => "ruby", &[".rb", ".rake", ".gemspec", ".ru"];
    Bash => "bash", &[".sh", ".bash", ".bats"];
    PowerShell => "powershell", &[".ps1", ".psm1"];
    Swift => "swift", &[".swift"];
    R => "r", &[".r", ".R"];
    Scala => "scala", &[".scala", ".sc"];
    Dart => "dart", &[".dart"];
    Zig => "zig", &[".zig"];
    Ocaml => "ocaml", &[".ml"];
    OcamlInterface => "ocaml", &[".mli"];
    Odin => "odin", &[".odin"];
    Starlark => "starlark", &[".bzl", ".star", ".BUILD", ".bazel"];
}

const HEADER_EXTENSION: &str = ".h";

const HEADER_FALLBACKS: &[Lang] = &[Lang::Cpp, Lang::ObjC];

const fn same_str(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn key_seen_before(index: usize) -> bool {
    let key = ALL_LANGS[index].identity().key;
    let mut earlier = 0;
    while earlier < index {
        if same_str(ALL_LANGS[earlier].identity().key, key) {
            return true;
        }
        earlier += 1;
    }
    false
}

const fn distinct_key_count() -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < ALL_LANGS.len() {
        if !key_seen_before(index) {
            count += 1;
        }
        index += 1;
    }
    count
}

const fn distinct_keys() -> [&'static str; distinct_key_count()] {
    let mut keys = [""; distinct_key_count()];
    let mut slot = 0;
    let mut index = 0;
    while index < ALL_LANGS.len() {
        if !key_seen_before(index) {
            keys[slot] = ALL_LANGS[index].identity().key;
            slot += 1;
        }
        index += 1;
    }
    keys
}

pub const ALL_CONFIG_KEYS: &[&str] = &distinct_keys();

const fn extension_count() -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < ALL_LANGS.len() {
        count += ALL_LANGS[index].identity().extensions.len();
        index += 1;
    }
    count
}

const fn extension_table() -> [(&'static str, Lang); extension_count()] {
    let mut table = [("", Lang::Rust); extension_count()];
    let mut slot = 0;
    let mut index = 0;
    while index < ALL_LANGS.len() {
        let lang = ALL_LANGS[index];
        let extensions = lang.identity().extensions;
        let mut position = 0;
        while position < extensions.len() {
            table[slot] = (extensions[position], lang);
            slot += 1;
            position += 1;
        }
        index += 1;
    }
    table
}

const EXTENSIONS: &[(&str, Lang)] = &extension_table();

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

impl Lang {
    pub fn from_path(rel_path: &str) -> Option<Lang> {
        if rel_path.ends_with(".blade.php") {
            return None;
        }
        let base = rel_path.rsplit('/').next().unwrap_or(rel_path);
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

    /// Cpp precedes ObjC: the Objective-C grammar also accepts some C++ headers, so Cpp must try them first.
    pub fn parse_fallbacks(self, rel_path: &str) -> &'static [Lang] {
        let base = rel_path.rsplit('/').next().unwrap_or(rel_path);
        let header = self == Lang::C && base.ends_with(HEADER_EXTENSION);
        if header { HEADER_FALLBACKS } else { &[] }
    }

    pub fn config_key(self) -> &'static str {
        self.identity().key
    }

    pub fn from_config_key(key: &str) -> Option<Lang> {
        ALL_LANGS
            .iter()
            .copied()
            .find(|lang| lang.identity().key == key)
    }

    pub fn is_entry_file(self, rel_path: &str) -> bool {
        let spec = self.entry_spec();
        let base = rel_path.rsplit('/').next().unwrap_or(rel_path);
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
    fn every_extension_routes_back_to_its_language() {
        for (extension, lang) in EXTENSIONS {
            assert_eq!(
                Lang::from_path(&format!("pkg/module{extension}")),
                Some(*lang),
                "{extension} must route to the language that declares it"
            );
        }
    }

    #[test]
    fn shares_config_keys() {
        for key in ALL_CONFIG_KEYS {
            assert!(
                Lang::from_config_key(key).is_some(),
                "config key {key:?} should resolve to a language"
            );
            assert_eq!(
                Lang::from_config_key(key).unwrap().config_key(),
                *key,
                "config key {key:?} must round-trip through its language"
            );
        }
        let mut table = ALL_CONFIG_KEYS.to_vec();
        table.sort_unstable();
        let mut langs: Vec<&str> = ALL_LANGS.iter().map(|lang| lang.config_key()).collect();
        langs.sort_unstable();
        langs.dedup();
        assert_eq!(
            table, langs,
            "the key table must hold exactly the distinct config keys of every language"
        );
        assert_eq!(
            Lang::Tsx.config_key(),
            "typescript",
            "tsx shares the typescript key"
        );
        assert_eq!(
            Lang::OcamlInterface.config_key(),
            "ocaml",
            "ocaml interface shares the ocaml key"
        );
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
