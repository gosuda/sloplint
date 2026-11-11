use sloplint_core::facts::{Param, Pos, PrimitiveKind, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::{TEST_ATTRS, attr_hit, has_mod};
use crate::hooks::{
    Container, Cx, FunctionHead, any_named_child, attributed_region, child_by_field,
    first_named_child, first_named_child_matching, named_children, only_named_child_matching,
    pos_of, preceding_comments, text, trimmed,
};

pub(crate) fn primitive_of(ty: &str) -> PrimitiveKind {
    let stepped = ty.trim().strip_suffix('?').unwrap_or(ty.trim()).trim();
    if stepped.contains('<') || stepped.contains(' ') {
        return PrimitiveKind::Other;
    }
    match stepped.rsplit('.').next().unwrap_or(stepped) {
        "string" | "String" => PrimitiveKind::Str,
        "bool" | "Boolean" => PrimitiveKind::Bool,
        "int" | "long" | "short" | "byte" | "sbyte" | "uint" | "ulong" | "ushort" | "char"
        | "Int16" | "Int32" | "Int64" | "UInt16" | "UInt32" | "UInt64" | "Byte" | "SByte"
        | "Char" => PrimitiveKind::Int,
        "float" | "double" | "decimal" | "Single" | "Double" | "Decimal" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn visibility_of(cx: &Cx<'_, '_>, n: Node<'_>, kind: &str, in_trait: bool) -> Visibility {
    if kind == "local_function_statement" || has_mod(cx, n, "private") {
        Visibility::Private
    } else if has_mod(cx, n, "public") || has_mod(cx, n, "protected") || in_trait {
        Visibility::Exported
    } else if has_mod(cx, n, "internal") {
        Visibility::Restricted
    } else {
        Visibility::Private
    }
}

fn push_one_param<'s>(cx: &Cx<'_, 's>, c: Node<'_>, out: &mut Vec<Param<'s>>) {
    let Some(nm) = child_by_field(c, "name") else {
        return;
    };
    let mut ty_node = None;
    let mut default_node = None;
    let mut before_name = true;
    for child in named_children(c) {
        if child.id() == nm.id() {
            before_name = false;
            continue;
        }
        if child.kind() == "modifier" || child.kind() == "attribute_list" {
            continue;
        }
        if before_name {
            ty_node = Some(child);
        } else if default_node.is_none() {
            default_node = Some(child);
        }
    }
    let ty = ty_node.map(|t| text(cx, t));
    let bool_default = default_node.is_some_and(|d| {
        d.kind() == "boolean_literal" && matches!(trimmed(cx, d), "true" | "false")
    });
    out.push(Param {
        name: text(cx, nm),
        ty,
        primitive_kind: primitive_of(ty.unwrap_or("")),
        has_default: default_node.is_some(),
        bool_default,
        keyword_only: false,
        is_variadic: any_named_child(c, |d| d.kind() == "modifier" && trimmed(cx, d) == "params"),
    });
}

pub(crate) fn params_from_list<'s>(cx: &Cx<'_, 's>, pl: Node<'_>, out: &mut Vec<Param<'s>>) {
    for c in named_children(pl) {
        if c.kind() == "parameter" {
            push_one_param(cx, c, out);
        }
    }
    if let Some(nm) = child_by_field(pl, "name") {
        let ty = child_by_field(pl, "type").map(|t| text(cx, t));
        out.push(Param {
            name: text(cx, nm),
            ty,
            primitive_kind: primitive_of(ty.unwrap_or("")),
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: true,
        });
    }
}

fn unwrap_await(mut n: Node<'_>) -> Node<'_> {
    while n.kind() == "await_expression" {
        let Some(next) = first_named_child(n) else {
            break;
        };
        n = next;
    }
    n
}

fn sole_call_of(body: Node<'_>) -> Option<usize> {
    if body.kind() == "arrow_expression_clause" {
        let call = unwrap_await(first_named_child(body)?);
        return (call.kind() == "invocation_expression").then_some(call.id());
    }
    let stmt = only_named_child_matching(body, |c| c.kind() != "comment")?;
    if stmt.kind() != "return_statement" && stmt.kind() != "expression_statement" {
        return None;
    }
    let call = unwrap_await(first_named_child(stmt)?);
    (call.kind() == "invocation_expression").then_some(call.id())
}

fn head_visibility(cx: &Cx<'_, '_>, n: Node<'_>, kind: &str) -> (Visibility, bool) {
    let inner = cx.innermost_container();
    let in_trait = matches!(inner, Some(Container::Trait { .. }));
    let mut visibility = visibility_of(cx, n, kind, in_trait);
    if let Some(Container::Class {
        exported: false, ..
    }) = inner
    {
        visibility = Visibility::Private;
    }
    (visibility, in_trait)
}

fn accessor_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    for anc in cx.ancestors() {
        if matches!(
            anc.kind(),
            "property_declaration"
                | "indexer_declaration"
                | "event_declaration"
                | "event_field_declaration"
        ) {
            if let Some(nm) = child_by_field(anc, "name") {
                return text(cx, nm);
            }
            return "";
        }
    }
    child_by_field(n, "name").map(|m| text(cx, m)).unwrap_or("")
}

fn extra_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "accessor_declaration" => Some(accessor_name(cx, n)),
        "destructor_declaration" => child_by_field(n, "name").map(|m| text(cx, m)),
        _ => Some(""),
    }
}

fn head_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kind: &str) -> Option<(&'s str, Pos)> {
    if matches!(
        kind,
        "accessor_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration"
            | "destructor_declaration"
    ) {
        return extra_name(cx, n).map(|nm| (nm, pos_of(n)));
    }
    child_by_field(n, "name").map(|m| (text(cx, m), pos_of(m)))
}

fn accessor_visibility(cx: &Cx<'_, '_>, current: Visibility, in_trait: bool) -> Visibility {
    if current != Visibility::Private || in_trait {
        return current;
    }
    for anc in cx.ancestors() {
        if matches!(
            anc.kind(),
            "property_declaration"
                | "indexer_declaration"
                | "event_declaration"
                | "event_field_declaration"
        ) {
            if has_mod(cx, anc, "public") || has_mod(cx, anc, "protected") {
                return Visibility::Exported;
            }
            if has_mod(cx, anc, "internal") {
                return Visibility::Restricted;
            }
            return Visibility::Private;
        }
    }
    current
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let kind = n.kind();
    let is_extra = matches!(
        kind,
        "accessor_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration"
            | "destructor_declaration"
    );
    if kind != "method_declaration"
        && kind != "constructor_declaration"
        && kind != "local_function_statement"
        && !is_extra
    {
        return None;
    }
    let sole_call = match child_by_field(n, "body") {
        Some(b) if b.kind() == "block" || b.kind() == "arrow_expression_clause" => sole_call_of(b),
        Some(_) => return None,
        None => None,
    };
    let (name, name_pos) = head_name(cx, n, kind)?;
    let (visibility, in_trait) = head_visibility(cx, n, kind);
    let visibility = if kind == "accessor_declaration" {
        accessor_visibility(cx, visibility, in_trait)
    } else {
        visibility
    };
    let tested = cx.inside_test_module() || attr_hit(cx, n, TEST_ATTRS);
    let mut params = Vec::new();
    if let Some(pl) = child_by_field(n, "parameters") {
        params_from_list(cx, pl, &mut params);
    }
    let return_type = if kind == "constructor_declaration" {
        None
    } else if kind == "local_function_statement"
        || kind == "operator_declaration"
        || kind == "conversion_operator_declaration"
    {
        child_by_field(n, "type").map(|t| text(cx, t))
    } else {
        child_by_field(n, "returns").map(|t| text(cx, t))
    };
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos,
        visibility,
        signature_forced: has_mod(cx, n, "override"),
        entry_point: kind == "method_declaration" && name == "Main",
        in_test: tested,
        is_overload: false,
        is_deprecated: attr_hit(cx, n, &["Obsolete"]),
        is_decorated: any_named_child(n, |c| c.kind() == "attribute_list"),
        is_trait_item: in_trait,
        region: tested.then(|| {
            attributed_region(
                n,
                first_named_child_matching(n, &["attribute_list"]),
                CodeRole::Test,
            )
        }),
        receiver: None,
        params,
        return_type,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call,
    })
}

fn closure_sole_call(body: Node<'_>) -> Option<usize> {
    if body.kind() == "invocation_expression" {
        return Some(body.id());
    }
    sole_call_of(body)
}

fn closure_tested(cx: &Cx<'_, '_>) -> bool {
    if cx.inside_test_module() {
        return true;
    }
    cx.ancestors().any(|a| {
        matches!(
            a.kind(),
            "method_declaration" | "constructor_declaration" | "local_function_statement"
        ) && attr_hit(cx, a, TEST_ATTRS)
    })
}
fn closure_param<'s>(cx: &Cx<'_, 's>, pl: Node<'_>, out: &mut Vec<Param<'s>>) {
    if pl.kind() == "parameter_list" {
        params_from_list(cx, pl, out);
        return;
    }
    if pl.kind() == "implicit_parameter" {
        out.push(Param {
            name: text(cx, pl),
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        });
        return;
    }
    let Some(id) = child_by_field(pl, "name").or_else(|| first_named_child(pl)) else {
        return;
    };
    out.push(Param {
        name: text(cx, id),
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    });
}

fn closure_body(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "body").or_else(|| first_named_child_matching(n, &["block"]))
}

pub(crate) fn closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() != "lambda_expression" && n.kind() != "anonymous_method_expression" {
        return None;
    }
    let mut params = Vec::new();
    if let Some(pl) = child_by_field(n, "parameters") {
        closure_param(cx, pl, &mut params);
    }
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: closure_tested(cx),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: closure_body(n).and_then(closure_sole_call),
    })
}
