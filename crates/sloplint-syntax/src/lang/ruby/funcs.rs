use sloplint_core::facts::{Param, Pos, PrimitiveKind};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_named_child, first_named_child_matching, first_of_kind,
    has_child_kind, last_segment, named_children, only_named_child_matching, pos_of, text, trimmed,
};

use super::field_text;

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
        "private" | "protected" | "module_function" => Some(false),
        "public" => Some(true),
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
    field_text(cx, grand, "method")
        .is_some_and(|m| matches!(m, "private" | "protected" | "module_function"))
}
pub(crate) fn ruby_visibility_marker(cx: &Cx<'_, '_>, sib: Node<'_>) -> Option<bool> {
    if sib.kind() == "identifier" {
        return match text(cx, sib) {
            "private" | "protected" => Some(false),
            "public" => Some(true),
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
    let Some(container) = cx.parent_of(n) else {
        return true;
    };
    cx.members
        .state_before(container, n, |sib| ruby_visibility_marker(cx, sib))
        .unwrap_or(true)
}
pub(crate) fn is_protocol_method(name: &str) -> bool {
    matches!(
        name,
        "initialize"
            | "to_s"
            | "to_str"
            | "to_i"
            | "to_a"
            | "to_h"
            | "inspect"
            | "hash"
            | "eql?"
            | "=="
            | "!="
            | "<=>"
            | "<"
            | ">"
            | "<="
            | ">="
            | "each"
            | "call"
            | "[]"
            | "[]="
            | "<<"
            | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "succ"
            | "next"
            | "length"
            | "size"
            | "empty?"
            | "coerce"
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
pub(crate) fn ruby_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let params = first_named_child_matching(n, &["method_parameters"]);
    let Some(params) = params else {
        return out;
    };
    for child in named_children(params) {
        match child.kind() {
            "identifier" => out.push(Param {
                name: text(cx, child),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            }),
            "optional_parameter" | "keyword_parameter" => {
                let name = field_text(cx, child, "name").unwrap_or("");
                let default = child_by_field(child, "value")
                    .map(|v| trimmed(cx, v))
                    .unwrap_or("");
                let flag = default == "true" || default == "false";
                out.push(Param {
                    name,
                    ty: None,
                    primitive_kind: if flag {
                        PrimitiveKind::Bool
                    } else {
                        PrimitiveKind::Other
                    },
                    has_default: false,
                    bool_default: flag,
                    keyword_only: false,
                    is_variadic: false,
                });
            }
            "destructured_parameter" | "forward_parameter" => {
                out.push(Param {
                    name: text(cx, child),
                    ty: None,
                    primitive_kind: PrimitiveKind::Other,
                    has_default: false,
                    bool_default: false,
                    keyword_only: false,
                    is_variadic: false,
                });
            }
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
