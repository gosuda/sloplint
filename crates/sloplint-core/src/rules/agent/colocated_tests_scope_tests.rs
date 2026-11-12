use crate::facts::{NamedSite, Reexport, WorldFacts};
use crate::lang::Lang;
use crate::rules::WorldFile;

use super::tests::{check, evidence, lib, manifest, rule, test_file};

#[test]
fn manifest_packages_scope_import_coverage_to_their_own_modules() {
    let rule = rule();
    let in_a = manifest(lib(Lang::Python, "pkga", 1));
    let in_b = manifest(lib(Lang::Python, "pkgb", 1));
    let probe_a = manifest(evidence(Lang::Python, "pkga", &["src.common"]));
    let probe_b = manifest(evidence(Lang::Python, "pkgb", &["pkgb/other"]));
    let files = [
        WorldFile {
            path: "pkga/src/common.py",
            facts: &in_a,
        },
        WorldFile {
            path: "pkgb/src/common.py",
            facts: &in_b,
        },
        WorldFile {
            path: "pkga/tests/test_common.py",
            facts: &probe_a,
        },
        WorldFile {
            path: "pkgb/tests/test_other.py",
            facts: &probe_b,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        ["pkgb/src/common.py".to_string()],
        "package A's test import covers A's module while B's twin stays reported"
    );
}

#[test]
fn unmanifested_packages_keep_the_cross_package_suffix_match() {
    let rule = rule();
    let module = lib(Lang::Python, "src", 1);
    let helper = test_file(Lang::Python, "src");
    let probe = evidence(Lang::Python, "tests", &["x"]);
    let files = [
        WorldFile {
            path: "src/x.py",
            facts: &module,
        },
        WorldFile {
            path: "src/test_helper.py",
            facts: &helper,
        },
        WorldFile {
            path: "tests/test_x.py",
            facts: &probe,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "without manifests a test import still covers another directory's module"
    );
}

#[test]
fn manifest_module_stays_covered_by_fallback_test_import() {
    let rule = rule();
    let module = manifest(lib(Lang::Python, "pkga", 1));
    let gate = manifest(test_file(Lang::Python, "pkga"));
    let probe = evidence(Lang::Python, "tests", &["src.common"]);
    let files = [
        WorldFile {
            path: "pkga/src/common.py",
            facts: &module,
        },
        WorldFile {
            path: "pkga/tests/test_gate.py",
            facts: &gate,
        },
        WorldFile {
            path: "tests/test_common.py",
            facts: &probe,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "a fallback test's import still covers a manifest package's module"
    );
}

#[test]
fn fallback_module_stays_covered_by_manifest_test_import() {
    let rule = rule();
    let module = lib(Lang::Python, "pkg", 1);
    let helper = test_file(Lang::Python, "pkg");
    let probe = manifest(evidence(Lang::Python, "pkgb", &["pkg.common"]));
    let files = [
        WorldFile {
            path: "pkg/common.py",
            facts: &module,
        },
        WorldFile {
            path: "pkg/test_helper.py",
            facts: &helper,
        },
        WorldFile {
            path: "pkgb/tests/test_common.py",
            facts: &probe,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "a manifest package's import still covers a fallback module"
    );
}

#[test]
fn typescript_module_is_covered_by_javascript_test_import() {
    let rule = rule();
    let foo = manifest(lib(Lang::TypeScript, "app", 1));
    let foo_test = manifest(evidence(Lang::JavaScript, "app", &["./foo"]));
    let other_test = manifest(test_file(Lang::TypeScript, "app"));
    let files = [
        WorldFile {
            path: "src/foo.ts",
            facts: &foo,
        },
        WorldFile {
            path: "src/foo.test.js",
            facts: &foo_test,
        },
        WorldFile {
            path: "src/other.test.ts",
            facts: &other_test,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "a JavaScript test importing ./foo covers foo.ts in the same package"
    );
}

#[test]
fn javascript_module_is_covered_by_typescript_test_import() {
    let rule = rule();
    let bar = manifest(lib(Lang::JavaScript, "app", 1));
    let bar_test = manifest(evidence(Lang::TypeScript, "app", &["./bar"]));
    let other_test = manifest(test_file(Lang::JavaScript, "app"));
    let files = [
        WorldFile {
            path: "src/bar.js",
            facts: &bar,
        },
        WorldFile {
            path: "src/bar.test.ts",
            facts: &bar_test,
        },
        WorldFile {
            path: "src/other.test.js",
            facts: &other_test,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "a TypeScript test importing ./bar covers bar.js in the same package"
    );
}

#[test]
fn tsx_module_is_covered_by_typescript_test_import() {
    let rule = rule();
    let view = manifest(lib(Lang::Tsx, "app", 1));
    let view_test = manifest(evidence(Lang::TypeScript, "app", &["./view"]));
    let other_test = manifest(test_file(Lang::Tsx, "app"));
    let files = [
        WorldFile {
            path: "src/view.tsx",
            facts: &view,
        },
        WorldFile {
            path: "src/view.test.ts",
            facts: &view_test,
        },
        WorldFile {
            path: "src/other.test.tsx",
            facts: &other_test,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "a TypeScript test importing ./view covers view.tsx in the same package"
    );
}

#[test]
fn python_module_is_not_covered_by_matching_javascript_test() {
    let rule = rule();
    let util = lib(Lang::Python, "pkg", 1);
    let util_test = evidence(Lang::JavaScript, "pkg", &["./util"]);
    let gate = test_file(Lang::Python, "pkg");
    let files = [
        WorldFile {
            path: "src/util.py",
            facts: &util,
        },
        WorldFile {
            path: "src/util.test.js",
            facts: &util_test,
        },
        WorldFile {
            path: "src/other_test.py",
            facts: &gate,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        ["src/util.py".to_string()],
        "a same-stem JavaScript test does not cover a Python module"
    );
}

#[test]
fn java_module_is_covered_by_kotlin_test_import() {
    let rule = rule();
    let parser = lib(Lang::Java, "svc", 1);
    let parser_test = evidence(Lang::Kotlin, "svc", &["app.Parser"]);
    let lexer_test = test_file(Lang::Java, "svc");
    let files = [
        WorldFile {
            path: "svc/src/main/java/app/Parser.java",
            facts: &parser,
        },
        WorldFile {
            path: "svc/src/test/kotlin/app/ParserTest.kt",
            facts: &parser_test,
        },
        WorldFile {
            path: "svc/src/test/java/app/LexerTest.java",
            facts: &lexer_test,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "a Kotlin test importing app.Parser covers Parser.java in the same package"
    );
}

#[test]
fn explicit_index_import_covers_the_index_module() {
    let rule = rule();
    let widget = manifest(lib(Lang::TypeScript, "app", 1));
    let widget_test = manifest(evidence(Lang::TypeScript, "app", &["../src/widget/index"]));
    let files = [
        WorldFile {
            path: "src/widget/index.ts",
            facts: &widget,
        },
        WorldFile {
            path: "tests/widget.test.ts",
            facts: &widget_test,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        Vec::<String>::new(),
        "an explicit index import covers the directory module"
    );
}

#[test]
fn index_import_does_not_cover_sibling_module() {
    let rule = rule();
    let other = manifest(lib(Lang::TypeScript, "app", 1));
    let widget_test = manifest(evidence(Lang::TypeScript, "app", &["../src/widget/index"]));
    let files = [
        WorldFile {
            path: "src/widget/other.ts",
            facts: &other,
        },
        WorldFile {
            path: "tests/widget.test.ts",
            facts: &widget_test,
        },
    ];
    assert_eq!(
        check(&rule, &files),
        ["src/widget/other.ts".to_string()],
        "an index import covers only the directory module, not its siblings"
    );
}

const DEMO_MODULES: [&str; 4] = [
    "crates/demo/src/lib.rs",
    "crates/demo/src/emit.rs",
    "crates/demo/src/hooks.rs",
    "crates/demo/src/paths.rs",
];

fn demo_root(definitions: &[&str], reexports: &[(&str, &str)]) -> WorldFacts {
    let mut root = lib(Lang::Rust, "crates/demo", 1);
    root.definitions = definitions
        .iter()
        .map(|name| NamedSite {
            name: Box::from(*name),
            line: 1,
            column: 1,
        })
        .collect();
    root.reexports = reexports
        .iter()
        .map(|(name, module)| Reexport {
            name: Box::from(*name),
            module: Box::from(*module),
        })
        .collect();
    root
}

fn demo_flagged(root: &WorldFacts, probes: &[(&str, &WorldFacts)]) -> Vec<String> {
    let module = lib(Lang::Rust, "crates/demo", 1);
    let mut files = vec![WorldFile {
        path: DEMO_MODULES[0],
        facts: root,
    }];
    for &path in &DEMO_MODULES[1..] {
        files.push(WorldFile {
            path,
            facts: &module,
        });
    }
    for &(path, facts) in probes {
        files.push(WorldFile { path, facts });
    }
    check(&rule(), &files)
}

fn demo_paths(paths: &[&str]) -> Vec<String> {
    paths
        .iter()
        .map(|name| format!("crates/demo/src/{name}.rs"))
        .collect()
}

#[test]
fn rust_crate_root_item_import_covers_the_root_file() {
    let root = demo_root(&["analyze"], &[]);
    let test = evidence(Lang::Rust, "crates/demo", &["demo", "demo/analyze"]);
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/tests/rendering.rs", &test)]),
        demo_paths(&["emit", "hooks", "paths"]),
        "an item defined in the crate root covers the root file"
    );
}

#[test]
fn rust_crate_relative_root_item_import_covers_the_root_file() {
    let root = demo_root(&["analyze"], &[]);
    let mut inline = lib(Lang::Rust, "crates/demo", 0);
    inline.has_inline_tests = true;
    inline.imports = vec![Box::<str>::from("crate/analyze")];
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/src/probe.rs", &inline)]),
        demo_paths(&["emit", "hooks", "paths"]),
        "`crate::Name` resolves like `<crate>::Name`"
    );
}

#[test]
fn rust_named_reexport_covers_its_source_module_only() {
    let root = demo_root(&[], &[("render", "emit")]);
    let test = evidence(Lang::Rust, "crates/demo", &["demo", "demo/render"]);
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/tests/rendering.rs", &test)]),
        demo_paths(&["lib", "hooks", "paths"]),
        "a re-exported name covers the module it comes from, not the root or other modules"
    );
}

#[test]
fn rust_aliased_reexport_covers_its_source_module() {
    let root = demo_root(&[], &[("show", "self::emit")]);
    let test = evidence(Lang::Rust, "crates/demo", &["demo", "demo/show"]);
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/tests/rendering.rs", &test)]),
        demo_paths(&["lib", "hooks", "paths"]),
        "the exported alias resolves to the module of the original item"
    );
}

#[test]
fn rust_glob_reexport_covers_each_globbed_module() {
    let root = demo_root(&[], &[("*", "emit"), ("*", "crate::hooks")]);
    let test = evidence(Lang::Rust, "crates/demo", &["demo", "demo/render"]);
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/tests/rendering.rs", &test)]),
        demo_paths(&["lib", "paths"]),
        "a name the root neither defines nor names falls to its glob re-exports"
    );
}

#[test]
fn rust_unimported_module_still_reports_beside_resolved_root_imports() {
    let root = demo_root(&["analyze"], &[("render", "emit")]);
    let test = evidence(
        Lang::Rust,
        "crates/demo",
        &[
            "demo",
            "demo/analyze",
            "demo/render",
            "demo::paths",
            "demo::paths/join",
        ],
    );
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/tests/rendering.rs", &test)]),
        demo_paths(&["hooks"]),
        "root, re-export source, and imported module are covered; the module nobody imports is not"
    );
}

#[test]
fn rust_root_import_from_another_workspace_crate_resolves_through_the_named_crate() {
    let root = demo_root(&[], &[("render", "emit")]);
    let own = evidence(Lang::Rust, "crates/demo", &["serde", "serde/Serialize"]);
    let foreign = evidence(Lang::Rust, "crates/other", &["demo", "demo/render"]);
    assert_eq!(
        demo_flagged(
            &root,
            &[
                ("crates/demo/tests/rendering.rs", &own),
                ("crates/other/tests/render.rs", &foreign),
            ]
        ),
        demo_paths(&["lib", "hooks", "paths"]),
        "a test in another workspace crate that imports `demo::render` names the module behind it"
    );
}

#[test]
fn rust_root_import_of_another_crate_does_not_resolve_in_the_importing_crate() {
    let root = demo_root(&["analyze"], &[("render", "emit")]);
    let test = evidence(
        Lang::Rust,
        "crates/demo",
        &["other", "other/render", "other/analyze"],
    );
    assert_eq!(
        demo_flagged(&root, &[("crates/demo/tests/rendering.rs", &test)]),
        demo_paths(&["lib", "emit", "hooks", "paths"]),
        "`other::render` names an item of `other`, not the `render` that `demo` re-exports"
    );
}
