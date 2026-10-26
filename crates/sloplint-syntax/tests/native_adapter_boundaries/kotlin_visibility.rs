//! Kotlin visibility boundaries: default public, explicit public/private/protected/internal,
//! container propagation, and nested function restrictions.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

fn visibilities(source: &str) -> Vec<(String, bool)> {
    with_facts(Lang::Kotlin, "case.kt", source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| (f.name.to_owned(), f.public))
            .collect()
    })
}

#[test]
fn top_level_visibility_modifiers() {
    let source = "fun top_default() { }\npublic fun top_public() { }\nprivate fun top_private() { }\nprotected fun top_protected() { }\ninternal fun top_internal() { }\n";
    assert_eq!(
        visibilities(source),
        [
            ("top_default", true),
            ("top_public", true),
            ("top_private", false),
            ("top_protected", false),
            ("top_internal", false),
        ]
        .into_iter()
        .map(|(name, public)| (name.to_owned(), public))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn container_and_nested_visibility() {
    let source = "class OpenClass {\n    fun open_member() { }\n    private fun hidden_member() { }\n    fun outer() {\n        fun nested() { }\n    }\n}\n\nprivate class HiddenClass {\n    fun inside_hidden() { }\n}\n\ninternal class ScopedClass {\n    fun inside_scoped() { }\n}\n\ninterface Service {\n    fun action() { }\n}\n\nprivate interface HiddenService {\n    fun action() { }\n}\n";
    assert_eq!(
        visibilities(source),
        [
            ("open_member", true),
            ("hidden_member", false),
            ("outer", true),
            ("nested", false),
            ("inside_hidden", false),
            ("inside_scoped", false),
            ("action", true),
            ("action", false),
        ]
        .into_iter()
        .map(|(name, public)| (name.to_owned(), public))
        .collect::<Vec<_>>(),
    );
}
