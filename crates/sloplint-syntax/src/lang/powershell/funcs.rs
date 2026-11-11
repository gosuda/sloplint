use sloplint_core::facts::{Param, PrimitiveKind, TypeDef, Visibility};
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, FunctionHead, TypeHead, any_named_child, first_of_kind, named_children,
    only_named_child_matching, pos_of, preceding_comments, text, trimmed,
};

fn node_id(n: Node<'_>) -> usize {
    n.id()
}

pub(crate) fn ps_var_name(t: &str) -> &str {
    let s = t.strip_prefix('$').unwrap_or(t);
    let s = s
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .unwrap_or(s);
    match s.rfind(':') {
        Some(at) => &s[at + 1..],
        None => s,
    }
}

fn ps_param_ty<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<&'s str> {
    if let Some(found) = first_of_kind(p, &["type_literal"]) {
        return Some(text(cx, found));
    }
    let var = first_of_kind(p, &["variable"])?;
    let full = text(cx, p);
    let end = var
        .start_byte()
        .saturating_sub(p.start_byte())
        .min(full.len());
    let before = &full[..end];
    let close = before.rfind(']')?;
    let open = before[..close].rfind('[')?;
    Some(&before[open..=close])
}

fn ps_typed_primitive(ty: &str) -> PrimitiveKind {
    let inner = ty
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(ty);
    if inner.eq_ignore_ascii_case("bool") || inner.eq_ignore_ascii_case("switch") {
        return PrimitiveKind::Bool;
    }
    if inner.eq_ignore_ascii_case("string") {
        return PrimitiveKind::Str;
    }
    if inner.eq_ignore_ascii_case("int") {
        return PrimitiveKind::Int;
    }
    PrimitiveKind::Other
}

fn ps_default_primitive(bool_default: bool) -> PrimitiveKind {
    if bool_default {
        PrimitiveKind::Bool
    } else {
        PrimitiveKind::Other
    }
}

fn push_ps_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>, out: &mut Vec<Param<'s>>) {
    let Some(var) = first_of_kind(p, &["variable"]) else {
        return;
    };
    let name = ps_var_name(text(cx, var));
    let ty = ps_param_ty(cx, p);
    let default = first_of_kind(p, &["script_parameter_default"]);
    let bool_default = default.is_some_and(|d| {
        let t = trimmed(cx, d);
        let v = t.strip_prefix('=').map_or(t, |s| s.trim());
        v.eq_ignore_ascii_case("$true") || v.eq_ignore_ascii_case("$false")
    });
    let primitive_kind = match ty {
        Some(t) => ps_typed_primitive(t),
        None => ps_default_primitive(bool_default),
    };
    out.push(Param {
        name,
        ty,
        primitive_kind,
        has_default: default.is_some(),
        bool_default,
        keyword_only: false,
        is_variadic: false,
    });
}

fn collect_script_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        if child.kind() == "script_parameter" || child.kind() == "class_method_parameter" {
            push_ps_param(cx, child, &mut *out);
        }
    }
}

fn collect_block_params<'s>(cx: &Cx<'_, 's>, block: Node<'_>, out: &mut Vec<Param<'s>>) {
    for sub in named_children(block) {
        if sub.kind() == "param_block"
            && let Some(list) = first_of_kind(sub, &["parameter_list"])
        {
            collect_script_params(cx, list, out);
        }
    }
}

fn ps_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        match child.kind() {
            "script_parameter" => push_ps_param(cx, child, &mut out),
            "function_parameter_declaration" => {
                if let Some(list) = first_of_kind(child, &["parameter_list"]) {
                    collect_script_params(cx, list, &mut out);
                }
            }
            "script_block" => collect_block_params(cx, child, &mut out),
            "function_name" | "simple_name" | "comment" | "class_attribute" => {}
            _ => {
                if !matches!(
                    child.kind(),
                    "function_statement" | "class_method_definition" | "script_block_expression"
                ) {
                    collect_script_params(cx, child, &mut out);
                }
            }
        }
    }
    out
}

fn enclosing_class<'s>(cx: &Cx<'_, 's>) -> &'s str {
    for parent in cx.ancestors() {
        if parent.kind() == "class_statement" {
            return first_of_kind(parent, &["simple_name"])
                .map(|v| text(cx, v))
                .unwrap_or("");
        }
    }
    ""
}

pub(crate) fn single_named(n: Node<'_>) -> Option<Node<'_>> {
    only_named_child_matching(n, |c| c.kind() != "comment")
}

fn ps_sole_call(n: Node<'_>) -> Option<usize> {
    let body = first_of_kind(n, &["script_block"])?;
    let inner = first_of_kind(body, &["script_block_body"]).unwrap_or(body);
    let list = first_of_kind(inner, &["statement_list"]).unwrap_or(inner);
    let mut stmt = single_named(list)?;
    loop {
        let kind = stmt.kind();
        if kind != "pipeline" && kind != "pipeline_chain" {
            break;
        }
        stmt = single_named(stmt)?;
    }
    (stmt.kind() == "command" || stmt.kind() == "invokation_expression").then(|| node_id(stmt))
}

fn is_nested_function(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors()
        .any(|a| a.kind() == "function_statement" || a.kind() == "class_method_definition")
}

fn ps_visibility(cx: &Cx<'_, '_>, n: Node<'_>, kind: &str) -> Visibility {
    if is_nested_function(cx) {
        return Visibility::Private;
    }
    if kind == "function_statement" {
        return Visibility::Exported;
    }
    let hidden = any_named_child(n, |c| {
        c.kind() == "class_attribute" && trimmed(cx, c) == "hidden"
    });
    if hidden {
        Visibility::Private
    } else {
        Visibility::Exported
    }
}

pub(crate) fn ps_function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let kind = n.kind();
    if kind != "function_statement" && kind != "class_method_definition" {
        return None;
    }
    let want = if kind == "function_statement" {
        "function_name"
    } else {
        "simple_name"
    };
    let name_node = first_of_kind(n, &[want])?;
    let name = text(cx, name_node);
    let return_type = if kind == "class_method_definition" {
        first_of_kind(n, &["type_literal"]).map(|t| text(cx, t))
    } else {
        None
    };
    let params = ps_params(cx, n);
    let mut missing: Vec<&'s str> = Vec::new();
    for param in &params {
        if param.ty.is_none() {
            missing.push(param.name);
        }
    }
    let ctor = name == enclosing_class(cx);
    if kind == "class_method_definition" && return_type.is_none() && !ctor {
        missing.push("return");
    }
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos: pos_of(name_node),
        visibility: ps_visibility(cx, n, kind),
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        receiver: None,
        params,
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: ps_sole_call(n),
    })
}

pub(crate) fn ps_script_block_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() != "script_block_expression" {
        return None;
    }
    let mut params = Vec::new();
    for child in named_children(n) {
        let block = if child.kind() == "param_block" {
            Some(child)
        } else if child.kind() == "script_block" {
            first_of_kind(child, &["param_block"])
        } else {
            None
        };
        if let Some(block) = block
            && let Some(list) = first_of_kind(block, &["parameter_list"])
        {
            collect_script_params(cx, list, &mut params);
        }
    }
    if params.is_empty() {
        return None;
    }
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        receiver: None,
        params,
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: ps_sole_call(n),
    })
}

pub(crate) fn ps_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let kind = n.kind();
    if kind != "class_statement" && kind != "enum_statement" {
        return None;
    }
    let name_node = first_of_kind(n, &["simple_name"])?;
    let container = if kind == "class_statement" {
        Some(Container::Class {
            exported: true,
            implements: false,
        })
    } else {
        None
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: Visibility::Exported,
            abstraction: false,
        }),
        impls: Vec::new(),
        container,
    })
}
