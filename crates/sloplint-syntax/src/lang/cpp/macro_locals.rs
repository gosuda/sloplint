use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, DeclPieces, child_by_field, named_children, pos_of, text};
use crate::lang::c_family::{self, CPP_DECLARATORS, word};

pub(crate) fn find_decl_from_cx<'t>(cx: &Cx<'t, '_>) -> Option<Node<'t>> {
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

pub(crate) fn is_macro_local_from_cx(decl: Node<'_>, cx: &Cx<'_, '_>) -> bool {
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
    let body = body.unwrap();
    cx.members.macro_body_is_local(body, || {
        let mut labels = Vec::new();
        for child in named_children(body) {
            match child.kind() {
                "access_specifier" | "qt_section_label" => return false,
                "macro_access_label" => {
                    let Some(name) = child_by_field(child, "name") else {
                        return false;
                    };
                    labels.push(text(cx, name));
                }
                _ => {}
            }
        }
        if labels.is_empty() {
            return true;
        }
        let jumped = goto_target_names(body, cx);
        labels.into_iter().all(|label| jumped.contains(&label))
    })
}

fn goto_target_names<'s>(root: Node<'_>, cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "goto_statement" {
            if let Some(label) = child_by_field(node, "label") {
                out.push(text(cx, label));
            }
            continue;
        }
        stack.extend(named_children(node));
    }
    out
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
    let want = pos_of(n);
    let found = cx.members.decl_pieces(
        decl,
        || {
            if word(
                cx,
                decl,
                &["type_qualifier", "storage_class_specifier"],
                &["const", "constexpr", "extern"],
            ) {
                return DeclPieces {
                    skipped: true,
                    names: Vec::new(),
                };
            }
            let (all, _) = c_family::decl_info(cx, decl, CPP_DECLARATORS.wrap);
            let base = cx.src.as_ptr() as usize;
            let end = base.wrapping_add(cx.src.len());
            DeclPieces {
                skipped: false,
                names: all
                    .into_iter()
                    .filter_map(|(name, pos)| {
                        let start = name.as_ptr() as usize;
                        (start >= base && start + name.len() <= end)
                            .then(|| (start - base, name.len(), pos))
                    })
                    .collect(),
            }
        },
        |cached| {
            if cached.skipped {
                return None;
            }
            let at = cached
                .names
                .partition_point(|(_, _, pos)| pos.byte < want.byte);
            cached
                .names
                .get(at)
                .filter(|(_, _, pos)| *pos == want)
                .and_then(|(from, len, pos)| {
                    cx.src
                        .get(*from..from.checked_add(*len)?)
                        .map(|name| (name, *pos))
                })
        },
    );
    if let Some((name, pos)) = found {
        out.push((name, pos));
    }
}
