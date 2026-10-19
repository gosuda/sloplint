//! Per-file facts shared by the syntax walker, the rules, and the cache.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::finding::source_site_indexed;
use crate::lang::Lang;

/// A source position: `line` is 1-based and `byte` is the offset in the BOM-stripped source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub byte: u32,
}

/// Everything one analyzed file contributes; all strings borrow the file source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts<'s> {
    pub lang: Lang,
    pub path: &'s str,
    pub source: &'s str,
    /// The byte offset where each line of `source` starts, from [`crate::finding::line_starts`].
    ///
    /// The index belongs to `source`; [`crate::finding::Finding::at`] and the world sites read it
    /// to find a line without rescanning the file.
    pub line_starts: Vec<u32>,
    pub is_test_file: bool,
    pub is_entry_file: bool,
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

/// One named function or method with its complexity metrics and surface annotations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub doc: Vec<&'s str>,
    pub doc_pos: Option<Pos>,
    pub public: bool,
    pub interface_bound: bool,
    pub in_test: bool,
    pub params: Vec<Param<'s>>,
    pub return_type: Option<&'s str>,
    /// True when the signature declares plain `throws` with no error type.
    ///
    /// Typed `throws(E)`, `rethrows`, and languages without the clause are false.
    pub untyped_throws: bool,
    pub missing_annotations: Vec<&'s str>,
    pub cognitive: u32,
    pub max_depth: u32,
    pub deepest_line: u32,
    pub sole_call: Option<u32>,
}

/// One function parameter; the receiver (`self`/`this`/`cls`/Go receiver) is never a `Param`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param<'s> {
    pub name: &'s str,
    pub ty: Option<&'s str>,
    pub bool_default: bool,
}

/// How a call site invokes its callee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallKind {
    Plain,
    Macro,
    New,
}

/// One call site with its argument shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call<'s> {
    pub pos: Pos,
    pub callee: &'s str,
    pub kind: CallKind,
    pub callee_root: Option<&'s str>,
    pub args: Vec<Arg<'s>>,
}

/// One call argument: its line span and the shape of its expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg<'s> {
    pub shape: ArgShape<'s>,
    pub start_line: u32,
    pub end_line: u32,
}

/// The literal-run shape of a call argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgShape<'s> {
    Str { longest_static: u32 },
    Concat,
    Ident(&'s str),
    Other,
}

/// One mutable binding with its span of mutability and deepest nested write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutBinding<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub last_write_line: u32,
    pub max_nested_depth: u32,
    pub nested_write_pos: Option<Pos>,
}

/// How an import brings names into scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKind {
    Named,
    Namespace,
    Wildcard,
    SideEffect,
}

/// One import site; `module` is owned when a language builds it from pieces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import<'s> {
    pub pos: Pos,
    pub module: Cow<'s, str>,
    pub name: &'s str,
    pub alias: Option<&'s str>,
    pub kind: ImportKind,
    pub in_test: bool,
}

/// One named type declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub public: bool,
    pub abstraction: bool,
}

/// One trait, interface, or superclass reference from an implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplRef<'s> {
    pub name: &'s str,
    pub pos: Pos,
}

/// One match arm that catches everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildcardArm {
    pub pos: Pos,
    pub enum_like: bool,
}

/// One statement kind inside an error handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt<'s> {
    Call { callee: &'s str, kind: CallKind },
    Rethrow,
    Other,
}

/// One catch/rescue/recover handler and its statement sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler<'s> {
    pub pos: Pos,
    pub stmts: Vec<Stmt<'s>>,
}

/// One assertion call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assertion {
    pub pos: Pos,
    pub has_message: bool,
    pub comparison: bool,
}

/// One struct/object literal that copies fields one-to-one from another value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldCopy<'s> {
    pub pos: Pos,
    pub fields: u32,
    pub source: Option<&'s str>,
}

/// One construct that resolves a symbol by a runtime-built name; `what` names the mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicSymbol {
    pub pos: Pos,
    pub what: &'static str,
}

/// The kind of ambient state a construct touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmbientKind {
    GlobalMutable,
    GlobalWrite,
    EnvRead,
}

/// One ambient-state site; `in_function` is the index of the enclosing function when any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ambient<'s> {
    pub pos: Pos,
    pub kind: AmbientKind,
    pub name: &'s str,
    pub in_function: Option<u32>,
}

/// Barrel-file candidates: per re-export, candidate `/`-separated rel paths in priority order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barrel {
    pub targets: Vec<Vec<String>>,
}

/// One owned, line-addressed name site for world-level rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedSite {
    pub name: Box<str>,
    pub line: u32,
    pub column: u32,
    pub context: Box<str>,
}

/// The owned, serializable projection of `FileFacts` that world rules and the cache consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldFacts {
    pub lang: Lang,
    pub is_test_file: bool,
    pub is_entry_file: bool,
    pub has_inline_tests: bool,
    pub public_functions: u32,
    pub definitions: Vec<NamedSite>,
    pub abstractions: Vec<NamedSite>,
    pub impls: Vec<NamedSite>,
    pub barrel: Option<Vec<Vec<String>>>,
}

impl WorldFacts {
    /// Projects borrowed file facts into the owned world view; line/column/context come from the same helper as findings.
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
            is_test_file: facts.is_test_file,
            is_entry_file: facts.is_entry_file,
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
    let (column, context) = source_site_indexed(&facts.line_starts, facts.source, pos);
    NamedSite {
        name: name.to_owned().into_boxed_str(),
        line: pos.line,
        column,
        context,
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
            is_test_file: false,
            is_entry_file: true,
            line_count: 2,
            functions: vec![
                Function {
                    name: "keep",
                    pos: Pos { line: 1, byte: 8 },
                    doc: vec![],
                    doc_pos: None,
                    public: true,
                    interface_bound: false,
                    in_test: false,
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
                    in_test: false,
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
                    in_test: false,
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
        assert_eq!(world.definitions[0].context.as_ref(), "pub fn keep() {}");
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
}
