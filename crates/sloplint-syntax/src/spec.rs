use std::sync::LazyLock;

use sloplint_core::lang::{ALL_LANGS, Lang};

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

pub struct LangSpec {
    pub lang: Lang,
    pub(crate) language: tree_sitter::Language,
    pub(crate) roles: Vec<Role>,
    pub(crate) hooks: &'static dyn LangHooks,
}

pub(crate) fn spec(lang: Lang) -> &'static LangSpec {
    static SPECS: LazyLock<Vec<LangSpec>> =
        LazyLock::new(|| ALL_LANGS.iter().map(|&lang| lang::spec_for(lang)).collect());
    &SPECS[lang as usize]
}

impl LangSpec {
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

    /// Keyword table entries match anonymous kinds: tree-sitter marks keywords unnamed.
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

    pub(crate) fn role_for(&self, kind_id: u16) -> Role {
        self.roles
            .get(kind_id as usize)
            .copied()
            .unwrap_or(Role::None)
    }
}

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
