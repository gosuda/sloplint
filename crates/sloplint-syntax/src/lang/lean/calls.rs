//! Lean call and argument-shape heads: generic application nodes.

use crate::hooks::{CallHead, Cx, child_by_field, narrow, text, trimmed};
use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

/// Flatten `f x y` (`app(app(f, x), y)`) to the head term and the
/// argument list in order.
fn flatten_app<'a>(n: Node<'a>) -> (Node<'a>, Vec<Node<'a>>) {
    let mut head = n;
    let mut reversed = Vec::new();
    while head.kind() == "app" {
        let Some(fun) = child_by_field(head, "fn") else {
            break;
        };
        if let Some(arg) = child_by_field(head, "arg") {
            reversed.push(arg);
        }
        head = fun;
    }
    reversed.reverse();
    (head, reversed)
}

fn lean_callee_root(callee: &str) -> Option<&str> {
    let root = callee
        .split_whitespace()
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("");
    (!root.is_empty()).then_some(root)
}

/// String argument shapes: plain and raw literals are one static run,
/// interpolated `s!"..."` strings measure the longest anonymous gap
/// between `interpolation` children. Escapes inflate plain runs by a
/// byte or two; the linter only compares against an 8-char floor.
fn lean_arg_shape<'s>(cx: &Cx<'_, 's>, arg: Node<'_>) -> ArgShape<'s> {
    match arg.kind() {
        "str_lit" | "raw_string" => {
            let raw = text(cx, arg);
            let inner = raw
                .strip_prefix('r')
                .unwrap_or(raw)
                .trim_matches('#')
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(raw);
            ArgShape::Str {
                longest_static: longest_static_chunk(&[inner]),
            }
        }
        "interpolated_str" => {
            let mut runs = Vec::new();
            let mut cursor = arg.walk();
            for child in arg.children(&mut cursor) {
                if !child.is_named() {
                    runs.push(text(cx, child));
                }
            }
            ArgShape::Str {
                longest_static: longest_static_chunk(&runs),
            }
        }
        "identifier" => ArgShape::Ident(text(cx, arg)),
        _ => ArgShape::Other,
    }
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() != "app" {
        return None;
    }
    let (head, args) = flatten_app(n);
    let callee = if head.kind() == "identifier" {
        text(cx, head)
    } else {
        trimmed(cx, head)
    };
    let simple = head.kind() == "identifier";
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root: lean_callee_root(callee),
        is_simple_path: simple,
        initializes_binding: false,
        args: args
            .into_iter()
            .map(|arg| Arg {
                shape: lean_arg_shape(cx, arg),
                start_line: narrow(arg.start_position().row) + 1,
                end_line: narrow(arg.end_position().row) + 1,
            })
            .collect(),
    })
}
