//! Java ambient facts: globals, environment reads, and static writes.

use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use super::calls::method_callee;
use super::{first_code, has_keyword};
use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};

fn static_globals<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let in_interface = cx
        .parent_of(n)
        .is_some_and(|parent| parent.kind() == "interface_body");
    if in_interface || !has_keyword(n, "static") || has_keyword(n, "final") {
        return;
    }
    for declarator in named_children(n) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = child_by_field(declarator, "name") {
            out.push((AmbientKind::GlobalMutable, text(cx, name), pos_of(name)));
        }
    }
}

fn ambient_write_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "assignment_expression" => {
            let left = child_by_field(n, "left")?;
            (left.kind() == "identifier").then_some(text(cx, left))
        }
        "update_expression" => {
            let operand = first_code(n)?;
            (operand.kind() == "identifier").then_some(text(cx, operand))
        }
        _ => None,
    }
}

fn static_field_named(cx: &Cx<'_, '_>, field: Node<'_>, name: &str) -> bool {
    if field.kind() != "field_declaration"
        || !has_keyword(field, "static")
        || has_keyword(field, "final")
    {
        return false;
    }
    named_children(field).into_iter().any(|decl| {
        decl.kind() == "variable_declarator"
            && child_by_field(decl, "name").is_some_and(|id| text(cx, id) == name)
    })
}

fn in_static_scope<'s>(cx: &Cx<'_, 's>, _n: Node<'_>, name: &str) -> bool {
    for node in cx.ancestors() {
        if matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            let found = named_children(node).into_iter().any(|child| {
                static_field_named(cx, child, name)
                    || named_children(child)
                        .into_iter()
                        .any(|sub| static_field_named(cx, sub, name))
            });
            if found {
                return true;
            }
        }
    }
    false
}
pub(crate) fn ambient_sites<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "field_declaration" => static_globals(cx, n, out),
        "method_invocation" => {
            if let Some(callee) = method_callee(cx, n).filter(|callee| *callee == "System.getenv") {
                out.push((AmbientKind::EnvRead, callee, pos_of(n)));
            }
        }
        "assignment_expression" | "update_expression" => {
            if let Some(name) = ambient_write_name(cx, n)
                && in_static_scope(cx, n, name)
            {
                out.push((AmbientKind::GlobalWrite, name, pos_of(n)));
            }
        }
        _ => {}
    }
}
