//! Kotlin visibility boundaries: default public, explicit public/private/protected/internal,
//! container propagation, and nested function restrictions.

use crate::support::with_facts;
use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;

fn visibilities(source: &str) -> Vec<(String, Visibility)> {
    with_facts(Lang::Kotlin, "case.kt", source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| (f.name.to_owned(), f.visibility))
            .collect()
    })
}

#[test]
fn top_level_visibility_modifiers() {
    let source = "fun top_default() { }\npublic fun top_public() { }\nprivate fun top_private() { }\nprotected fun top_protected() { }\ninternal fun top_internal() { }\n";
    assert_eq!(
        visibilities(source),
        [
            ("top_default", Visibility::Exported),
            ("top_public", Visibility::Exported),
            ("top_private", Visibility::Private),
            ("top_protected", Visibility::Exported),
            ("top_internal", Visibility::Restricted),
        ]
        .into_iter()
        .map(|(name, visibility)| (name.to_owned(), visibility))
        .collect::<Vec<_>>(),
        "default, public, and protected export; private hides; internal restricts",
    );
}

#[test]
fn container_and_nested_visibility() {
    let source = "class OpenClass {\n    fun open_member() { }\n    private fun hidden_member() { }\n    fun outer() {\n        fun nested() { }\n    }\n}\n\nprivate class HiddenClass {\n    fun inside_hidden() { }\n}\n\ninternal class ScopedClass {\n    fun inside_scoped() { }\n}\n\ninterface Service {\n    fun action() { }\n}\n\nprivate interface HiddenService {\n    fun action() { }\n}\n";
    assert_eq!(
        visibilities(source),
        [
            ("open_member", Visibility::Exported),
            ("hidden_member", Visibility::Private),
            ("outer", Visibility::Exported),
            ("nested", Visibility::Private),
            ("inside_hidden", Visibility::Private),
            ("inside_scoped", Visibility::Restricted),
            ("action", Visibility::Exported),
            ("action", Visibility::Private),
        ]
        .into_iter()
        .map(|(name, visibility)| (name.to_owned(), visibility))
        .collect::<Vec<_>>(),
        "containers cap member visibility and nested functions stay private",
    );
}
