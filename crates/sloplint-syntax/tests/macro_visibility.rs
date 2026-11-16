use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;

#[path = "analyze_facts/support.rs"]
mod support;

const PRIVATE_QT: &str =
    include_str!("../../sloplint/tests/fixtures/grep/generic-name-near-miss__cpp/src/qt.cpp");
const PUBLIC_QT: &str =
    include_str!("../../sloplint/tests/fixtures/grep/generic-name__cpp/src/qt.cpp");

#[test]
fn qt_macros_start_private_member_sections() {
    let analysis = support::run_clean(Lang::Cpp, "src/qt.cpp", PRIVATE_QT);
    let functions: Vec<_> = analysis
        .facts
        .functions
        .iter()
        .map(|function| (function.name, function.visibility))
        .collect();
    assert_eq!(
        functions,
        [
            ("process", Visibility::Private),
            ("handle", Visibility::Private),
            ("manage", Visibility::Private),
            ("perform", Visibility::Private),
        ]
    );
}

#[test]
fn explicit_public_after_qt_macros_exports_members() {
    let analysis = support::run_clean(Lang::Cpp, "src/qt.cpp", PUBLIC_QT);
    let functions: Vec<_> = analysis
        .facts
        .functions
        .iter()
        .map(|function| (function.name, function.visibility))
        .collect();
    assert_eq!(
        functions,
        [
            ("process", Visibility::Exported),
            ("handle", Visibility::Exported),
            ("manage", Visibility::Exported),
            ("perform", Visibility::Exported),
        ]
    );
}

#[test]
fn static_inline_macros_keep_c_and_cpp_helpers_private() {
    for (lang, path) in [(Lang::C, "src/board.c"), (Lang::Cpp, "src/board.cpp")] {
        for qualifier in ["__STATIC_INLINE", "__STATIC_FORCEINLINE"] {
            let source = format!("{qualifier} int process(int value) {{ return value; }}\n");
            let analysis = support::run_clean(lang, path, &source);
            assert_eq!(analysis.facts.functions.len(), 1, "{lang:?}: {qualifier}");
            assert_eq!(
                analysis.facts.functions[0].visibility,
                Visibility::Private,
                "{lang:?}: {qualifier}"
            );
        }
    }
}
