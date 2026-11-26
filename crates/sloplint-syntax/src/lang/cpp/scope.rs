//! C++ statement scopes and range bindings.

use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, named_children, text};
use crate::lang::c_family::{self, CPP_DECLARATORS};

pub(crate) fn statement_scope(depth: usize, node: Node<'_>) -> usize {
    if node
        .parent()
        .is_some_and(|parent| parent.kind() == "case_statement")
    {
        return depth.saturating_sub(2);
    }
    let mut levels = 1;
    let mut current = node.parent();
    while let Some(outer) = current {
        if matches!(
            outer.kind(),
            "if_statement" | "while_statement" | "switch_statement" | "for_range_loop"
        ) {
            return depth.saturating_sub(levels);
        }
        if !matches!(outer.kind(), "condition_clause" | "init_statement") {
            break;
        }
        levels += 1;
        current = outer.parent();
    }
    depth.saturating_sub(1)
}

pub(crate) fn range_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(declarator) = child_by_field(n, "declarator") else {
        return;
    };
    let found = c_family::leaf(declarator, CPP_DECLARATORS.wrap);
    if found.kind() == "identifier" {
        out.push(text(cx, found));
    } else if found.kind() == "structured_binding_declarator" {
        for id in named_children(found) {
            if id.kind() == "identifier" {
                out.push(text(cx, id));
            }
        }
    }
}
