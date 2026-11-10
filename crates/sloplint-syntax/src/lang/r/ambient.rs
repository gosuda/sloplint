use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use super::{argument_nodes, operator_text, string_content};
use crate::hooks::{Cx, child_by_field, pos_of, text, trimmed};

pub(crate) fn push_global_binding<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match operator_text(cx, n) {
        "<-" | "=" | ":=" if !in_function => {
            let Some(target) = child_by_field(n, "lhs").filter(|t| t.kind() == "identifier") else {
                return;
            };
            if child_by_field(n, "rhs").is_some_and(|rhs| rhs.kind() == "function_definition") {
                return;
            }
            out.push((AmbientKind::GlobalMutable, text(cx, target), pos_of(target)));
        }
        "<<-" if in_function => {
            if let Some(target) = child_by_field(n, "lhs").filter(|t| t.kind() == "identifier") {
                out.push((AmbientKind::GlobalWrite, text(cx, target), pos_of(target)));
            }
        }
        "->>" if in_function => {
            if let Some(target) = child_by_field(n, "rhs").filter(|t| t.kind() == "identifier") {
                out.push((AmbientKind::GlobalWrite, text(cx, target), pos_of(target)));
            }
        }
        _ => {}
    }
}

fn is_global_envir(cx: &Cx<'_, '_>, value: Node<'_>) -> bool {
    if value.kind() == "identifier" {
        return trimmed(cx, value) == ".GlobalEnv";
    }
    if value.kind() != "call" {
        return false;
    }
    child_by_field(value, "function").is_some_and(|f| trimmed(cx, f) == "globalenv")
}

pub(crate) fn global_assign_target<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Option<&'s str> {
    let arguments = argument_nodes(call);
    let first = arguments.first()?;
    if child_by_field(*first, "name").is_some() {
        return None;
    }
    let value = child_by_field(*first, "value").filter(|v| v.kind() == "string")?;
    let global = arguments.iter().any(|argument| {
        child_by_field(*argument, "name").is_some_and(|name| trimmed(cx, name) == "envir")
            && child_by_field(*argument, "value").is_some_and(|target| is_global_envir(cx, target))
    });
    global.then(|| string_content(cx, value)).flatten()
}
