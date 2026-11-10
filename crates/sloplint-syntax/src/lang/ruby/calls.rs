use sloplint_core::facts::{Arg, ArgShape, CallKind};
use tree_sitter::Node;

use crate::hooks::{
    Cx, Link, child_by_field, first_of_kind, has_child_kind, named_children, narrow, slice,
    string_arg_shape, text,
};

use super::field_text;

pub(crate) const RECEIVER_CHAIN: u8 = 0;
const RECEIVER_BASE: u8 = 13;
const CHAIN_DIRTY: u8 = 1;
const CHAIN_REF: u8 = 2;

pub(crate) fn ruby_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let Some(method) = child_by_field(n, "method") else {
        return "";
    };
    let Some(recv) = child_by_field(n, "receiver") else {
        return text(cx, method);
    };
    if receiver_chain_clean(cx, recv) && ruby_receiver_root(cx, recv).is_some() {
        return slice(cx.src, recv.start_byte(), method.end_byte());
    }
    text(cx, method)
}
fn receiver_chain_clean(cx: &Cx<'_, '_>, recv: Node<'_>) -> bool {
    cx.chain_root(CHAIN_DIRTY, recv, dirty_step) == cx.chain_root(CHAIN_REF, recv, ref_step)
}
fn dirty_step(current: Node<'_>) -> Link<'_> {
    if current.kind() == "call" {
        if chain_has_arguments(current) {
            return Link::Root(current);
        }
        return match child_by_field(current, "receiver") {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        };
    }
    if current.kind() == "scope_resolution" {
        return match child_by_field(current, "scope") {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        };
    }
    Link::Root(current)
}
fn ref_step(current: Node<'_>) -> Link<'_> {
    if current.kind() == "call" {
        return ref_call_step(current);
    }
    if current.kind() == "scope_resolution" {
        return match child_by_field(current, "scope") {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        };
    }
    Link::Root(current)
}
fn ref_call_step(current: Node<'_>) -> Link<'_> {
    if chain_has_arguments(current) {
        if let Some(next) = child_by_field(current, "receiver") {
            return Link::Next(next);
        }
        if let Some(method) = child_by_field(current, "method") {
            return Link::Root(method);
        }
        return Link::Root(current);
    }
    match child_by_field(current, "receiver") {
        Some(next) => Link::Next(next),
        None => Link::Root(current),
    }
}
fn chain_has_arguments(n: Node<'_>) -> bool {
    if has_child_kind(n, "block") || has_child_kind(n, "do_block") {
        return true;
    }
    let Some(list) = first_of_kind(n, &["argument_list"]) else {
        return false;
    };
    let count = u32::try_from(list.named_child_count()).unwrap_or(u32::MAX);
    (0..count)
        .filter_map(|i| list.named_child(i))
        .any(|c| c.kind() != "comment")
}
pub(crate) fn ruby_call_kind<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> CallKind {
    let method = field_text(cx, n, "method").unwrap_or("");
    if method == "new" && child_by_field(n, "receiver").is_some() {
        CallKind::New
    } else {
        CallKind::Plain
    }
}
pub(crate) fn ruby_new_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>, recv: Node<'_>) -> &'s str {
    if receiver_chain_clean(cx, recv) && ruby_receiver_root(cx, recv).is_some() {
        return text(cx, recv);
    }
    field_text(cx, n, "method").unwrap_or("new")
}
pub(crate) fn ruby_receiver_root<'s>(cx: &Cx<'_, 's>, recv: Node<'_>) -> Option<&'s str> {
    let root = cx.chain_root(RECEIVER_CHAIN, recv, receiver_step);
    let base = cx.chain_root(RECEIVER_BASE, recv, base_step);
    if root != base {
        return None;
    }
    canonical_root(root)
}
pub(crate) fn canonical_root(root: &str) -> Option<&str> {
    let stripped = root.trim_start_matches(['@', '$']);
    if stripped.is_empty()
        || !stripped
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'?' | b'!' | b':'))
    {
        return None;
    }
    let bare = stripped.split("::").next().unwrap_or("");
    let first = bare.bytes().next()?;
    if !first.is_ascii_alphabetic() && first != b'_' {
        return None;
    }
    if bare
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'?' | b'!'))
    {
        Some(bare)
    } else {
        None
    }
}
fn receiver_step<'n>(current: Node<'n>) -> Link<'n> {
    if current.kind() != "call" {
        return Link::Root(current);
    }
    match child_by_field(current, "receiver") {
        Some(next) => Link::Next(next),
        None => Link::Root(current),
    }
}
fn base_step<'n>(current: Node<'n>) -> Link<'n> {
    if current.kind() != "call" {
        return Link::Root(current);
    }
    match child_by_field(current, "receiver") {
        Some(next) => Link::Next(next),
        None => match child_by_field(current, "method") {
            Some(method) => Link::Root(method),
            None => Link::Root(current),
        },
    }
}
pub(crate) fn call_args(n: Node<'_>) -> Vec<Node<'_>> {
    match first_of_kind(n, &["argument_list"]) {
        Some(list) => named_children(list)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect(),
        None => Vec::new(),
    }
}
pub(crate) fn ruby_is_simple_path(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(recv) = child_by_field(n, "receiver") else {
        return true;
    };
    receiver_chain_clean(cx, recv) && ruby_receiver_root(cx, recv).is_some()
}
fn str_static_len(cx: &Cx<'_, '_>, n: Node<'_>, fragments: &[&str]) -> u32 {
    match string_arg_shape(cx, n, fragments) {
        ArgShape::Str { longest_static } => longest_static,
        _ => 0,
    }
}
fn chained_len(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut total = 0u32;
    let mut cursor = n.walk();
    for child in n.children(&mut cursor) {
        if child.is_named() && child.kind() == "string" {
            total += str_static_len(cx, child, &["string_content"]);
        }
    }
    total
}
fn operand_static_len(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    match n.kind() {
        "string" => str_static_len(cx, n, &["string_content"]),
        "heredoc_body" => str_static_len(cx, n, &["heredoc_content"]),
        "chained_string" => chained_len(cx, n),
        _ => 0,
    }
}
fn unwrap_paren(mut n: Node<'_>) -> Node<'_> {
    while n.kind() == "parenthesized_statements" {
        let mut cursor = n.walk();
        let mut inner = n
            .children(&mut cursor)
            .filter(|c| c.is_named() && c.kind() != "comment");
        match (inner.next(), inner.next()) {
            (Some(only), None) => n = only,
            _ => break,
        }
    }
    n
}
fn concat_static_len(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    let mut best = 0u32;
    let mut found = false;
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        let current = unwrap_paren(current);
        let (Some(left), Some(op), Some(right)) =
            (current.child(0), current.child(1), current.child(2))
        else {
            continue;
        };
        if current.kind() != "binary" || current.child_count() != 3 || text(cx, op) != "+" {
            continue;
        }
        for operand in [left, right] {
            let operand = unwrap_paren(operand);
            if matches!(operand.kind(), "string" | "heredoc_body" | "chained_string") {
                found = true;
            }
            best = best.max(operand_static_len(cx, operand));
            if operand.kind() == "binary" {
                stack.push(operand);
            }
        }
    }
    found.then_some(best)
}
fn array_has_closure(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        if matches!(current.kind(), "lambda" | "block" | "do_block") {
            return true;
        }
        let mut cursor = current.walk();
        stack.extend(current.children(&mut cursor).filter(|c| c.is_named()));
    }
    false
}
pub(crate) fn ruby_arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => {
            if has_child_kind(n, "interpolation") {
                ArgShape::Concat {
                    longest_static: str_static_len(cx, n, &["string_content"]),
                }
            } else {
                string_arg_shape(cx, n, &["string_content"])
            }
        }
        "heredoc_body" => {
            if has_child_kind(n, "interpolation") {
                ArgShape::Concat {
                    longest_static: str_static_len(cx, n, &["heredoc_content"]),
                }
            } else {
                string_arg_shape(cx, n, &["heredoc_content"])
            }
        }
        "chained_string" => ArgShape::Concat {
            longest_static: chained_len(cx, n),
        },
        "hash" => ArgShape::Other,
        "array" => {
            if array_has_closure(n) {
                ArgShape::CodeBody
            } else {
                ArgShape::Collection
            }
        }
        "lambda" => ArgShape::CodeBody,
        "binary" => match concat_static_len(cx, n) {
            Some(longest_static) => ArgShape::Concat { longest_static },
            None => ArgShape::Other,
        },
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}

pub(crate) fn ruby_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    call_args(n)
        .into_iter()
        .map(|arg| Arg {
            shape: ruby_arg_shape(cx, arg),
            start_line: narrow(arg.start_position().row) + 1,
            end_line: narrow(arg.end_position().row) + 1,
        })
        .collect()
}
