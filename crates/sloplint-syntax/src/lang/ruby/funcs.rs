use sloplint_core::facts::{Param, Pos, PrimitiveKind};
use tree_sitter::Node;

use super::field_text;
use crate::hooks::{
    Cx, FileSetKind, child_by_field, first_named_child, first_named_child_matching, first_of_kind,
    has_child_kind, last_segment, named_children, only_named_child_matching, pos_of, strip_quotes,
    text, trimmed,
};

pub(crate) fn node_id(n: Node<'_>) -> usize {
    n.id()
}
pub(crate) fn bare_marker_call<'s>(cx: &Cx<'_, 's>, sib: Node<'_>) -> Option<bool> {
    let method = field_text(cx, sib, "method")?;
    if child_by_field(sib, "receiver").is_some() {
        return None;
    }
    if let Some(list) = first_of_kind(sib, &["argument_list"])
        && list.named_child_count() != 0
    {
        return None;
    }
    match method {
        "private" | "module_function" => Some(false),
        "protected" | "public" => Some(true),
        _ => None,
    }
}
pub(crate) fn wrapped_by_marker<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent_of(n) else {
        return false;
    };
    if parent.kind() != "argument_list" {
        return false;
    }
    let Some(grand) = cx.parent_of(parent) else {
        return false;
    };
    if grand.kind() != "call" {
        return false;
    }
    field_text(cx, grand, "method").is_some_and(|m| matches!(m, "private" | "module_function"))
}
pub(crate) fn ruby_visibility_marker(cx: &Cx<'_, '_>, sib: Node<'_>) -> Option<bool> {
    if sib.kind() == "identifier" {
        return match text(cx, sib) {
            "private" => Some(false),
            "protected" | "public" => Some(true),
            _ => None,
        };
    }
    if sib.kind() == "call" {
        return bare_marker_call(cx, sib);
    }
    None
}
pub(crate) fn ruby_public<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    if n.kind() == "singleton_method" {
        return true;
    }
    if wrapped_by_marker(cx, n) {
        return false;
    }
    let name = child_by_field(n, "name").map(|v| text(cx, v)).unwrap_or("");
    if explicit_private_names(cx).contains(&name) {
        return false;
    }
    let Some(container) = cx.parent_of(n) else {
        return true;
    };
    cx.members
        .state_before(container, n, |sib| ruby_visibility_marker(cx, sib))
        .unwrap_or(true)
}
pub(crate) fn explicit_private_names<'x, 's>(cx: &'x Cx<'_, 's>) -> &'x [&'s str] {
    cx.file_names(FileSetKind::Member, || {
        let mut out = Vec::new();
        let Some(root) = cx.root() else {
            return out;
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "call" {
                collect_marker_names(cx, node, &mut out);
            }
            stack.extend(named_children(node));
        }
        out
    })
}
fn collect_marker_names<'s>(cx: &Cx<'_, 's>, node: Node<'_>, out: &mut Vec<&'s str>) {
    if child_by_field(node, "receiver").is_some() {
        return;
    }
    if field_text(cx, node, "method") != Some("private") {
        return;
    }
    let Some(list) = first_of_kind(node, &["argument_list"]) else {
        return;
    };
    for arg in named_children(list) {
        let name = match arg.kind() {
            "simple_symbol" => text(cx, arg).strip_prefix(':').unwrap_or(""),
            "string" => strip_quotes(text(cx, arg)),
            "identifier" | "constant" => text(cx, arg),
            _ => continue,
        };
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
}
pub(crate) fn is_protocol_method(name: &str) -> bool {
    matches!(
        name,
        "initialize"
            | "to_s"
            | "inspect"
            | "hash"
            | "=="
            | "method_missing"
            | "respond_to_missing?"
    )
}
pub(crate) fn superclass_name<'s>(cx: &Cx<'_, 's>, class: Node<'_>) -> Option<(&'s str, Pos)> {
    let sup = child_by_field(class, "superclass")?;
    let base = first_named_child(sup)?;
    Some((last_segment(text(cx, base), &["::"]), pos_of(sup)))
}
pub(crate) fn ruby_in_test_class<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    let mut current = cx.parent_of(n);
    while let Some(parent) = current {
        if parent.kind() == "class" {
            return superclass_name(cx, parent)
                .is_some_and(|(name, _)| matches!(name, "Test" | "TestCase"));
        }
        current = cx.parent_of(parent);
    }
    false
}
pub(crate) fn push_plain_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, out: &mut Vec<Param<'s>>) {
    let name = field_text(cx, child, "name").unwrap_or_else(|| text(cx, child));
    out.push(Param {
        name,
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    });
}
fn push_optional_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, out: &mut Vec<Param<'s>>) {
    push_named_default_param(cx, child, out, false);
}
fn push_keyword_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, out: &mut Vec<Param<'s>>) {
    push_named_default_param(cx, child, out, true);
}
fn push_named_default_param<'s>(
    cx: &Cx<'_, 's>,
    child: Node<'_>,
    out: &mut Vec<Param<'s>>,
    keyword_only: bool,
) {
    let name = field_text(cx, child, "name").unwrap_or("");
    let value = child_by_field(child, "value");
    let flag = value.is_some_and(|v| {
        let default = trimmed(cx, v);
        default == "true" || default == "false"
    });
    out.push(Param {
        name,
        ty: None,
        primitive_kind: if flag {
            PrimitiveKind::Bool
        } else {
            PrimitiveKind::Other
        },
        has_default: value.is_some(),
        bool_default: flag,
        keyword_only,
        is_variadic: false,
    });
}
pub(crate) fn ruby_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let params = first_named_child_matching(n, &["method_parameters"]);
    let Some(params) = params else {
        return out;
    };
    for child in named_children(params) {
        match child.kind() {
            "optional_parameter" => {
                push_optional_param(cx, child, &mut out);
            }
            "keyword_parameter" => {
                push_keyword_param(cx, child, &mut out);
            }
            "splat_parameter" => {
                out.push(Param {
                    name: field_text(cx, child, "name").unwrap_or_else(|| text(cx, child)),
                    ty: None,
                    primitive_kind: PrimitiveKind::Other,
                    has_default: false,
                    bool_default: false,
                    keyword_only: false,
                    is_variadic: true,
                });
            }
            "hash_splat_parameter" | "block_parameter" => {}
            _ => push_plain_param(cx, child, &mut out),
        }
    }
    out
}
pub(crate) fn ruby_sole_call(n: Node<'_>) -> Option<usize> {
    let body = child_by_field(n, "body")?;
    if body.kind() == "rescue_modifier" {
        return None;
    }
    let mut stmt = if body.kind() == "body_statement" {
        only_named_child_matching(body, |c| c.kind() != "comment")?
    } else {
        body
    };
    if stmt.kind() == "return" {
        stmt = only_named_child_matching(stmt, |_| true)?;
    }
    if stmt.kind() != "call" {
        return None;
    }
    if has_child_kind(stmt, "block") || has_child_kind(stmt, "do_block") {
        return None;
    }
    Some(node_id(stmt))
}
