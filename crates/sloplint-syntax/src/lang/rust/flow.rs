use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, find_named_child, has_child_kind, pos_of, rfind_named_child,
};

pub(crate) fn redundant_else(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if n.kind() != "if_expression"
        || child_by_field(n, "condition").is_none_or(|c| c.kind() != "identifier")
    {
        return None;
    }
    let statement = cx.parent().filter(|p| p.kind() == "expression_statement")?;
    if !cx.parent_of(statement).is_some_and(|p| p.kind() == "block")
        || has_outer_attribute(statement)
        || (!has_child_kind(statement, ";") && next_code(statement).is_none())
    {
        return None;
    }
    let alternative = child_by_field(n, "alternative")?;
    let body = find_named_child(alternative, executable).filter(|b| b.kind() == "block")?;
    if !discarded_statements(body) {
        return None;
    }
    let consequence = child_by_field(n, "consequence")?;
    let last = rfind_named_child(consequence, executable)?;
    if has_outer_attribute(last) {
        return None;
    }
    let terminal = if last.kind() == "expression_statement" {
        find_named_child(last, executable)?
    } else {
        last
    };
    matches!(
        terminal.kind(),
        "return_expression" | "break_expression" | "continue_expression"
    )
    .then(|| pos_of(alternative))
}

/// A tail value or a let/item/macro declaration would change type, visibility,
/// or drop scope when the containing else block is removed.
fn discarded_statements(body: Node<'_>) -> bool {
    let mut found = false;
    let mut cursor = body.walk();
    for statement in body.named_children(&mut cursor).filter(|n| executable(*n)) {
        if statement.kind() != "expression_statement" || !has_child_kind(statement, ";") {
            return false;
        }
        let Some(expression) = find_named_child(statement, executable) else {
            return false;
        };
        if expression.kind() == "macro_invocation" {
            return false;
        }
        found = true;
    }
    found
}

fn has_outer_attribute(n: Node<'_>) -> bool {
    let mut sibling = n.prev_named_sibling();
    while let Some(previous) = sibling {
        if executable(previous) {
            return previous.kind() == "attribute_item";
        }
        sibling = previous.prev_named_sibling();
    }
    false
}

fn next_code(n: Node<'_>) -> Option<Node<'_>> {
    let mut sibling = n.next_named_sibling();
    while let Some(next) = sibling {
        if !matches!(next.kind(), "line_comment" | "block_comment") {
            return Some(next);
        }
        sibling = next.next_named_sibling();
    }
    None
}

fn executable(n: Node<'_>) -> bool {
    !matches!(
        n.kind(),
        "line_comment" | "block_comment" | "empty_statement"
    )
}
