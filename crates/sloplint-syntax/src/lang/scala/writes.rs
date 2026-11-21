use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use super::calls::func_node;
use super::items::first_ident;
use crate::hooks::{
    Cx, FileSetKind, child_by_field, first_named_child, named_children, pos_of, text,
};

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "field_expression" => child_by_field(current, "value")?,
            "parenthesized_expression" => first_named_child(current)?,
            _ => return None,
        };
    }
}

fn is_compound_write_op(op: &str) -> bool {
    op.ends_with('=')
        && !matches!(
            op,
            "==" | "!=" | "<=" | ">=" | "=/=" | "=!=" | "===" | "!==" | "=:="
        )
}

pub(crate) fn is_compound_write(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "infix_expression"
        && child_by_field(n, "operator").is_some_and(|o| is_compound_write_op(text(cx, o)))
}

pub(crate) fn write_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if (n.kind() == "assignment_expression" || is_compound_write(cx, n))
        && let Some(l) = child_by_field(n, "left")
    {
        if n.kind() == "assignment_expression"
            && l.kind() == "identifier"
            && cx
                .parent()
                .is_some_and(|parent| parent.kind() == "arguments")
            && named_arg_verdict(cx, n, text(cx, l)) != Some(false)
        {
            // `f(name = v)` is a named argument, not a write — same rule as
            // report_assign_write. A field path stays a real write.
            return;
        }
        out.extend(write_root(cx, l));
    }
}

fn var_pattern_name<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> Option<&'s str> {
    if c.kind() != "var_definition" {
        return None;
    }
    let pat = child_by_field(c, "pattern")?;
    first_ident(cx, pat).map(|(name, _)| name)
}

fn top_level_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    named_children(root)
        .iter()
        .filter_map(|c| var_pattern_name(cx, *c))
        .collect()
}

fn top_level_var<'s>(cx: &Cx<'_, 's>, _n: Node<'_>, name: &str) -> bool {
    cx.file_names(FileSetKind::Scope, || top_level_names(cx))
        .contains(&name)
}

// A backquoted name compares equal to its unquoted spelling —
// `` `using` `` in `def f(`using`: Int)` and `using` in `f(using = 1)`
// are the same parameter.
fn unquote(name: &str) -> &str {
    name.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(name)
}

// Whether `name = v` inside `f(...)` names a parameter of a `def f`
// visible from the call. The innermost lexical scope holding a `def f`
// decides — a member method shadows a file-scope signature. None when
// the callee cannot be resolved (precision first — the named-argument
// reading is the modern default); Some(true) when an in-scope def has
// that parameter; Some(false) when every in-scope `def f` lacks it —
// in Scala 2.12 scripts that shape is a `Unit`-valued assignment.
fn named_arg_verdict<'s>(cx: &Cx<'_, 's>, n: Node<'_>, arg_name: &str) -> Option<bool> {
    let args = n.parent().filter(|p| p.kind() == "arguments")?;
    let call = args.parent().filter(|p| p.kind() == "call_expression")?;
    let func = func_node(call)?;
    if func.kind() != "identifier" {
        return None;
    }
    let fname = unquote(text(cx, func));
    let arg_name = unquote(arg_name);
    let mut scope = n.parent();
    while let Some(s) = scope {
        scope = s.parent();
        let mut saw_def = false;
        for def in named_children(s) {
            if def.kind() != "function_definition" && def.kind() != "function_declaration" {
                continue;
            }
            let Some(name_node) = child_by_field(def, "name") else {
                continue;
            };
            if unquote(text(cx, name_node)) != fname {
                continue;
            }
            saw_def = true;
            for c in named_children(def) {
                if c.kind() != "parameters" {
                    continue;
                }
                for param in named_children(c) {
                    if param.kind() == "parameter"
                        && child_by_field(param, "name")
                            .is_some_and(|nm| unquote(text(cx, nm)) == arg_name)
                    {
                        return Some(true);
                    }
                }
            }
        }
        if saw_def {
            return Some(false);
        }
    }
    None
}

pub(crate) fn report_assign_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    if n.kind() == "assignment_expression"
        && left.kind() == "identifier"
        && n.parent().is_some_and(|p| p.kind() == "arguments")
        && named_arg_verdict(cx, n, text(cx, left)) != Some(false)
    {
        // `f(name = v)` is a named argument, not a write — unless a same-file
        // `def f` has no such parameter, which is a Scala 2.12 assignment.
        // A field path (`f(obj.x = v)`) can't name a parameter, so it stays
        // a real write.
        return;
    }
    let Some((name, pos)) = write_root(cx, left) else {
        return;
    };
    if top_level_var(cx, n, name) {
        out.push((AmbientKind::GlobalWrite, name, pos));
    }
}
