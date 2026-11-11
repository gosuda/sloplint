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

    pub fn namespace(self) -> Lang {
        match self {
            Lang::JavaScript | Lang::TypeScript | Lang::Tsx => Lang::JavaScript,
            Lang::Java | Lang::Kotlin | Lang::Scala => Lang::Java,
            Lang::C | Lang::Cpp | Lang::ObjC => Lang::C,
            other => other,
        }
    }

    pub fn from_config_key(key: &str) -> Option<Lang> {
        ALL_LANGS
            .iter()
            .copied()
            .find(|lang| lang.identity().key == key)
    }
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
}
