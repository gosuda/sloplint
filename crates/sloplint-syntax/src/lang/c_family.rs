//! Helpers shared by the C, C++, and Objective-C adapters.

use sloplint_core::facts::{AmbientKind, Arg, ArgShape, Assertion, Pos};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, named_children, narrow, pos_of, slice, text};

pub(crate) fn flat(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
pub(crate) fn angles(t: &str) -> &str {
    t.strip_prefix('<')
        .unwrap_or(t)
        .strip_suffix('>')
        .unwrap_or(t)
}
pub(crate) fn word<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kinds: &[&str], words: &[&str]) -> bool {
    named_children(n)
        .iter()
        .any(|c| kinds.contains(&c.kind()) && words.contains(&text(cx, *c)))
}
pub(crate) fn inner(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "declarator").or_else(|| named_children(n).into_iter().next())
}
pub(crate) fn unwrap<'n>(n: Node<'n>, wrap: &[&str]) -> Node<'n> {
    let mut current = n;
    loop {
        if !wrap.contains(&current.kind()) {
            break;
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    current
}
pub(crate) fn leaf<'n>(n: Node<'n>, wrap: &[&str]) -> Node<'n> {
    unwrap(
        match n.kind() {
            "init_declarator" => child_by_field(n, "declarator").unwrap_or(n),
            _ => n,
        },
        wrap,
    )
}
pub(crate) fn span<'s>(cx: &Cx<'_, 's>, ty: Node<'_>, end: Node<'_>) -> &'s str {
    let shown = slice(cx.src, ty.start_byte(), end.start_byte()).trim();
    if !shown.is_empty() {
        shown
    } else {
        text(cx, ty)
    }
}
pub(crate) fn param_target<'n>(d: Node<'n>, wrap: &[&str]) -> (Option<Node<'n>>, bool) {
    let mut current = d;
    let mut callable = false;
    loop {
        if current.kind() == "function_declarator" {
            callable = true;
        } else if !wrap.contains(&current.kind()) {
            break;
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    (
        (current.kind() == "identifier").then_some(current),
        callable,
    )
}
pub(crate) fn const_pointer(cx: &Cx<'_, '_>, d: Node<'_>, wrap: &[&str]) -> bool {
    let mut current = match d.kind() {
        "init_declarator" => child_by_field(d, "declarator").unwrap_or(d),
        _ => d,
    };
    let mut innermost_const = false;
    while wrap.contains(&current.kind()) {
        if current.kind() == "pointer_declarator" {
            innermost_const = named_children(current)
                .iter()
                .any(|c| c.kind() == "type_qualifier" && text(cx, *c) == "const");
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    innermost_const
}
pub(crate) fn decl_info<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    wrap: &[&str],
) -> (Vec<(&'s str, Pos)>, bool) {
    let mut cursor = n.walk();
    let ds: Vec<Node<'_>> = n
        .children_by_field_name("declarator", &mut cursor)
        .collect();
    let mut out = Vec::new();
    let mut func = false;
    for d in ds {
        let found = leaf(d, wrap);
        if found.kind() == "function_declarator" {
            func = true;
        } else if found.kind() == "identifier" && !const_pointer(cx, d, wrap) {
            out.push((text(cx, found), pos_of(found)));
        }
    }
    (out, func)
}
pub(crate) fn root_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "field_expression" | "subscript_expression" | "pointer_expression" => {
                let next = child_by_field(current, "argument")?;
                current = next
            }
            "parenthesized_expression" => {
                let next = named_children(current).into_iter().next()?;
                current = next
            }
            _ => return None,
        }
    }
}
pub(crate) fn env_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    envs: &[&str],
) {
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let raw = text(cx, func);
    if envs.contains(&flat(raw).as_str()) {
        out.push((AmbientKind::EnvRead, raw, pos_of(n)));
    }
}
pub(crate) fn bin_op<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if n.child_count() != 3 {
        return None;
    }
    n.child(1).map(|op| text(cx, op))
}
pub(crate) fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|a| {
        named_children(a)
            .into_iter()
            .find(|c| c.kind() != "comment")
    })
}
pub(crate) fn plain_assert(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let arg = first_arg(n).filter(|a| a.kind() == "binary_expression");
    let mut has_message = false;
    let mut comparison = false;
    if let Some(found) = arg {
        let op = bin_op(cx, found).unwrap_or("");
        if op == "&&" {
            has_message =
                child_by_field(found, "right").is_some_and(|r| r.kind() == "string_literal");
        }
        comparison = op == "==" || op == "!=";
    }
    Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
    }
}
pub(crate) fn pair_source<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    if pair.kind() != "initializer_pair" {
        return None;
    }
    let value = child_by_field(pair, "value")?;
    if value.kind() != "field_expression" {
        return None;
    }
    let designator = designator_of(cx, pair)?;
    let field = child_by_field(value, "field")?;
    if designator != text(cx, field) {
        return None;
    }
    let arg = child_by_field(value, "argument")?;
    if arg.kind() != "identifier" {
        return None;
    }
    Some(text(cx, arg))
}
pub(crate) fn designator_of<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    for c in named_children(pair) {
        if c.kind() == "field_designator" {
            let found = named_children(c)
                .into_iter()
                .find(|d| d.kind() == "field_identifier")?;
            return Some(text(cx, found));
        }
        if c.kind() == "field_identifier" {
            return Some(text(cx, c));
        }
    }
    None
}
pub(crate) fn in_function(n: Node<'_>) -> bool {
    let mut parent = n.parent();
    while let Some(found) = parent {
        if found.kind() == "function_definition" {
            return true;
        }
        parent = found.parent();
    }
    false
}
pub(crate) fn collect_fragments<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    kinds: &[&str],
    out: &mut Vec<&'s str>,
) {
    for i in 0..n.child_count() {
        if let Some(found) = n.child(i)
            && kinds.contains(&found.kind())
        {
            out.push(text(cx, found));
        }
    }
}
pub(crate) fn sole_call(body: Node<'_>) -> Option<usize> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let mut stmts = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment");
    let stmt = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    if stmt.kind() != "expression_statement" && stmt.kind() != "return_statement" {
        return None;
    }
    let expr =
        child_by_field(stmt, "expression").or_else(|| named_children(stmt).into_iter().next())?;
    (expr.kind() == "call_expression").then_some(expr.id())
}
pub(crate) fn call_args<'s>(
    cx: &Cx<'_, 's>,
    args_node: Node<'_>,
    shape: fn(&Cx<'_, 's>, Node<'_>) -> ArgShape<'s>,
) -> Vec<Arg<'s>> {
    let mut args = Vec::new();
    for c in named_children(args_node) {
        if c.kind() == "comment" {
            continue;
        }
        args.push(Arg {
            shape: shape(cx, c),
            start_line: narrow(c.start_position().row) + 1,
            end_line: narrow(c.end_position().row) + 1,
        });
    }
    args
}
