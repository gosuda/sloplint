use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, find_named_child, pos_of, rfind_named_child};

pub(crate) fn redundant_else(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if n.kind() != "if_statement"
        || !cx
            .parent()
            .is_some_and(|p| matches!(p.kind(), "statement_block" | "program"))
    {
        return None;
    }
    let alternative = child_by_field(n, "alternative")?;
    let body = find_named_child(alternative, executable)?;
    if body.kind() == "if_statement" || needs_scope(body) {
        return None;
    }
    let consequence = child_by_field(n, "consequence")?;
    let last = if consequence.kind() == "statement_block" {
        rfind_named_child(consequence, executable)?
    } else {
        consequence
    };
    if !matches!(
        last.kind(),
        "return_statement" | "throw_statement" | "break_statement" | "continue_statement"
    ) {
        return None;
    }
    if body.kind() == "statement_block" {
        let mut cursor = body.walk();
        if body.named_children(&mut cursor).any(needs_scope) {
            return None;
        }
        find_named_child(body, executable)?;
    }
    Some(pos_of(alternative))
}

fn executable(n: Node<'_>) -> bool {
    !matches!(n.kind(), "comment" | "empty_statement")
}

fn needs_scope(n: Node<'_>) -> bool {
    n.kind().ends_with("_declaration")
        || matches!(n.kind(), "internal_module" | "labeled_statement")
}
