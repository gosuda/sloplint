use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, find_named_child, pos_of, rfind_named_child};

pub(crate) fn redundant_else(n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Pos> {
    if n.kind() != "if_statement" {
        return None;
    }
    // The first alternative is an elif when the branches form a decision chain.
    let alternative = child_by_field(n, "alternative").filter(|a| a.kind() == "else_clause")?;
    let consequence = child_by_field(n, "consequence")?;
    let last = rfind_named_child(consequence, |child| child.kind() != "comment")?;
    if !matches!(
        last.kind(),
        "return_statement" | "raise_statement" | "break_statement" | "continue_statement"
    ) {
        return None;
    }
    let body = child_by_field(alternative, "body")?;
    find_named_child(body, |child| child.kind() != "comment")?;
    Some(pos_of(alternative))
}
