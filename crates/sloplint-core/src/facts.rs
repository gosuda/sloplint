use std::borrow::Cow;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::finding::source_column;
use crate::lang::Lang;
use crate::role::{CodeRole, RoleRegion, role_at};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub byte: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Visibility {
    Private,
    Restricted,
    Exported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Str,
    Bool,
    Int,
    Float,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportScope {
    Module,
    FunctionLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrutineeKind {
    Ident,
    Field,
    Path,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertShape {
    BareName,
    TopEqNe,
    Other,
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
    pub package: String,
    pub is_generated: bool,
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
    pub visibility: Visibility,
    pub signature_forced: bool,
    pub entry_point: bool,
    pub in_test: bool,
    pub is_overload: bool,
    pub is_deprecated: bool,
    pub is_decorated: bool,
    pub is_trait_item: bool,
    pub is_method: bool,
    pub receiver: Option<&'s str>,
    pub params: Vec<Param<'s>>,
    pub return_type: Option<&'s str>,
    pub untyped_throws: bool,
    pub missing_annotations: Vec<&'s str>,
    pub cognitive: u32,
    pub max_depth: u32,
    pub deepest_line: u32,
    pub end_line: u32,
    pub has_value_return: bool,
    pub sole_call: Option<u32>,
}

impl Function<'_> {
    pub fn exported(&self) -> bool {
        self.visibility == Visibility::Exported
    }

    pub fn actionable_api(&self) -> bool {
        self.exported() && !self.signature_forced && !self.in_test
    }
}

/// The receiver (`self`/`this`/`cls`/Go receiver) is never a `Param`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param<'s> {
    pub name: &'s str,
    pub ty: Option<&'s str>,
    pub primitive_kind: PrimitiveKind,
    pub has_default: bool,
    pub bool_default: bool,
    pub keyword_only: bool,
    pub is_variadic: bool,
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
    pub is_simple_path: bool,
    pub initializes_binding: bool,
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
    Concat { longest_static: u32 },
    Ident(&'s str),
    Collection,
    CodeBody,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutBinding<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub last_write_line: u32,
    pub first_write_line: u32,
    pub write_lines: u32,
    pub loop_depth: u32,
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
    pub scope: ImportScope,
    pub in_test: bool,
    pub is_reexport: bool,
    pub external_root: Option<&'s str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub visibility: Visibility,
    pub abstraction: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplRef<'s> {
    pub name: &'s str,
    pub pos: Pos,
    pub is_generic: bool,
    pub child_is_abstract: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WildcardArm {
    pub pos: Pos,
    pub enum_like: bool,
    pub scrutinee: ScrutineeKind,
    pub variant_count: u32,
    pub has_nullable_ctor: bool,
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
    pub has_else: bool,
    pub try_imports_only: bool,
    pub in_cleanup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assertion {
    pub pos: Pos,
    pub has_message: bool,
    pub comparison: bool,
    pub shape: AssertShape,
    pub in_typecheck: bool,
    pub value_is_bool_literal: bool,
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
pub struct Reexport {
    pub name: Box<str>,
    pub module: Box<str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldFacts {
    pub lang: Lang,
    pub role: CodeRole,
    pub regions: Vec<RoleRegion>,
    #[serde(skip)]
    pub package: String,
    #[serde(skip)]
    pub package_from_manifest: bool,
    pub has_inline_tests: bool,
    pub public_functions: u32,
    pub definitions: Vec<NamedSite>,
    pub abstractions: Vec<NamedSite>,
    pub impls: Vec<NamedSite>,
    pub barrel: Option<Vec<Vec<String>>>,
    pub imports: Vec<Box<str>>,
    pub reexports: Vec<Reexport>,
}

impl WorldFacts {
    pub fn from_facts(facts: &FileFacts<'_>) -> WorldFacts {
        let public_functions =
            u32::try_from(facts.functions.iter().filter(|f| f.exported()).count())
                .unwrap_or(u32::MAX);
        let mut definitions: Vec<NamedSite> = facts
            .functions
            .iter()
            .filter(|f| {
                f.exported()
                    && !f.signature_forced
                    && !f.is_trait_item
                    && !f.is_method
                    && !f.entry_point
            })
            .map(|f| named_site(facts, f.pos, f.name))
            .collect();
        definitions.extend(
            facts
                .types
                .iter()
                .filter(|t| t.visibility == Visibility::Exported)
                .map(|t| named_site(facts, t.pos, t.name)),
        );
        WorldFacts {
            lang: facts.lang,
            role: facts.role,
            regions: facts.regions.clone(),
            package: facts.package.clone(),
            package_from_manifest: false,
            has_inline_tests: facts.has_inline_tests,
            public_functions,
            definitions,
            abstractions: facts
                .types
                .iter()
                .filter(|t| t.abstraction && t.visibility != Visibility::Exported)
                .map(|t| named_site(facts, t.pos, t.name))
                .collect(),
            impls: facts
                .impls
                .iter()
                .filter(|i| !i.is_generic && !i.child_is_abstract)
                .map(|i| named_site(facts, i.pos, i.name))
                .collect(),
            barrel: facts.barrel.as_ref().map(|b| b.targets.clone()),
            imports: import_targets(facts),
            reexports: reexports_of(facts),
        }
    }
}
fn import_targets(facts: &FileFacts<'_>) -> Vec<Box<str>> {
    let file_test = matches!(facts.role, CodeRole::Test | CodeRole::Bench);
    let mut targets = Vec::new();
    for import in &facts.imports {
        if !import.in_test && !file_test {
            continue;
        }
        let module: &str = &import.module;
        if module.is_empty() {
            continue;
        }
        if facts.lang == Lang::Go && is_single_segment(module) {
            continue;
        }
        targets.push(Box::<str>::from(module));
        let bound = module.rsplit(['/', '.']).next() != Some(import.name);
        if !import.name.is_empty() && import.name != "*" && bound {
            targets.push(Box::<str>::from(format!("{module}/{}", import.name)));
        }
    }
    targets
}
fn reexports_of(facts: &FileFacts<'_>) -> Vec<Reexport> {
    facts
        .imports
        .iter()
        .filter(|import| {
            import.is_reexport && !import.in_test && import.scope == ImportScope::Module
        })
        .map(|import| Reexport {
            name: Box::from(import.alias.unwrap_or(import.name)),
            module: Box::from(&*import.module),
        })
        .collect()
}
fn is_single_segment(module: &str) -> bool {
    !module.starts_with('.')
        && !module.contains('/')
        && !module.contains('.')
        && !module.contains(':')
}

fn named_site(facts: &FileFacts<'_>, pos: Pos, name: &str) -> NamedSite {
    NamedSite {
        name: name.to_owned().into_boxed_str(),
        line: pos.line,
        column: source_column(&facts.line_starts, facts.source, pos),
    }
}
/// Which ecosystem a manifest file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ManifestKind {
    JavaScript,
    Python,
}

fn manifest_kind_for(lang: Lang) -> Option<ManifestKind> {
    match lang {
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => Some(ManifestKind::JavaScript),
        Lang::Python => Some(ManifestKind::Python),
        _ => None,
    }
}

pub fn package_scope(
    lang: Lang,
    path: &str,
    manifests: &BTreeSet<(ManifestKind, String)>,
) -> (String, bool) {
    if let Some(kind) = manifest_kind_for(lang)
        && let Some(key) = nearest_manifest(path, manifests, kind)
    {
        return (key, true);
    }
    (legacy_package_key(lang, path), false)
}

fn nearest_manifest(
    path: &str,
    manifests: &BTreeSet<(ManifestKind, String)>,
    kind: ManifestKind,
) -> Option<String> {
    let mut prefix = path;
    while let Some(i) = prefix.rfind('/') {
        prefix = &prefix[..i];
        if manifests.contains(&(kind, prefix.to_string())) {
            return Some(prefix.to_string());
        }
    }
    if manifests.contains(&(kind, String::new())) {
        return Some(String::new());
    }
    None
}

fn legacy_package_key(lang: Lang, path: &str) -> String {
    match lang {
        Lang::Rust => {
            let mut parts = path.split('/');
            if parts.next() == Some("crates")
                && let Some(name) = parts.next()
                && !name.is_empty()
                && parts.next().is_some()
            {
                return ["crates", name].join("/");
            }
            String::new()
        }
        Lang::Go => match path.rfind('/') {
            Some(i) => path[..i].to_string(),
            None => String::new(),
        },
        _ => match path.find('/') {
            Some(i) => path[..i].to_string(),
            None => String::new(),
        },
    }
}
