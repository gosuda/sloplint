use tree_sitter::Node;

use crate::hooks::{Cx, named_children, text};

pub(crate) fn shadow_decl_kind(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "if_expression"
            | "else_clause"
            | "for_statement"
            | "for_expression"
            | "while_statement"
            | "while_expression"
            | "switch_case"
            | "catch_expression"
    )
}

fn push_payload_idents<'s>(cx: &Cx<'_, 's>, payload: Node<'_>, out: &mut Vec<&'s str>) {
    for id in named_children(payload) {
        if id.kind() == "identifier" {
            out.push(text(cx, id));
        }
    }
}

pub(crate) fn shadow_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    for child in named_children(n) {
        if child.kind() == "payload" {
            push_payload_idents(cx, child, out);
        }
    }
}
