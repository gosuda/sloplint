use std::collections::HashSet;

use sloplint_core::lang::Lang;

use crate::hooks::LangHooks;
use crate::spec::{LangSpec, Role, spec};

pub(crate) mod bash;
pub(crate) mod c;
pub(crate) mod c_family;
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

pub(crate) type Table = &'static [(&'static str, Role)];

fn grammar(lang: Lang) -> (tree_sitter::Language, Table, &'static dyn LangHooks) {
    match lang {
        Lang::Rust => (tree_sitter_rust::LANGUAGE.into(), rust::TABLE, &rust::HOOKS),
        Lang::TypeScript => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            typescript::TABLE,
            &typescript::HOOKS,
        ),
        Lang::Tsx => (
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            typescript::TABLE,
            &typescript::HOOKS,
        ),
        Lang::JavaScript => (
            tree_sitter_javascript::LANGUAGE.into(),
            javascript::TABLE,
            &javascript::HOOKS,
        ),
        Lang::Python => (
            tree_sitter_python::LANGUAGE.into(),
            python::TABLE,
            &python::HOOKS,
        ),
        Lang::Go => (tree_sitter_go::LANGUAGE.into(), go::TABLE, &go::HOOKS),
        Lang::Java => (tree_sitter_java::LANGUAGE.into(), java::TABLE, &java::HOOKS),
        Lang::Kotlin => (
            tree_sitter_kotlin_ng::LANGUAGE.into(),
            kotlin::TABLE,
            &kotlin::HOOKS,
        ),
        Lang::CSharp => (
            tree_sitter_c_sharp::LANGUAGE.into(),
            csharp::TABLE,
            &csharp::HOOKS,
        ),
        Lang::C => (tree_sitter_c::LANGUAGE.into(), c::TABLE, &c::HOOKS),
        Lang::Cpp => (tree_sitter_cpp::LANGUAGE.into(), cpp::TABLE, &cpp::HOOKS),
        Lang::ObjC => (tree_sitter_objc::LANGUAGE.into(), objc::TABLE, &objc::HOOKS),
        Lang::Php => (
            tree_sitter_php::LANGUAGE_PHP.into(),
            php::TABLE,
            &php::HOOKS,
        ),
        Lang::Ruby => (tree_sitter_ruby::LANGUAGE.into(), ruby::TABLE, &ruby::HOOKS),
        Lang::Bash => (tree_sitter_bash::LANGUAGE.into(), bash::TABLE, &bash::HOOKS),
        Lang::PowerShell => (
            tree_sitter_powershell::LANGUAGE.into(),
            powershell::TABLE,
            &powershell::HOOKS,
        ),
        Lang::Swift => (
            tree_sitter_swift::LANGUAGE.into(),
            swift::TABLE,
            &swift::HOOKS,
        ),
        Lang::R => (tree_sitter_r::LANGUAGE.into(), r::TABLE, &r::HOOKS),
        Lang::Scala => (
            tree_sitter_scala::LANGUAGE.into(),
            scala::TABLE,
            &scala::HOOKS,
        ),
        Lang::Dart => (tree_sitter_dart::LANGUAGE.into(), dart::TABLE, &dart::HOOKS),
        Lang::Zig => (tree_sitter_zig::LANGUAGE.into(), zig::TABLE, &zig::HOOKS),
        Lang::Ocaml => (
            tree_sitter_ocaml::LANGUAGE_OCAML.into(),
            ocaml::TABLE,
            &ocaml::HOOKS,
        ),
        Lang::OcamlInterface => (
            tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE.into(),
            ocaml::TABLE,
            &ocaml::HOOKS,
        ),
        Lang::Odin => (tree_sitter_odin::LANGUAGE.into(), odin::TABLE, &odin::HOOKS),
        Lang::Starlark => (
            tree_sitter_starlark::LANGUAGE.into(),
            starlark::TABLE,
            &starlark::HOOKS,
        ),
    }
}

pub(crate) fn spec_for(lang: Lang) -> LangSpec {
    let (language, table, hooks) = grammar(lang);
    LangSpec::new(lang, language, table, hooks)
}

pub(crate) fn hooks_for(
    lang: Lang,
    path: &str,
    ocaml_interfaces: Option<&HashSet<String>>,
) -> &'static dyn LangHooks {
    let shadowed = ocaml_interfaces.is_some_and(|set| set.contains(path));
    if lang == Lang::Ocaml && shadowed {
        &ocaml::SHADOWED_HOOKS
    } else {
        spec(lang).hooks
    }
}
