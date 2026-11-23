use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, find_named_child, pos_of, rfind_named_child};

pub(crate) fn redundant_else(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if n.kind() != "if_statement"
        || child_by_field(n, "initializer").is_some()
        || !cx.parent().is_some_and(|p| p.kind() == "statement_list")
    {
        return None;
    }
    let alternative = child_by_field(n, "alternative").filter(|a| a.kind() == "block")?;
    let consequence = child_by_field(n, "consequence")?;
    let consequence = statements(consequence)?;
    let last = rfind_named_child(consequence, executable)?;
    if !matches!(
        last.kind(),
        "return_statement" | "break_statement" | "continue_statement"
    ) {
        return None;
    }
    let alternative = statements(alternative)?;
    let mut cursor = alternative.walk();
    if alternative
        .named_children(&mut cursor)
        .any(|stmt| stmt.kind().ends_with("_declaration") || stmt.kind() == "labeled_statement")
    {
        return None;
    }
    find_named_child(alternative, executable)?;
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .find(|child| child.kind() == "else")
        .map(pos_of)
}

fn statements(block: Node<'_>) -> Option<Node<'_>> {
    find_named_child(block, |child| child.kind() == "statement_list")
}

fn executable(n: Node<'_>) -> bool {
    !matches!(n.kind(), "comment" | "empty_statement")
}
