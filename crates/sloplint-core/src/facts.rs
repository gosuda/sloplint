use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::finding::source_column;
use crate::lang::Lang;
use crate::role::{CodeRole, RoleRegion, role_at};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub byte: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts<'s> {
    pub lang: Lang,
    pub path: &'s str,
    pub source: &'s str,
    pub line_starts: Vec<u32>,
    pub role: CodeRole,
    pub regions: Vec<RoleRegion>,
    pub line_count: u32,
    pub functions: Vec<Function<'s>>,
    pub calls: Vec<Call<'s>>,
    pub bindings: Vec<MutBinding<'s>>,
    pub imports: Vec<Import<'s>>,
    pub types: Vec<TypeDef<'s>>,
    pub impls: Vec<ImplRef<'s>>,
    pub wildcard_arms: Vec<WildcardArm>,
    pub handlers: Vec<Handler<'s>>,
    pub assertions: Vec<Assertion>,
    pub field_copies: Vec<FieldCopy<'s>>,
    pub dynamic_symbols: Vec<DynamicSymbol>,
    pub ambient: Vec<Ambient<'s>>,
    pub barrel: Option<Barrel>,
    pub has_inline_tests: bool,
}

impl<'s> FileFacts<'s> {
    pub fn role_at(&self, line: u32) -> CodeRole {
        role_at(self.role, &self.regions, line)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub doc: Vec<&'s str>,
    pub doc_pos: Option<Pos>,
    pub public: bool,
    pub interface_bound: bool,
    pub params: Vec<Param<'s>>,
    pub return_type: Option<&'s str>,
    pub untyped_throws: bool,
    pub missing_annotations: Vec<&'s str>,
    pub cognitive: u32,
    pub max_depth: u32,
    pub deepest_line: u32,
    pub sole_call: Option<u32>,
}

/// The receiver (`self`/`this`/`cls`/Go receiver) is never a `Param`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param<'s> {
    pub name: &'s str,
    pub ty: Option<&'s str>,
    pub bool_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallKind {
    Plain,
    Macro,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call<'s> {
    pub pos: Pos,
    pub callee: &'s str,
    pub kind: CallKind,
    pub callee_root: Option<&'s str>,
    pub args: Vec<Arg<'s>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg<'s> {
    pub shape: ArgShape<'s>,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgShape<'s> {
    Str { longest_static: u32 },
    Concat,
    Ident(&'s str),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutBinding<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub last_write_line: u32,
    pub max_nested_depth: u32,
    pub nested_write_pos: Option<Pos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKind {
    Named,
    Namespace,
    Wildcard,
    SideEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import<'s> {
    pub pos: Pos,
    pub module: Cow<'s, str>,
    pub name: &'s str,
    pub alias: Option<&'s str>,
    pub kind: ImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub public: bool,
    pub abstraction: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplRef<'s> {
    pub name: &'s str,
    pub pos: Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildcardArm {
    pub pos: Pos,
    pub enum_like: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt<'s> {
    Call { callee: &'s str, kind: CallKind },
    Rethrow,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler<'s> {
    pub pos: Pos,
    pub stmts: Vec<Stmt<'s>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assertion {
    pub pos: Pos,
    pub has_message: bool,
    pub comparison: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldCopy<'s> {
    pub pos: Pos,
    pub fields: u32,
    pub source: Option<&'s str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicSymbol {
    pub pos: Pos,
    pub what: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmbientKind {
    GlobalMutable,
    GlobalWrite,
    EnvRead,
}

/// `in_function` indexes [`FileFacts::functions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ambient<'s> {
    pub pos: Pos,
    pub kind: AmbientKind,
    pub name: &'s str,
    pub in_function: Option<u32>,
}

/// Per re-export, candidate `/`-separated rel paths in priority order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barrel {
    pub targets: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedSite {
    pub name: Box<str>,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldFacts {
    pub lang: Lang,
    pub role: CodeRole,
    pub regions: Vec<RoleRegion>,
    pub has_inline_tests: bool,
    pub public_functions: u32,
    pub definitions: Vec<NamedSite>,
    pub abstractions: Vec<NamedSite>,
    pub impls: Vec<NamedSite>,
    pub barrel: Option<Vec<Vec<String>>>,
}

impl WorldFacts {
    pub fn from_facts(facts: &FileFacts<'_>) -> WorldFacts {
        let public_functions =
            u32::try_from(facts.functions.iter().filter(|f| f.public).count()).unwrap_or(u32::MAX);
        let mut definitions: Vec<NamedSite> = facts
            .functions
            .iter()
            .filter(|f| f.public && !f.interface_bound)
            .map(|f| named_site(facts, f.pos, f.name))
            .collect();
        definitions.extend(
            facts
                .types
                .iter()
                .filter(|t| t.public)
                .map(|t| named_site(facts, t.pos, t.name)),
        );
        WorldFacts {
            lang: facts.lang,
            role: facts.role,
            regions: facts.regions.clone(),
            has_inline_tests: facts.has_inline_tests,
            public_functions,
            definitions,
            abstractions: facts
                .types
                .iter()
                .filter(|t| t.abstraction)
                .map(|t| named_site(facts, t.pos, t.name))
                .collect(),
            impls: facts
                .impls
                .iter()
                .map(|i| named_site(facts, i.pos, i.name))
                .collect(),
            barrel: facts.barrel.as_ref().map(|b| b.targets.clone()),
        }
    }
}

fn named_site(facts: &FileFacts<'_>, pos: Pos, name: &str) -> NamedSite {
    NamedSite {
        name: name.to_owned().into_boxed_str(),
        line: pos.line,
        column: source_column(&facts.line_starts, facts.source, pos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::line_starts;

    #[test]
    fn projects_public_surface_and_barrel() {
        let source = "pub fn keep() {}\nfn hidden() {}\n";
        let facts = FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: CodeRole::Entry,
            regions: Vec::new(),
            line_count: 2,
            functions: vec![
                Function {
                    name: "keep",
                    pos: Pos { line: 1, byte: 8 },
                    doc: vec![],
                    doc_pos: None,
                    public: true,
                    interface_bound: false,
                    params: vec![],
                    return_type: None,
                    untyped_throws: false,
                    missing_annotations: vec![],
                    cognitive: 0,
                    max_depth: 0,
                    deepest_line: 0,
                    sole_call: None,
                },
                Function {
                    name: "sealed",
                    pos: Pos { line: 1, byte: 8 },
                    doc: vec![],
                    doc_pos: None,
                    public: true,
                    interface_bound: true,
                    params: vec![],
                    return_type: None,
                    untyped_throws: false,
                    missing_annotations: vec![],
                    cognitive: 0,
                    max_depth: 0,
                    deepest_line: 0,
                    sole_call: None,
                },
                Function {
                    name: "hidden",
                    pos: Pos { line: 2, byte: 4 },
                    doc: vec![],
                    doc_pos: None,
                    public: false,
                    interface_bound: false,
                    params: vec![],
                    return_type: None,
                    untyped_throws: false,
                    missing_annotations: vec![],
                    cognitive: 0,
                    max_depth: 0,
                    deepest_line: 0,
                    sole_call: None,
                },
            ],
            calls: vec![],
            bindings: vec![],
            imports: vec![],
            types: vec![TypeDef {
                name: "Shape",
                pos: Pos { line: 2, byte: 100 },
                public: true,
                abstraction: true,
            }],
            impls: vec![ImplRef {
                name: "Draw",
                pos: Pos { line: 1, byte: 0 },
            }],
            wildcard_arms: vec![],
            handlers: vec![],
            assertions: vec![],
            field_copies: vec![],
            dynamic_symbols: vec![],
            ambient: vec![],
            barrel: Some(Barrel {
                targets: vec![vec!["a.rs".to_string(), "a/mod.rs".to_string()]],
            }),
            has_inline_tests: false,
        };
        let world = WorldFacts::from_facts(&facts);
        assert_eq!(world.public_functions, 2);
        assert_eq!(
            world
                .definitions
                .iter()
                .map(|d| d.name.as_ref())
                .collect::<Vec<_>>(),
            ["keep", "Shape"]
        );
        assert_eq!(world.definitions[0].line, 1);
        assert_eq!(world.definitions[0].column, 9);
        assert_eq!(
            world
                .abstractions
                .iter()
                .map(|a| a.name.as_ref())
                .collect::<Vec<_>>(),
            ["Shape"]
        );
        assert_eq!(world.impls.len(), 1);
        assert_eq!(
            world.barrel,
            Some(vec![vec!["a.rs".to_string(), "a/mod.rs".to_string()]])
        );
    }

    #[test]
    fn serialized_world_facts_carry_no_line_text() {
        let source = "pub fn listed() { let withheld_token = 1; }\n";
        let facts = FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 1,
            functions: vec![Function {
                name: "listed",
                pos: Pos { line: 1, byte: 8 },
                doc: vec![],
                doc_pos: None,
                public: true,
                interface_bound: false,
                params: vec![],
                return_type: None,
                untyped_throws: false,
                missing_annotations: vec![],
                cognitive: 0,
                max_depth: 0,
                deepest_line: 0,
                sole_call: None,
            }],
            calls: vec![],
            bindings: vec![],
            imports: vec![],
            types: vec![],
            impls: vec![],
            wildcard_arms: vec![],
            handlers: vec![],
            assertions: vec![],
            field_copies: vec![],
            dynamic_symbols: vec![],
            ambient: vec![],
            barrel: None,
            has_inline_tests: false,
        };
        let world = WorldFacts::from_facts(&facts);
        let serialized = String::from_utf8(
            sonic_rs::to_vec(&world).unwrap_or_else(|e| panic!("serialization failed: {e}")),
        )
        .unwrap_or_else(|e| panic!("serialized bytes are not UTF-8: {e}"));
        assert!(
            !serialized.contains("withheld_token"),
            "world facts serialization must not carry source line text: {serialized}"
        );
        assert!(
            serialized.contains("\"line\":1") && serialized.contains("\"column\":9"),
            "positions stay in the serialization: {serialized}"
        );
    }
}
