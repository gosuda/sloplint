use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, binary_concat, child_by_field, first_named_child_matching, first_of_kind,
    named_children, narrow, string_arg_shape, text, trimmed,
};

pub(crate) fn builtin_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let found = first_of_kind(n, &["builtin_identifier"]);
    let name = found.map(|id| trimmed(cx, id));
    name.map(|raw| raw.strip_prefix('@').unwrap_or(raw))
        .unwrap_or("")
}

pub(crate) fn is_import_builtin(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    first_of_kind(n, &["builtin_identifier"]).is_some_and(|id| text(cx, id) == "@import")
}

pub(crate) fn positional_nodes<'a>(n: Node<'a>, func: Node<'a>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        if child.id() == func.id() {
            continue;
        }
        if child.kind() == "arguments" {
            out.extend(named_children(child));
        } else {
            out.push(child);
        }
    }
    out
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "multiline_string" => {
            let raw = text(cx, n);
            let clean = |line: &'s str| line.trim_start().strip_prefix("\\\\").unwrap_or(line);
            let parts: Vec<&'s str> = raw.lines().map(clean).collect();
            ArgShape::Str {
                longest_static: longest_static_chunk(&parts),
            }
        }
        "identifier" => ArgShape::Ident(text(cx, n)),
        "binary_expression" if binary_concat(cx, n, "++", &["string", "multiline_string"]) => {
            ArgShape::Concat
        }
        "struct_initializer" | "anonymous_struct_initializer" | "array_literal" => {
            ArgShape::Collection
        }
        "block" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}

fn callee_root(func: Node<'_>) -> Option<Node<'_>> {
    let mut node = func;
    loop {
        if node.kind() == "identifier" {
            return Some(node);
        }
        {
            let next = child_by_field(node, "object")?;
            node = next
        }
    }
}

fn zig_is_simple_path(func: Node<'_>) -> bool {
    if func.kind() == "identifier" {
        return true;
    }
    if func.kind() != "field_expression" {
        return false;
    }
    let mut node = func;
    loop {
        let Some(object) = child_by_field(node, "object") else {
            return false;
        };
        if object.kind() == "identifier" {
            return true;
        }
        if object.kind() != "field_expression" {
            return false;
        }
        node = object;
    }
}

pub(crate) fn decl_value(holder: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = holder.walk();
    let mut seen_eq = false;
    let mut value = None;
    for child in holder.children(&mut cursor) {
        if !child.is_named() {
            if child.kind() == "=" {
                seen_eq = true;
            }
        } else if seen_eq {
            value = Some(child);
        }
    }
    value
}

fn zig_initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(holder) = cx.parent() else {
        return false;
    };
    if holder.kind() != "variable_declaration" {
        return false;
    }
    if decl_value(holder).is_none_or(|found| found.id() != n.id()) {
        return false;
    }
    let named = first_named_child_matching(holder, &["identifier"]);
    named.is_some_and(|id| text(cx, id) != "_")
}

pub(crate) fn zig_call<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
    if n.kind() == "call_expression" {
        let func = child_by_field(n, "function")?;
        let root = callee_root(func);
        let args = positional_nodes(n, func)
            .into_iter()
            .map(|a| make_arg(cx, a))
            .collect();
        return Some(CallHead {
            callee: trimmed(cx, func),
            kind: CallKind::Plain,
            callee_root: root.map(|r| text(cx, r)),
            is_simple_path: zig_is_simple_path(func),
            initializes_binding: zig_initializes_binding(cx, n),
            args,
        });
    }
    if n.kind() != "builtin_function" {
        return None;
    }
    let args_node = first_named_child_matching(n, &["arguments"])?;
    let args = named_children(args_node)
        .into_iter()
        .map(|a| make_arg(cx, a))
        .collect();
    Some(CallHead {
        callee: builtin_name(cx, n),
        kind: CallKind::Macro,
        callee_root: None,
        is_simple_path: false,
        initializes_binding: false,
        args,
    })
}
