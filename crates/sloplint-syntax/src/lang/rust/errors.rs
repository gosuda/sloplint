use sloplint_core::facts::{CallKind, Stmt};
use tree_sitter::Node;

use crate::hooks::{
    Cx, any_named_child, child_by_field, first_named_child, last_segment, named_children, text,
};

fn enum_pattern(cx: &Cx<'_, '_>, pat: Node<'_>) -> bool {
    let mut stack = vec![pat];
    while let Some(node) = stack.pop() {
        if node.kind() == "match_pattern" {
            let guarded = child_by_field(node, "condition").map(|cond| cond.id());
            stack.extend(
                named_children(node)
                    .into_iter()
                    .filter(|child| Some(child.id()) != guarded),
            );
            continue;
        }
        match node.kind() {
            "tuple_struct_pattern" | "struct_pattern" => return true,
            "scoped_identifier" | "scoped_type_identifier" if text(cx, node).contains("::") => {
                return true;
            }
            _ => {}
        }
        stack.extend(named_children(node));
    }
    false
}
pub(super) fn sibling_enum(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.parent().is_some_and(|parent| {
        any_named_child(parent, |arm| {
            arm.kind() == "match_arm"
                && arm.id() != n.id()
                && child_by_field(arm, "pattern").is_some_and(|p| enum_pattern(cx, p))
        })
    })
}
pub(super) fn err_bound<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> Option<Option<&'s str>> {
    let pat = if pat.kind() == "match_pattern" {
        if child_by_field(pat, "condition").is_some() {
            return None;
        }
        let only = pat.named_child(0)?;
        if pat.named_child(1).is_some() {
            return None;
        }
        only
    } else {
        pat
    };
    if pat.kind() != "tuple_struct_pattern" {
        return None;
    }
    if child_by_field(pat, "type").map_or("", |t| text(cx, t)) != "Err" {
        return None;
    }
    let ty = child_by_field(pat, "type")?;
    let mut cursor = pat.walk();
    let mut rest = pat
        .children(&mut cursor)
        .filter(|c| c.id() != ty.id() && (c.is_named() || c.kind() == "_"));
    let only = rest.next()?;
    if rest.next().is_some() {
        return None;
    }
    if only.kind() == "identifier" {
        return Some(Some(text(cx, only)));
    }
    (text(cx, only) == "_").then_some(None)
}
fn match_arm_pattern(pat: Node<'_>) -> Option<Node<'_>> {
    if pat.kind() != "match_pattern" {
        return Some(pat);
    }
    let cond = child_by_field(pat, "condition").map(|c| c.id());
    let mut cursor = pat.walk();
    let mut kids = pat
        .children(&mut cursor)
        .filter(|c| Some(c.id()) != cond && (c.is_named() || c.kind() == "_"));
    let only = kids.next()?;
    if kids.next().is_some() {
        return None;
    }
    Some(only)
}

fn pattern_may_match_err(cx: &Cx<'_, '_>, pat: Node<'_>) -> bool {
    let mut pending = vec![pat];
    while let Some(node) = pending.pop() {
        let Some(inner) = match_arm_pattern(node) else {
            continue;
        };
        match inner.kind() {
            "tuple_struct_pattern" => {
                if child_by_field(inner, "type").is_some_and(|t| text(cx, t) == "Err") {
                    return true;
                }
            }
            "or_pattern" => {
                let mut cursor = inner.walk();
                pending.extend(
                    inner
                        .children(&mut cursor)
                        .filter(|c| c.is_named() || c.kind() == "_"),
                );
            }
            "_" | "identifier" => return true,
            _ => {}
        }
    }
    false
}
pub(super) fn has_err_selective_predecessor(cx: &Cx<'_, '_>, arm: Node<'_>) -> bool {
    let Some(parent) = arm.parent() else {
        return false;
    };
    let mut cursor = parent.walk();
    for sib in parent.children(&mut cursor) {
        if sib.id() == arm.id() {
            break;
        }
        if sib.kind() != "match_arm" {
            continue;
        }
        if let Some(pat) = child_by_field(sib, "pattern")
            && pattern_may_match_err(cx, pat)
        {
            return true;
        }
    }
    false
}
fn is_err_ctor(cx: &Cx<'_, '_>, call: Node<'_>, bound: Option<&str>, callee: &str) -> bool {
    let Some(b) = bound else {
        return false;
    };
    if last_segment(callee, &["::", "."]) != "Err" {
        return false;
    }
    child_by_field(call, "arguments")
        .and_then(first_named_child)
        .is_some_and(|f| f.kind() == "identifier" && text(cx, f) == b)
}
fn handler_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, bound: Option<&str>) -> Stmt<'s> {
    let expr = if s.kind() == "expression_statement" {
        first_named_child(s).unwrap_or(s)
    } else {
        s
    };
    match expr.kind() {
        "call_expression" => {
            let callee = child_by_field(expr, "function").map_or("", |f| text(cx, f));
            if is_err_ctor(cx, expr, bound, callee) {
                Stmt::Rethrow
            } else {
                Stmt::Call {
                    callee,
                    kind: CallKind::Plain,
                }
            }
        }
        "macro_invocation" => {
            let callee = child_by_field(expr, "macro").map_or("", |m| text(cx, m));
            Stmt::Call {
                callee,
                kind: CallKind::Macro,
            }
        }
        "return_expression" => match first_named_child(expr) {
            Some(v) if v.kind() == "call_expression" => {
                let callee = child_by_field(v, "function").map_or("", |f| text(cx, f));
                if is_err_ctor(cx, v, bound, callee) {
                    Stmt::Rethrow
                } else {
                    Stmt::Other
                }
            }
            _ => Stmt::Other,
        },
        _ => Stmt::Other,
    }
}
pub(super) fn body_stmts<'s>(
    cx: &Cx<'_, 's>,
    body: Node<'_>,
    bound: Option<&str>,
) -> Vec<Stmt<'s>> {
    if body.kind() == "block" {
        named_children(body)
            .into_iter()
            .filter(|c| c.kind() != "line_comment" && c.kind() != "block_comment")
            .map(|s| handler_stmt(cx, s, bound))
            .collect()
    } else {
        vec![handler_stmt(cx, body, bound)]
    }
}
