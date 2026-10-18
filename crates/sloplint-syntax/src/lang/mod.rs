//! Grammar wiring: one adapter module per language plus the grammar table.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;

use crate::hooks::LangHooks;
use crate::spec::{LangSpec, Role};

pub(crate) mod bash;
pub(crate) mod c;
pub(crate) mod cpp;
pub(crate) mod csharp;
pub(crate) mod dart;
pub(crate) mod go;
pub(crate) mod java;
pub(crate) mod javascript;
pub(crate) mod kotlin;
pub(crate) mod objc;
pub(crate) mod ocaml;
pub(crate) mod odin;
pub(crate) mod php;
pub(crate) mod powershell;
pub(crate) mod python;
pub(crate) mod r;
pub(crate) mod ruby;
pub(crate) mod rust;
pub(crate) mod scala;
pub(crate) mod starlark;
pub(crate) mod swift;
pub(crate) mod typescript;
pub(crate) mod zig;

/// A role table exported by one adapter module.
pub(crate) type Table = &'static [(&'static str, Role)];

/// One wired grammar: its language, role table, and adapter hooks.
struct Grammar {
    lang: Lang,
    language: tree_sitter::Language,
    table: Table,
    hooks: &'static dyn LangHooks,
}

/// Every grammar wired to its `Lang`; TypeScript and TSX share one adapter,
/// as do OCaml and its interface grammar.
static GRAMMARS: LazyLock<Vec<Grammar>> = LazyLock::new(|| {
    vec![
        grammar(Lang::Rust, tree_sitter_rust::LANGUAGE, rust::TABLE, &rust::HOOKS),
        grammar(
            Lang::TypeScript,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            typescript::TABLE,
            &typescript::HOOKS,
        ),
        grammar(
            Lang::Tsx,
            tree_sitter_typescript::LANGUAGE_TSX,
            typescript::TABLE,
            &typescript::HOOKS,
        ),
        grammar(
            Lang::JavaScript,
            tree_sitter_javascript::LANGUAGE,
            javascript::TABLE,
            &javascript::HOOKS,
        ),
        grammar(
            Lang::Python,
            tree_sitter_python::LANGUAGE,
            python::TABLE,
            &python::HOOKS,
        ),
        grammar(Lang::Go, tree_sitter_go::LANGUAGE, go::TABLE, &go::HOOKS),
        grammar(
            Lang::Java,
            tree_sitter_java::LANGUAGE,
            java::TABLE,
            &java::HOOKS,
        ),
        grammar(
            Lang::Kotlin,
            tree_sitter_kotlin_ng::LANGUAGE,
            kotlin::TABLE,
            &kotlin::HOOKS,
        ),
        grammar(
            Lang::CSharp,
            tree_sitter_c_sharp::LANGUAGE,
            csharp::TABLE,
            &csharp::HOOKS,
        ),
        grammar(Lang::C, tree_sitter_c::LANGUAGE, c::TABLE, &c::HOOKS),
        grammar(Lang::Cpp, tree_sitter_cpp::LANGUAGE, cpp::TABLE, &cpp::HOOKS),
        grammar(
            Lang::ObjC,
            tree_sitter_objc::LANGUAGE,
            objc::TABLE,
            &objc::HOOKS,
        ),
        grammar(
            Lang::Php,
            tree_sitter_php::LANGUAGE_PHP,
            php::TABLE,
            &php::HOOKS,
        ),
        grammar(
            Lang::Ruby,
            tree_sitter_ruby::LANGUAGE,
            ruby::TABLE,
            &ruby::HOOKS,
        ),
        grammar(
            Lang::Bash,
            tree_sitter_bash::LANGUAGE,
            bash::TABLE,
            &bash::HOOKS,
        ),
        grammar(
            Lang::PowerShell,
            tree_sitter_powershell::LANGUAGE,
            powershell::TABLE,
            &powershell::HOOKS,
        ),
        grammar(
            Lang::Swift,
            tree_sitter_swift::LANGUAGE,
            swift::TABLE,
            &swift::HOOKS,
        ),
        grammar(Lang::R, tree_sitter_r::LANGUAGE, r::TABLE, &r::HOOKS),
        grammar(
            Lang::Scala,
            tree_sitter_scala::LANGUAGE,
            scala::TABLE,
            &scala::HOOKS,
        ),
        grammar(
            Lang::Dart,
            tree_sitter_dart::LANGUAGE,
            dart::TABLE,
            &dart::HOOKS,
        ),
        grammar(
            Lang::Zig,
            tree_sitter_zig::LANGUAGE,
            zig::TABLE,
            &zig::HOOKS,
        ),
        grammar(
            Lang::Ocaml,
            tree_sitter_ocaml::LANGUAGE_OCAML,
            ocaml::TABLE,
            &ocaml::HOOKS,
        ),
        grammar(
            Lang::OcamlInterface,
            tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE,
            ocaml::TABLE,
            &ocaml::HOOKS,
        ),
        grammar(
            Lang::Odin,
            tree_sitter_odin::LANGUAGE,
            odin::TABLE,
            &odin::HOOKS,
        ),
        grammar(
            Lang::Starlark,
            tree_sitter_starlark::LANGUAGE,
            starlark::TABLE,
            &starlark::HOOKS,
        ),
    ]
});

/// Builds one [`Grammar`], converting the language function to a [`tree_sitter::Language`].
fn grammar(
    lang: Lang,
    language: impl Into<tree_sitter::Language>,
    table: Table,
    hooks: &'static dyn LangHooks,
) -> Grammar {
    Grammar {
        lang,
        language: language.into(),
        table,
        hooks,
    }
}

/// Builds the spec of one language by looking up its wired grammar.
///
/// A missing entry means `spec.rs` and this table disagree about the
/// `Lang` variants, a programming error.
pub(crate) fn spec_for(lang: Lang) -> LangSpec {
    let Some(entry) = GRAMMARS.iter().find(|grammar| grammar.lang == lang) else {
        panic!("no grammar wired for {lang:?}");
    };
    LangSpec::new(lang, entry.language, entry.table, entry.hooks)
}
