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
    ) || (consequence.kind() != "statement_block" && !separated(consequence))
    {
        return None;
    }
    let last_alternative = if body.kind() == "statement_block" {
        let mut cursor = body.walk();
        if body.named_children(&mut cursor).any(needs_scope) {
            return None;
        }
        rfind_named_child(body, executable)?
    } else {
        body
    };
    if !separated(last_alternative) {
        return None;
    }
    Some(pos_of(alternative))
}

// Removing `else` or its braces must not remove an ASI boundary. A retained
// block or an explicit semicolon keeps adjacent `(`, `[`, and template tokens
// from joining the previous expression after the suggested rewrite.
// Follow terminal statement slots, never loop headers or expression braces.
fn separated(mut n: Node<'_>) -> bool {
    loop {
        let next = match n.kind() {
            "statement_block" | "switch_body" | "empty_statement" => return true,
            "expression_statement"
            | "return_statement"
            | "throw_statement"
            | "break_statement"
            | "continue_statement"
            | "debugger_statement"
            | "do_statement" => return explicit_semicolon(n),
            "if_statement" => {
                child_by_field(n, "alternative").or_else(|| child_by_field(n, "consequence"))
            }
            "else_clause" => find_named_child(n, |child| child.kind() != "comment"),
            "for_statement" | "for_in_statement" | "while_statement" | "with_statement"
            | "labeled_statement" | "switch_statement" | "catch_clause" | "finally_clause" => {
                child_by_field(n, "body")
            }
            "try_statement" => {
                child_by_field(n, "finalizer").or_else(|| child_by_field(n, "handler"))
            }
            _ => return false,
        };
        let Some(next) = next else {
            return false;
        };
        n = next;
    }
}

fn explicit_semicolon(n: Node<'_>) -> bool {
    (0..n.child_count())
        .rev()
        .filter_map(|index| n.child(index))
        .find(|child| child.kind() != "comment")
        .is_some_and(|child| child.kind() == ";")
}

fn executable(n: Node<'_>) -> bool {
    !matches!(n.kind(), "comment" | "empty_statement")
}

fn needs_scope(n: Node<'_>) -> bool {
    n.kind().ends_with("_declaration")
        || matches!(n.kind(), "internal_module" | "labeled_statement")
}
