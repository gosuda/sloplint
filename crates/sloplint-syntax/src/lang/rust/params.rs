use sloplint_core::facts::{Param, Pos, PrimitiveKind};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, has_child_kind, named_children, pos_of, text};

pub(super) fn push_ident_tree<'a>(cx: &Cx<'_, '_>, root: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" || cur.kind() == "shorthand_field_identifier" {
            if is_pat_binding(text(cx, cur)) {
                out.push(cur);
            }
        } else {
            push_pattern_kids(cur, &mut stack);
        }
    }
}

pub(super) fn push_mut_idents<'a>(cx: &Cx<'_, '_>, root: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "mut_pattern" {
            push_ident_tree(cx, cur, out);
        } else {
            push_pattern_kids(cur, &mut stack);
        }
    }
}

fn is_pat_binding(name: &str) -> bool {
    name.chars()
        .next()
        .is_none_or(|first| !first.is_ascii_uppercase())
}

fn push_pattern_kids<'a>(cur: Node<'a>, stack: &mut Vec<Node<'a>>) {
    if cur.kind() == "scoped_identifier" {
        return;
    }
    if cur.kind() == "tuple_struct_pattern" || cur.kind() == "struct_pattern" {
        let skip = child_by_field(cur, "type").map(|node| node.id());
        for child in named_children(cur) {
            if skip.is_some_and(|id| id == child.id()) {
                continue;
            }
            stack.push(child);
        }
        return;
    }
    stack.extend(named_children(cur));
}

fn strip_ref_prefix(rest: &str) -> Option<&str> {
    rest.strip_prefix('&').map(|s| s.trim_start())
}

fn strip_mut_keyword(rest: &str) -> Option<&str> {
    let after = rest.strip_prefix("mut")?;
    if after.is_empty() {
        return Some("");
    }
    after
        .starts_with(char::is_whitespace)
        .then(|| after.trim_start())
}

fn strip_lifetime(rest: &str) -> Option<&str> {
    let body = rest.strip_prefix('\'')?;
    let end = body.find(char::is_whitespace)?;
    Some(body[end..].trim_start())
}

fn strip_known_prefix<'a>(rest: &'a str, prefixes: &[&str]) -> &'a str {
    for prefix in prefixes {
        if let Some(inner) = rest.strip_prefix(prefix) {
            return inner;
        }
    }
    rest
}

pub(super) fn primitive_kind_of(ty: Option<&str>) -> PrimitiveKind {
    let mut rest = ty.map_or("", |t| t.trim());
    loop {
        let before = rest;
        if let Some(inner) = strip_ref_prefix(rest) {
            rest = inner;
        } else if let Some(inner) = strip_lifetime(rest) {
            rest = inner;
        } else if let Some(inner) = strip_mut_keyword(rest) {
            rest = inner;
        }
        if rest == before {
            break;
        }
    }
    rest = strip_known_prefix(rest, &["::"]);
    rest = strip_known_prefix(rest, &["std::", "core::", "alloc::"]);
    rest = strip_known_prefix(rest, &["primitive::", "string::"]);
    match rest {
        "bool" => PrimitiveKind::Bool,
        "str" | "String" => PrimitiveKind::Str,
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => PrimitiveKind::Int,
        "f32" | "f64" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn typed_param<'s>(name: &'s str, ty: Option<&'s str>) -> Param<'s> {
    Param {
        name,
        ty,
        primitive_kind: primitive_kind_of(ty),
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    }
}

fn push_pattern_params<'s>(
    cx: &Cx<'_, 's>,
    pattern: Node<'_>,
    ty: Option<&'s str>,
    out: &mut Vec<Param<'s>>,
) {
    let mut ids = Vec::new();
    push_ident_tree(cx, pattern, &mut ids);
    if ids.is_empty() {
        out.push(typed_param("", ty));
    }
    for id in ids {
        out.push(typed_param(text(cx, id), ty));
    }
}

pub(super) fn rust_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Param<'s>>) {
    let Some(list) = child_by_field(n, "parameters") else {
        return;
    };
    for child in named_children(list) {
        if child.kind() != "parameter" {
            continue;
        }
        let Some(pattern) = child_by_field(child, "pattern") else {
            continue;
        };
        let ty = child_by_field(child, "type").map(|t| text(cx, t));
        push_pattern_params(cx, pattern, ty, out);
    }
}

pub(super) fn closure_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Param<'s>>) {
    let Some(list) = child_by_field(n, "parameters") else {
        return;
    };
    for child in named_children(list) {
        if child.kind() == "parameter" {
            let Some(pattern) = child_by_field(child, "pattern") else {
                continue;
            };
            let ty = child_by_field(child, "type").map(|t| text(cx, t));
            push_pattern_params(cx, pattern, ty, out);
        } else if child.kind() != "attribute_item" {
            push_pattern_params(cx, child, None, out);
        }
    }
}

pub(super) fn decl_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    match n.kind() {
        "let_declaration" => {
            let Some(pat) = child_by_field(n, "pattern") else {
                return;
            };
            let mut found = Vec::new();
            if has_child_kind(n, "mutable_specifier") {
                push_ident_tree(cx, pat, &mut found);
            } else {
                push_mut_idents(cx, pat, &mut found);
            }
            for id in found {
                out.push((text(cx, id), pos_of(id)));
            }
        }
        "for_expression" => {
            if let Some(pat) = child_by_field(n, "pattern") {
                push_flow_binder_names(cx, pat, out);
            }
        }
        "if_expression" | "while_expression" => {
            if let Some(cond) = child_by_field(n, "condition") {
                push_let_condition_names(cx, cond, out);
            }
        }
        "match_pattern" => {
            let guard = child_by_field(n, "condition").map(|node| node.id());
            for child in named_children(n) {
                if guard.is_some_and(|id| id == child.id()) {
                    continue;
                }
                push_flow_binder_names(cx, child, out);
            }
        }
        _ => {}
    }
}

pub(super) fn decl_scope_parent(depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
    match node.kind() {
        "if_expression" | "while_expression" | "for_expression" | "match_arm" => depth,
        "block" => then_block_scope(depth, node, cx),
        _ => depth.saturating_sub(1),
    }
}

pub(super) fn shadow_decl_kind(kind: &str) -> bool {
    matches!(
        kind,
        "for_expression" | "while_expression" | "match_arm" | "block"
    )
}

pub(super) fn shadow_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "for_expression" => {
            if let Some(pat) = child_by_field(n, "pattern") {
                push_shadow_pattern_names(cx, pat, out);
            }
        }
        "while_expression" => {
            if let Some(cond) = child_by_field(n, "condition") {
                push_shadow_condition_names(cx, cond, out);
            }
        }
        "match_arm" => {
            if let Some(pat) = child_by_field(n, "pattern") {
                push_shadow_arm_names(cx, pat, out);
            }
        }
        "block" => {
            push_then_block_names(cx, n, out);
        }
        _ => {}
    }
}

fn then_block_scope(depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
    let Some(parent) = cx.parent_of(node) else {
        return depth.saturating_sub(1);
    };
    if parent.kind() == "if_expression"
        && child_by_field(parent, "consequence").is_some_and(|first| first.id() == node.id())
    {
        depth
    } else {
        depth.saturating_sub(1)
    }
}

fn push_flow_binder_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "mut_pattern"
            || cur.kind() == "field_pattern" && has_child_kind(cur, "mutable_specifier")
        {
            let mut found = Vec::new();
            push_ident_tree(cx, cur, &mut found);
            for id in found {
                out.push((text(cx, id), pos_of(id)));
            }
        } else {
            push_pattern_kids(cur, &mut stack);
        }
    }
}

fn push_let_condition_names<'s>(cx: &Cx<'_, 's>, cond: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![cond];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "let_condition" {
            if let Some(pat) = child_by_field(cur, "pattern") {
                push_flow_binder_names(cx, pat, out);
            }
        } else if cur.kind() == "let_chain" {
            stack.extend(named_children(cur));
        }
    }
}
fn push_shadow_pattern_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "mut_pattern"
            || cur.kind() == "field_pattern" && has_child_kind(cur, "mutable_specifier")
        {
            continue;
        }
        if cur.kind() == "identifier" || cur.kind() == "shorthand_field_identifier" {
            let name = text(cx, cur);
            if is_pat_binding(name) {
                out.push(name);
            }
        } else {
            push_pattern_kids(cur, &mut stack);
        }
    }
}

fn push_shadow_condition_names<'s>(cx: &Cx<'_, 's>, cond: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![cond];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "let_condition"
            && let Some(pat) = child_by_field(cur, "pattern")
        {
            push_shadow_pattern_names(cx, pat, out);
        } else if cur.kind() == "let_chain" {
            stack.extend(named_children(cur));
        }
    }
}

fn push_shadow_arm_names<'s>(cx: &Cx<'_, 's>, pat: Node<'_>, out: &mut Vec<&'s str>) {
    let guard = child_by_field(pat, "condition").map(|node| node.id());
    for child in named_children(pat) {
        if guard.is_some_and(|id| id == child.id()) {
            continue;
        }
        push_shadow_pattern_names(cx, child, out);
    }
}

fn push_then_block_names<'s>(cx: &Cx<'_, 's>, body: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(parent) = cx.parent_of(body) else {
        return;
    };
    if parent.kind() != "if_expression" {
        return;
    }
    if child_by_field(parent, "consequence").is_none_or(|first| first.id() != body.id()) {
        return;
    }
    let Some(cond) = child_by_field(parent, "condition") else {
        return;
    };
    push_shadow_condition_names(cx, cond, out);
}
