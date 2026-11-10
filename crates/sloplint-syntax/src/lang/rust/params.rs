use sloplint_core::facts::{Param, Pos, PrimitiveKind};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, has_child_kind, named_children, pos_of, text};

pub(super) fn push_ident_tree<'a>(root: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            out.push(cur);
        } else {
            stack.extend(named_children(cur));
        }
    }
}

pub(super) fn push_mut_idents<'a>(root: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "mut_pattern" {
            push_ident_tree(cur, out);
        } else {
            stack.extend(named_children(cur));
        }
    }
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

fn typed_param<'s>(cx: &Cx<'_, 's>, pattern: Node<'_>, ty: Option<&'s str>) -> Param<'s> {
    let mut ids = Vec::new();
    push_ident_tree(pattern, &mut ids);
    let name = ids.first().map_or("", |id| text(cx, *id));
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

fn untyped_param<'s>(cx: &Cx<'_, 's>, pattern: Node<'_>) -> Param<'s> {
    let mut ids = Vec::new();
    push_ident_tree(pattern, &mut ids);
    let name = ids.first().map_or("", |id| text(cx, *id));
    Param {
        name,
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
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
        out.push(typed_param(cx, pattern, ty));
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
            out.push(typed_param(cx, pattern, ty));
        } else if child.kind() != "attribute_item" {
            out.push(untyped_param(cx, child));
        }
    }
}

pub(super) fn decl_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(pat) = child_by_field(n, "pattern") else {
        return;
    };
    let mut found = Vec::new();
    if has_child_kind(n, "mutable_specifier") {
        push_ident_tree(pat, &mut found);
    } else {
        push_mut_idents(pat, &mut found);
    }
    for id in found {
        out.push((text(cx, id), pos_of(id)));
    }
}
