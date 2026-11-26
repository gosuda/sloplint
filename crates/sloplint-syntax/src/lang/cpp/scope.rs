//! Cpp declaration scope: macro-local names and statement scopes.

use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};
use crate::lang::c_family::{self, CPP_DECLARATORS, word};

fn find_decl_from_cx<'t>(cx: &Cx<'t, '_>) -> Option<Node<'t>> {
    for anc in cx.ancestors() {
        if anc.kind() == "declaration" || anc.kind() == "field_declaration" {
            return Some(anc);
        }
        if !matches!(
            anc.kind(),
            "init_declarator"
                | "pointer_declarator"
                | "reference_declarator"
                | "array_declarator"
                | "parenthesized_declarator"
                | "attributed_declarator"
                | "structured_binding_declarator"
        ) {
            return None;
        }
    }
    None
}

fn is_macro_local_from_cx(decl: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let mut seen = false;
    let mut step = 0;
    let mut body = None;
    let mut pending_list = false;
    for anc in cx.ancestors() {
        if !seen {
            if anc.id() == decl.id() {
                seen = true;
            }
            continue;
        }
        if pending_list {
            if anc.kind() != "macro_braced_declaration" {
                return false;
            }
            pending_list = false;
            step += 1;
            continue;
        }
        if step == 0 {
            if anc.kind() != "field_declaration_list" {
                return false;
            }
            body = Some(anc);
        } else if step == 1 {
            if anc.kind() != "macro_braced_declaration" {
                return false;
            }
        } else {
            if anc.kind() == "compound_statement" {
                break;
            }
            if anc.kind() == "field_declaration_list" {
                pending_list = true;
                continue;
            }
        }
        step += 1;
    }
    if !seen || body.is_none() {
        return false;
    }
    if pending_list {
        return false;
    }
    !named_children(body.unwrap()).into_iter().any(|child| {
        matches!(
            child.kind(),
            "access_specifier" | "qt_section_label" | "macro_access_label"
        )
    })
}

pub(crate) fn push_cpp_piece_name<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(decl) = find_decl_from_cx(cx) else {
        return;
    };
    if decl.kind() == "field_declaration" {
        if !is_macro_local_from_cx(decl, cx) {
            return;
        }
    } else if decl.kind() != "declaration" {
        return;
    }
    if word(
        cx,
        decl,
        &["type_qualifier", "storage_class_specifier"],
        &["const", "constexpr", "extern"],
    ) {
        return;
    }
    let (all, _) = c_family::decl_info(cx, decl, CPP_DECLARATORS.wrap);
    let want = pos_of(n);
    for (name, pos) in all {
        if pos == want {
            out.push((name, pos));
            return;
        }
    }
}

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
