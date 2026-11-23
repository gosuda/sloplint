//! Lean call and argument-shape heads: generic application nodes.

use crate::hooks::{CallHead, Cx, child_by_field, narrow, text, trimmed};
use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::text::{longest_static_chunk, longest_static_chunk_interp};
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

/// String argument shapes: plain and raw literals are one static run.
/// Interpolated `s!"..."/m!"..."` strings measure the longest literal
/// run in the gaps between the parsed `interpolation` hole spans: the
/// grammar marks every hole, so primed identifiers and nested calls never
/// leak into the measurement. Escapes break runs but `%` stays literal;
/// the linter only compares against an 8-char floor.
fn lean_literal_run(segment: &str) -> u32 {
    let mut best = 0u32;
    let mut run = 0u32;
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            best = best.max(run);
            run = 0;
            chars.next();
        } else {
            run += 1;
        }
    }
    best.max(run)
}

/// Gaps between the parsed hole spans are literal text. A string whose
/// opener or closer the parser did not produce falls back to the shared
/// hole scanner so error-recovery trees still measure something.
fn lean_interpolated_static(cx: &Cx<'_, '_>, arg: Node<'_>) -> u32 {
    let raw = text(cx, arg);
    let Some(rest) = raw
        .strip_prefix("s!\"")
        .or_else(|| raw.strip_prefix("m!\""))
    else {
        return longest_static_chunk_interp(raw);
    };
    let Some(inner) = rest.strip_suffix('"') else {
        return longest_static_chunk_interp(raw);
    };
    let base = arg.start_byte() + (raw.len() - rest.len());
    let mut holes: Vec<(usize, usize)> = Vec::new();
    for index in 0..arg.child_count() {
        let Some(child) = arg.child(index) else {
            continue;
        };
        if child.kind() == "interpolation" {
            holes.push((
                child.start_byte().saturating_sub(base),
                child.end_byte().saturating_sub(base),
            ));
        }
    }
    let mut best = 0u32;
    let mut pos = 0usize;
    for (start, end) in holes {
        let start = start.min(inner.len());
        let end = end.min(inner.len());
        if start > pos
            && let Some(gap) = inner.get(pos..start)
        {
            best = best.max(lean_literal_run(gap));
        }
        pos = pos.max(end);
    }
    if let Some(gap) = inner.get(pos..) {
        best = best.max(lean_literal_run(gap));
    }
    best
}

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
        "interpolated_str" => ArgShape::Str {
            longest_static: lean_interpolated_static(cx, arg),
        },
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
