use super::*;
use crate::facts::WorldFacts;
use crate::lang::Lang;
use globset::GlobSetBuilder;

pub(super) fn lib(lang: Lang, package: &str, public_functions: u32) -> WorldFacts {
    WorldFacts {
        lang,
        role: CodeRole::Library,
        regions: Vec::new(),
        package: package.to_string(),
        package_from_manifest: false,
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

pub(super) fn evidence(lang: Lang, package: &str, imports: &[&str]) -> WorldFacts {
    WorldFacts {
        lang,
        role: CodeRole::Test,
        regions: Vec::new(),
        package: package.to_string(),
        package_from_manifest: false,
        has_inline_tests: false,
        public_functions: 0,
        definitions: Vec::new(),
        abstractions: Vec::new(),
        impls: Vec::new(),
        barrel: None,
        imports: imports.iter().map(|spec| Box::<str>::from(*spec)).collect(),
    }
}

pub(super) fn test_file(lang: Lang, package: &str) -> WorldFacts {
    WorldFacts {
        lang,
        role: CodeRole::Test,
        regions: Vec::new(),
        package: package.to_string(),
        package_from_manifest: false,
        has_inline_tests: false,
        public_functions: 0,
        definitions: Vec::new(),
        abstractions: Vec::new(),
        impls: Vec::new(),
        barrel: None,
        imports: Vec::new(),
    }
}

pub(super) fn rule() -> ColocatedTests {
    ColocatedTests {
        exclude: GlobSetBuilder::new().build().unwrap(),
    }
}

pub(super) fn check(rule: &ColocatedTests, files: &[WorldFile<'_>]) -> Vec<String> {
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

pub(super) fn manifest(mut facts: WorldFacts) -> WorldFacts {
    facts.package_from_manifest = true;
    facts
}
