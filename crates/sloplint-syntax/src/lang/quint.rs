//! Quint adapter: the specification-language constructs lowered onto the
//! linter's shared concepts (see `vendor/tree-sitter-quint/SLOPLINT.md`).
//!
//! Quint is expression-oriented: `def`/`val` families (including the
//! `action`, `temporal`, `run`, and `nondet` qualifiers) are the
//! function-like units, `lambda_expression` is the closure, `if` is
//! `if_expression`, `match` carries `hole`-variant wildcard arms, and the
//! `all`/`any` blocks are the unbounded-branching constructs where the
//! linter counts nesting. State is module-level `var` declarations mutated
//! through `assignment_expression` targets (always `primed_identifier`).

use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, CallKind, FieldCopy, Import, ImportKind, ImportScope, Param, Pos,
    PrimitiveKind, ScrutineeKind, TypeDef, Visibility, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, ident_root, narrow, pos_of,
    preceding_comments, text, trimmed,
};
use crate::spec::Role;

pub(crate) static TABLE: crate::lang::Table = &[
    ("operator_definition", Role::Function),
    ("value_definition", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_expression", Role::If),
    ("match_expression", Role::Match),
    ("match_arm", Role::MatchArm),
    ("all_expression", Role::Loop),
    ("any_expression", Role::Loop),
    ("nondet_binding", Role::Decl),
    ("call_expression", Role::Call),
    ("ufcs_call_expression", Role::Call),
    ("variable_declaration", Role::Decl),
    ("constant_declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("module_import_declaration", Role::Import),
    ("named_import_declaration", Role::Import),
    ("wildcard_import_declaration", Role::Import),
    ("instance_declaration", Role::Import),
    ("anonymous_instance_declaration", Role::Import),
    ("module_export_declaration", Role::Export),
    ("named_export_declaration", Role::Export),
    ("wildcard_export_declaration", Role::Export),
    ("type_alias_declaration", Role::TypeDef),
    ("uninterpreted_type_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("documentation_comment", Role::Comment),
    ("record_literal", Role::StructLit),
];

pub(crate) static HOOKS: QuintHooks = QuintHooks;

pub(crate) struct QuintHooks;

const DOC_KINDS: &[&str] = &["documentation_comment"];
const SKIP_KINDS: &[&str] = &["comment"];
const OBJECT_FIELDS: &[&str] = &["object", "name"];

/// The plain name a definition, parameter, or reference introduces:
/// identifiers as-is, qualified and primed names by their last segment.
fn plain_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "identifier" => Some(text(cx, n)),
        "qualified_identifier" | "name_reference" | "primed_identifier" => {
            child_by_field(n, "name").and_then(|name| plain_name(cx, name))
        }
        _ => None,
    }
}

fn primitive_kind(ty: &str) -> PrimitiveKind {
    match ty.trim() {
        "int" => PrimitiveKind::Int,
        "str" => PrimitiveKind::Str,
        "bool" => PrimitiveKind::Bool,
        _ => PrimitiveKind::Other,
    }
}

fn param_of<'s>(cx: &Cx<'_, 's>, param: Node<'_>) -> Option<Param<'s>> {
    let is_annotated = param.kind() == "annotated_parameter";
    if param.kind() != "parameter" && !is_annotated {
        return None;
    }
    let name_node = child_by_field(param, "name")?;
    let name = match name_node.kind() {
        "identifier" => text(cx, name_node),
        "hole" => "_",
        _ => trimmed(cx, name_node),
    };
    let ty = is_annotated
        .then(|| child_by_field(param, "type"))
        .flatten()
        .map(|ty| trimmed(cx, ty));
    Some(Param {
        name,
        ty,
        primitive_kind: ty.map_or(PrimitiveKind::Other, primitive_kind),
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    })
}

fn field_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut cursor = n.walk();
    n.children_by_field_name("parameter", &mut cursor)
        .filter_map(|param| param_of(cx, param))
        .collect()
}

fn lambda_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut cursor = n.walk();
    n.children_by_field_name("parameter", &mut cursor)
        .map(|param| {
            let name = match param.kind() {
                "identifier" => text(cx, param),
                "hole" => "_",
                _ => trimmed(cx, param),
            };
            Param {
                name,
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            }
        })
        .collect()
}

fn base_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>, visibility: Visibility) -> FunctionHead<'s> {
    let (doc, doc_pos) = preceding_comments(cx, n, DOC_KINDS, SKIP_KINDS);
    FunctionHead {
        shadow_params: Vec::new(),
        name: "",
        name_pos: pos_of(n),
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        visibility,
        is_trait_item: false,
        region: None,
        params: Vec::new(),
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: None,
    }
}

/// A definition introduced by `nested_definition_expression` is a local of the
/// enclosing body, so only a module-level definition can be imported.
fn definition_visibility(cx: &Cx<'_, '_>, n: Node<'_>) -> Visibility {
    match cx.parent_of(n).map(|parent| parent.kind()) {
        Some("nested_definition_expression") => Visibility::Private,
        _ => Visibility::Exported,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, arg: Node<'_>) -> ArgShape<'s> {
    match arg.kind() {
        "string_literal" => {
            let raw = text(cx, arg);
            let inner = raw
                .strip_prefix('"')
                .unwrap_or(raw)
                .strip_suffix('"')
                .unwrap_or(raw);
            ArgShape::Str {
                longest_static: narrow(inner.len()),
            }
        }
        "identifier" | "name_reference" => ArgShape::Ident(text(cx, arg)),
        "list_literal" | "tuple_literal" | "record_literal" => ArgShape::Collection,
        "lambda_expression" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}

fn call_arg<'s>(cx: &Cx<'_, 's>, arg: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, arg),
        start_line: narrow(arg.start_position().row) + 1,
        end_line: narrow(arg.end_position().row) + 1,
    }
}

fn field_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    let mut cursor = n.walk();
    n.children_by_field_name("argument", &mut cursor)
        .map(|arg| call_arg(cx, arg))
        .collect()
}

fn module_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    child_by_field(n, "module").map_or("", |module| trimmed(cx, module))
}

fn push_import<'s>(
    n: Node<'_>,
    module: &'s str,
    name: &'s str,
    alias: Option<&'s str>,
    kind: ImportKind,
    is_reexport: bool,
    out: &mut Vec<Import<'s>>,
) {
    if module.is_empty() {
        return;
    }
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name,
        alias,
        kind,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport,
        external_root: Some(module),
    });
}

/// The object a record field copies one-to-one: the destination name must
/// equal the member read, and the object must be a plain name reference, so a
/// renamed, repeated or nested member reads as a transformation.
fn copy_source<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> Option<&'s str> {
    let name = child_by_field(field, "name").filter(|name| name.kind() == "identifier")?;
    let value =
        child_by_field(field, "value").filter(|value| value.kind() == "field_access_expression")?;
    let object =
        child_by_field(value, "object").filter(|object| object.kind() == "name_reference")?;
    let member = child_by_field(value, "field").filter(|field| field.kind() == "identifier")?;
    if text(cx, name) != text(cx, member) {
        return None;
    }
    plain_name(cx, object)
}

/// True when the call is the whole initializer of a `val` definition or a
/// `nondet` binding, the two named locals a call can initialize.
fn initializes_named_local(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent_of(n) else {
        return false;
    };
    match parent.kind() {
        "value_definition" => {
            child_by_field(parent, "value").is_some_and(|value| value.id() == n.id())
        }
        "nondet_binding" => {
            child_by_field(parent, "value").is_some_and(|value| value.id() == n.id())
        }
        _ => false,
    }
}

impl LangHooks for QuintHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body_field = match n.kind() {
            "operator_definition" => "body",
            "value_definition" => "value",
            _ => return None,
        };
        let name_node = child_by_field(n, "name")?;
        if !matches!(name_node.kind(), "identifier" | "qualified_identifier") {
            return None;
        }
        let mut head = base_head(cx, n, definition_visibility(cx, n));
        head.name = plain_name(cx, name_node).unwrap_or_default();
        head.name_pos = pos_of(name_node);
        head.params = field_params(cx, n);
        head.return_type = child_by_field(n, "return_type").map(|ty| trimmed(cx, ty));
        head.sole_call = child_by_field(n, body_field)
            .filter(|body| body.kind() == "call_expression")
            .map(|body| body.id());
        Some(head)
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        if n.kind() != "lambda_expression" {
            return None;
        }
        let mut head = base_head(cx, n, Visibility::Private);
        head.params = lambda_params(cx, n);
        head.sole_call = child_by_field(n, "body")
            .filter(|body| body.kind() == "call_expression")
            .map(|body| body.id());
        Some(head)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        match n.kind() {
            "variable_declaration" | "constant_declaration" => {
                if let Some(name) = child_by_field(n, "name").and_then(|name| plain_name(cx, name))
                {
                    out.push((name, pos_of(n)));
                }
            }
            "nondet_binding" => {
                if let Some(name) = child_by_field(n, "name").and_then(|name| plain_name(cx, name))
                {
                    out.push((name, pos_of(n)));
                }
            }
            _ => {}
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment_expression" {
            return;
        }
        if let Some(name) = child_by_field(n, "target").and_then(|target| plain_name(cx, target)) {
            out.push((name, pos_of(n)));
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        match n.kind() {
            "call_expression" => {
                let function = child_by_field(n, "function")?;
                let (callee, callee_root, is_simple_path) = match function.kind() {
                    "name_reference" => {
                        let name = plain_name(cx, function).unwrap_or_default();
                        (name, Some(name), true)
                    }
                    "namespace_access_expression" => {
                        let root =
                            child_by_field(function, "namespace").and_then(|ns| plain_name(cx, ns));
                        (trimmed(cx, function), root, true)
                    }
                    "field_access_expression" => {
                        let root = child_by_field(function, "object").and_then(|object| {
                            ident_root(cx, object, OBJECT_FIELDS).map(|(name, _)| name)
                        });
                        (trimmed(cx, function), root, false)
                    }
                    _ => (trimmed(cx, function), None, false),
                };
                Some(CallHead {
                    callee,
                    kind: CallKind::Plain,
                    callee_root,
                    is_simple_path,
                    initializes_binding: initializes_named_local(cx, n),
                    args: field_args(cx, n),
                })
            }
            "ufcs_call_expression" => {
                let method = child_by_field(n, "method")?;
                let root =
                    child_by_field(n, "receiver").and_then(|receiver| plain_name(cx, receiver));
                Some(CallHead {
                    callee: trimmed(cx, method),
                    kind: CallKind::Plain,
                    callee_root: root,
                    is_simple_path: true,
                    initializes_binding: initializes_named_local(cx, n),
                    args: field_args(cx, n),
                })
            }
            _ => None,
        }
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        match n.kind() {
            "module_import_declaration" => {
                let module = module_text(cx, n);
                let alias = child_by_field(n, "alias").map(|alias| trimmed(cx, alias));
                push_import(n, module, module, alias, ImportKind::Named, false, out);
            }
            "named_import_declaration" => {
                let name = child_by_field(n, "name")
                    .and_then(|name| plain_name(cx, name))
                    .unwrap_or("");
                push_import(
                    n,
                    module_text(cx, n),
                    name,
                    None,
                    ImportKind::Named,
                    false,
                    out,
                );
            }
            "wildcard_import_declaration" => {
                push_import(
                    n,
                    module_text(cx, n),
                    "*",
                    None,
                    ImportKind::Wildcard,
                    false,
                    out,
                );
            }
            "instance_declaration" => {
                let module = module_text(cx, n);
                let alias = child_by_field(n, "alias").map(|alias| trimmed(cx, alias));
                push_import(n, module, module, alias, ImportKind::Named, false, out);
            }
            "anonymous_instance_declaration" => {
                push_import(
                    n,
                    module_text(cx, n),
                    "*",
                    None,
                    ImportKind::Wildcard,
                    false,
                    out,
                );
            }
            "module_export_declaration" => {
                let module = module_text(cx, n);
                let alias = child_by_field(n, "alias").map(|alias| trimmed(cx, alias));
                push_import(n, module, module, alias, ImportKind::Named, true, out);
            }
            "named_export_declaration" => {
                let name = child_by_field(n, "name")
                    .and_then(|name| plain_name(cx, name))
                    .unwrap_or("");
                push_import(
                    n,
                    module_text(cx, n),
                    name,
                    None,
                    ImportKind::Named,
                    true,
                    out,
                );
            }
            "wildcard_export_declaration" => {
                push_import(
                    n,
                    module_text(cx, n),
                    "*",
                    None,
                    ImportKind::Wildcard,
                    true,
                    out,
                );
            }
            _ => {}
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let abstraction = match n.kind() {
            "type_alias_declaration" => false,
            "uninterpreted_type_declaration" => true,
            _ => return None,
        };
        let name = child_by_field(n, "name").and_then(|name| plain_name(cx, name))?;
        Some(TypeHead {
            def: Some(TypeDef {
                name,
                pos: pos_of(n),
                visibility: Visibility::Exported,
                abstraction,
            }),
            impls: Vec::new(),
            container: None,
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "match_arm" {
            return None;
        }
        if child_by_field(n, "variant").is_some_and(|variant| variant.kind() != "hole") {
            return None;
        }
        let parent = cx.parent()?;
        if parent.kind() != "match_expression" {
            return None;
        }
        let scrutinee = match child_by_field(parent, "value").map(|value| value.kind()) {
            Some("name_reference") | Some("identifier") => ScrutineeKind::Ident,
            Some("field_access_expression") => ScrutineeKind::Field,
            Some("namespace_access_expression") => ScrutineeKind::Path,
            _ => ScrutineeKind::Other,
        };
        let mut variants = 0u32;
        let mut cursor = parent.walk();
        for sibling in parent.children_by_field_name("arm", &mut cursor) {
            if sibling.id() == n.id() {
                continue;
            }
            if child_by_field(sibling, "variant").is_some_and(|variant| variant.kind() != "hole") {
                variants += 1;
            }
        }
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like: variants >= 2,
            scrutinee,
            variant_count: variants,
            has_nullable_ctor: false,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "record_literal" {
            return None;
        }
        let mut fields = 0u32;
        let mut source: Option<&'s str> = None;
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if !child.is_named() || child.kind() != "record_literal_field" {
                continue;
            }
            fields += 1;
            let copied = copy_source(cx, child)?;
            if source.is_some_and(|known| known != copied) {
                return None;
            }
            source = Some(copied);
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields,
            source,
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        _in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        let (kind, field) = match n.kind() {
            "variable_declaration" => (AmbientKind::GlobalMutable, "name"),
            "assignment_expression" => (AmbientKind::GlobalWrite, "target"),
            _ => return,
        };
        if let Some(name) = child_by_field(n, field).and_then(|name| plain_name(cx, name)) {
            out.push((kind, name, pos_of(n)));
        }
    }
}
