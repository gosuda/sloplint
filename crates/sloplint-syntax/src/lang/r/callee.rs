use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, first_named_child, text, trimmed};

pub(crate) fn callee_parts<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> (&'s str, Option<&'s str>) {
    let full = trimmed(cx, callee);
    let mut current = callee;
    let mut member = None;
    let mut terminal = None;
    let mut simple = true;
    let mut dynamic = false;
    loop {
        match current.kind() {
            "identifier" => {
                terminal = Some(text(cx, current));
                break;
            }
            "extract_operator" | "namespace_operator" => {
                let Some(lhs) = child_by_field(current, "lhs") else {
                    simple = false;
                    break;
                };
                if member.is_none() {
                    member = child_by_field(current, "rhs").map(|name| trimmed(cx, name));
                }
                current = lhs;
            }
            "call" | "subset" | "subset2" => {
                dynamic = true;
                simple = false;
                if let Some(function) = child_by_field(current, "function") {
                    current = function;
                } else {
                    break;
                }
            }
            "parenthesized_expression" => {
                simple = false;
                if let Some(inner) = first_named_child(current) {
                    current = inner;
                } else {
                    break;
                }
            }
            _ => {
                simple = false;
                break;
            }
        }
    }
    if dynamic {
        (member.or(terminal).unwrap_or(""), None)
    } else if simple {
        (full, terminal)
    } else {
        (full, None)
    }
}
