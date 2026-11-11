use super::*;

fn shared(lang: Lang, path: &str, spec: &str) -> bool {
    let lib = library_keys(lang, path);
    let imp = import_keys(lang, path, spec);
    lib.iter().any(|key| imp.contains(key))
}

#[test]
fn huge_spec_yields_at_most_k_keys() {
    let spec = vec!["a"; 20_000].join(".");
    let keys = import_keys(Lang::Python, "tests/test_big.py", &spec);
    assert_eq!(keys.len(), MAX_KEY_SEGMENTS - 1);
}

#[test]
fn deep_library_yields_at_most_k_keys() {
    let path = format!("{}/deep.py", vec!["d"; 2_000].join("/"));
    assert!(library_keys(Lang::Python, &path).len() <= MAX_KEY_SEGMENTS);
}

#[test]
fn hyphen_and_underscore_segments_are_equal() {
    assert!(shared(
        Lang::Rust,
        "crates/log-core/src/tag.rs",
        "log_core::tag"
    ));
}

#[test]
fn nested_rust_module_matches_its_import() {
    assert!(shared(
        Lang::Rust,
        "crates/parser/src/rules/lexer.rs",
        "parser::rules::lexer"
    ));
}

#[test]
fn python_double_dot_resolves_against_importer_dir() {
    assert_eq!(
        import_keys(Lang::Python, "tests/unit/test_x.py", "..core"),
        ["core"]
    );
}

#[test]
fn bare_import_covers_only_top_level_modules() {
    assert_eq!(import_keys(Lang::Python, "a/b/c.py", "utils"), ["utils"]);
    assert_eq!(
        library_keys(Lang::Python, "billing/utils.py"),
        ["billing/utils"]
    );
    assert!(!shared(Lang::Python, "billing/utils.py", "utils"));
}
#[test]
fn python_src_layout_module_drops_the_src_root() {
    assert_eq!(library_keys(Lang::Python, "src/tag.py"), ["tag"]);
    assert!(shared(Lang::Python, "src/tag.py", "tag"));
    assert!(shared(Lang::Python, "src/app/user.py", "app.user"));
    assert!(!shared(Lang::Python, "vendor/src/tag.py", "tag"));
}
#[test]
fn dotted_ts_basename_stays_one_segment() {
    assert!(
        library_keys(Lang::TypeScript, "src/app/user.service.ts")
            .contains(&"src/app/user.service".to_string())
    );
    assert!(shared(
        Lang::TypeScript,
        "src/app/user.service.ts",
        "../src/app/user.service"
    ));
}
#[test]
fn package_marker_files_key_their_directory() {
    assert_eq!(library_keys(Lang::Python, "pkg/__init__.py"), ["pkg"]);
    assert_eq!(
        library_keys(Lang::Rust, "crates/x/src/foo/mod.rs"),
        ["x/foo"]
    );
    assert_eq!(
        library_keys(Lang::TypeScript, "src/ui/index.ts"),
        ["src/ui"]
    );
}
#[test]
fn go_import_suffix_covers_nested_package() {
    let lib = library_keys(Lang::Go, "internal/tag/add.go");
    let imp = import_keys(
        Lang::Go,
        "cmd/app/app_test.go",
        "example.com/m/internal/tag",
    );
    assert_eq!(lib, ["internal/tag"]);
    assert!(imp.contains(&"internal/tag".to_string()));
}
#[test]
fn test_keys_strip_each_affix_form() {
    let cases = [
        (Lang::CSharp, "src/WidgetTests.cs", "src/Widget"),
        (Lang::Java, "src/ServiceTest.java", "src/Service"),
        (Lang::Scala, "src/serviceSpec.scala", "src/service"),
        (Lang::Rust, "widget_tests.rs", "widget"),
        (Lang::Rust, "widget_test.rs", "widget"),
        (Lang::Bash, "widget-test.sh", "widget"),
        (Lang::JavaScript, "src/service.test.js", "src/service"),
        (Lang::TypeScript, "src/service.spec.ts", "src/service"),
        (Lang::Ocaml, "src/service.test.ml", "src/service"),
        (Lang::PowerShell, "Deploy.Tests.ps1", "Deploy"),
        (Lang::Ruby, "widget_spec.rb", "widget"),
        (Lang::Bash, "widget-spec.sh", "widget"),
        (Lang::Python, "pkg/test_widget.py", "pkg/widget"),
        (Lang::R, "test-service.R", "service"),
    ];
    for (lang, path, want) in cases {
        assert_eq!(test_keys(lang, path), [want], "affix in {path}");
    }
}
#[test]
fn test_keys_never_strip_to_an_empty_stem() {
    assert_eq!(test_keys(Lang::Java, "Test.java"), ["Test"]);
    assert_eq!(test_keys(Lang::Python, "pkg/tests.py"), ["pkg/tests"]);
}
#[test]
fn library_keys_keep_test_like_stems() {
    assert_eq!(library_keys(Lang::Scala, "src/Spec.scala"), ["src/Spec"]);
    assert_eq!(
        library_keys(Lang::Java, "src/ServiceTest.java"),
        ["src/ServiceTest"]
    );
}

#[test]
fn rust_root_crate_import_covers_module() {
    assert!(
        shared(Lang::Rust, "crates/app/src/store.rs", "app::store"),
        "crates layout via own name covers"
    );
    assert!(
        shared(Lang::Rust, "src/foo.rs", "crate::foo"),
        "root via crate covers"
    );
    assert!(
        !shared(Lang::Rust, "src/foo.rs", "mycrate::foo"),
        "root via package name cannot be proved"
    );
    assert!(
        !shared(Lang::Rust, "src/fs.rs", "std::fs"),
        "std must not cover src/fs.rs"
    );
    assert!(
        !shared(Lang::Rust, "src/de.rs", "serde::de"),
        "serde must not cover src/de.rs"
    );
}
#[test]
fn ts_alias_prefix_covers_src_root() {
    assert_eq!(
        import_keys(Lang::TypeScript, "test/foo.test.ts", "@/foo"),
        ["src/foo"]
    );
    assert!(
        shared(Lang::TypeScript, "src/foo.ts", "@/foo"),
        "before fix @/foo gave @/foo"
    );
    assert!(
        !shared(Lang::TypeScript, "src/foo.ts", "@scope/foo"),
        "@scope must not cover"
    );
}
#[test]
fn dotted_import_splits_for_jvm_langs() {
    assert!(
        shared(
            Lang::Java,
            "src/main/java/com/acme/Foo.java",
            "com.acme/Foo"
        ),
        "dots kept, Foo reported"
    );
    assert!(
        shared(
            Lang::Kotlin,
            "src/main/kotlin/com/acme/Foo.kt",
            "com.acme/Foo"
        ),
        "Kotlin kept dots"
    );
    assert!(
        !shared(
            Lang::Java,
            "src/main/java/com/acme/Unit.java",
            "com.acme/Foo"
        ),
        "Unit stays reported"
    );
}
#[test]
fn csharp_test_dir_suffix_stripped() {
    let lib = library_keys(Lang::CSharp, "src/Company.Lib/Widget.cs");
    let test = test_keys(Lang::CSharp, "src/Company.Lib.Tests/WidgetTests.cs");
    assert!(
        lib.iter().any(|key| test.contains(key)),
        "Tests dir hid Widget"
    );
    assert!(
        !library_keys(Lang::CSharp, "src/Company.Lib/Other.cs")
            .iter()
            .any(|key| test.contains(key)),
        "Other stays reported"
    );
}
#[test]
fn java_affix_it_suffix_only() {
    let lib = library_keys(Lang::Java, "src/main/java/com/acme/Foo.java");
    assert!(
        lib.iter()
            .any(|k| test_keys(Lang::Java, "src/main/java/com/acme/FooIT.java").contains(k)),
        "FooIT hid Foo"
    );
    assert!(
        !lib.iter()
            .any(|k| test_keys(Lang::Java, "src/main/java/com/acme/TestFoo.java").contains(k)),
        "TestFoo stays uncovered"
    );
    assert!(
        !library_keys(Lang::Java, "src/main/java/com/acme/Config.java")
            .iter()
            .any(|k| test_keys(Lang::Java, "src/main/java/com/acme/TestConfig.java").contains(k)),
        "TestConfig stays uncovered"
    );
    assert_eq!(test_keys(Lang::Java, "src/FOOIT.java"), ["src/FOOIT"]);
}
#[test]
fn csharp_dotted_dir_matches_dotted_import() {
    assert!(shared(
        Lang::CSharp,
        "src/Company.Lib/Widget.cs",
        "Company.Lib/Widget"
    ));
    assert!(!shared(
        Lang::CSharp,
        "src/Company.Lib/Other.cs",
        "Company.Lib/Widget"
    ));
}
#[test]
fn twin_python_modules_share_only_short_suffix() {
    assert!(shared(Lang::Python, "services/b/app/utils.py", "app.utils"));
    assert!(!shared(
        Lang::Python,
        "services/b/app/utils.py",
        "other.utils"
    ));
}
