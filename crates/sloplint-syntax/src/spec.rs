//! Grammar-driven role tables: every node kind of a grammar mapped to its walker role.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;

use crate::hooks::LangHooks;
use crate::lang;

/// What the walker does when it enters a node of a given kind.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    None,
    Function,
    Closure,
    If,
    ElseClause,
    ElseIf,
    Match,
    Loop,
    /// A `with`-style block: bumps nesting depth only, adds no cognitive score.
    With,
    Catch,
    Ternary,
    Call,
    MacroCall,
    NewCall,
    Decl,
    Bind,
    Write,
    Import,
    Export,
    TypeDef,
    Impl,
    Module,
    Comment,
    MatchArm,
    StructLit,
    Member,
    Subscript,
    Assert,
    Global,
    StaticItem,
}

/// One language's grammar, its kind-to-role table, and its adapter hooks.
pub struct LangSpec {
    /// The language this spec serves.
    pub lang: Lang,
    /// The grammar the roles and hooks were built against.
    pub language: tree_sitter::Language,
    pub(crate) roles: Vec<Role>,
    pub(crate) hooks: &'static dyn LangHooks,
}

/// Every `Lang` variant, in declaration order.
const ALL_LANGS: [Lang; 25] = [
    Lang::Rust,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::JavaScript,
    Lang::Python,
    Lang::Go,
    Lang::Java,
    Lang::Kotlin,
    Lang::CSharp,
    Lang::C,
    Lang::Cpp,
    Lang::ObjC,
    Lang::Php,
    Lang::Ruby,
    Lang::Bash,
    Lang::PowerShell,
    Lang::Swift,
    Lang::R,
    Lang::Scala,
    Lang::Dart,
    Lang::Zig,
    Lang::Ocaml,
    Lang::OcamlInterface,
    Lang::Odin,
    Lang::Starlark,
];

/// The process-wide spec of each language, built once on first use.
pub fn spec(lang: Lang) -> &'static LangSpec {
    static SPECS: LazyLock<Vec<LangSpec>> =
        LazyLock::new(|| ALL_LANGS.iter().map(|&lang| lang::spec_for(lang)).collect());
    &SPECS[lang as usize]
}

impl LangSpec {
    /// Builds a spec, mapping every table name onto its grammar kind ids.
    ///
    /// A table name matching no kind id is a grammar/version mismatch, a
    /// programming error, so it panics at startup.
    pub(crate) fn new(
        lang: Lang,
        language: tree_sitter::Language,
        table: &[(&str, Role)],
        hooks: &'static dyn LangHooks,
    ) -> LangSpec {
        LangSpec {
            lang,
            language: language.clone(),
            roles: Self::roles_from(lang, language, table),
            hooks,
        }
    }

    /// The role vector indexed by node kind id: `None` for unmapped kinds.
    ///
    /// Table entries first match named kinds; an entry matching no named kind
    /// falls back to anonymous keyword kinds so keyword tokens like Ruby's
    /// `else` still map.
    pub(crate) fn roles_from(
        lang: Lang,
        language: tree_sitter::Language,
        table: &[(&str, Role)],
    ) -> Vec<Role> {
        let count = u16::try_from(language.node_kind_count()).unwrap_or(u16::MAX);
        let mut roles = vec![Role::None; count as usize];
        let mut matched = vec![false; table.len()];
        apply_named(language.clone(), table, &mut roles, &mut matched);
        apply_fallback(lang, language, table, &mut roles, &mut matched);
        roles
    }

    /// The role of a node kind id; `None` when out of range.
    pub(crate) fn role_for(&self, kind_id: u16) -> Role {
        self.roles
            .get(kind_id as usize)
            .copied()
            .unwrap_or(Role::None)
    }
}

/// First pass: every named kind whose kind name appears in the table.
fn apply_named(
    language: tree_sitter::Language,
    table: &[(&str, Role)],
    roles: &mut [Role],
    matched: &mut [bool],
) {
    for id in 0..u16::try_from(language.node_kind_count()).unwrap_or(u16::MAX) {
        if !language.node_kind_is_named(id) {
            continue;
        }
        let Some(name) = language.node_kind_for_id(id) else {
            continue;
        };
        if let Some(index) = table.iter().position(|(candidate, _)| *candidate == name) {
            roles[id as usize] = table[index].1;
            matched[index] = true;
        }
    }
}

/// Second pass: table entries with no named kind match anonymous keyword ids.
fn apply_fallback(
    lang: Lang,
    language: tree_sitter::Language,
    table: &[(&str, Role)],
    roles: &mut [Role],
    matched: &mut [bool],
) {
    for (index, (name, role)) in table.iter().enumerate() {
        if matched[index] {
            continue;
        }
        let mut found = false;
        for id in 0..u16::try_from(language.node_kind_count()).unwrap_or(u16::MAX) {
            if language.node_kind_for_id(id) == Some(*name) {
                roles[id as usize] = *role;
                found = true;
            }
        }
        if !found {
            panic!("grammar for {lang:?} lacks node kind {name}");
        }
    }
}
