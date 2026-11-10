use sloplint_core::facts::{Arg, ArgShape, CallKind};
use tree_sitter::Node;

use crate::hooks::{
    Cx, Link, binary_concat, child_by_field, first_of_kind, named_children, narrow, slice,
    string_arg_shape, text,
};

use super::field_text;

pub(crate) const RECEIVER_CHAIN: u8 = 0;

pub(crate) fn ruby_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    match (child_by_field(n, "receiver"), child_by_field(n, "method")) {
        (Some(recv), Some(method)) => slice(cx.src, recv.start_byte(), method.end_byte()),
        (None, Some(method)) => text(cx, method),
        _ => "",
    }
}
pub(crate) fn ruby_call_kind<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> CallKind {
    let method = field_text(cx, n, "method").unwrap_or("");
    if method == "new" && child_by_field(n, "receiver").is_some() {
        CallKind::New
    } else {
        CallKind::Plain
    }
}
pub(crate) fn ruby_receiver_root<'s>(cx: &Cx<'_, 's>, recv: Node<'_>) -> &'s str {
    cx.chain_root(RECEIVER_CHAIN, recv, |current| {
        if current.kind() != "call" {
            return Link::Root(current);
        }
        match child_by_field(current, "receiver") {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        }
    })
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
pub(crate) fn ruby_is_simple_path(_cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(recv) = child_by_field(n, "receiver") else {
        return true;
    };
    [
        "identifier",
        "constant",
        "self",
        "instance_variable",
        "class_variable",
        "global_variable",
    ]
    .contains(&recv.kind())
}
pub(crate) fn ruby_arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "heredoc_body" => string_arg_shape(cx, n, &["heredoc_content"]),
        "chained_string" => ArgShape::Concat,
        "hash" | "array" => ArgShape::Collection,
        "binary" => {
            if binary_concat(cx, n, "+", &["string", "heredoc_body", "chained_string"]) {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
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
