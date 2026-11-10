use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, binary_concat, child_by_field, first_named_child_matching, first_of_kind,
    has_child_kind, named_children, narrow, string_arg_shape, text, trimmed,
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

fn multiline_static<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> u32 {
    let raw = text(cx, n);
    let clean = |line: &'s str| line.trim_start().strip_prefix("\\\\").unwrap_or(line);
    let parts: Vec<&'s str> = raw.lines().map(clean).collect();
    longest_static_chunk(&parts)
}

fn operand_static<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> u32 {
    match n.kind() {
        "string" => match string_arg_shape(cx, n, &["string_content"]) {
            ArgShape::Str { longest_static } => longest_static,
            _ => 0,
        },
        "multiline_string" => multiline_static(cx, n),
        _ => 0,
    }
}

fn concat_static<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> u32 {
    let mut best = 0;
    let mut current = n;
    loop {
        if child_by_field(current, "operator").is_none_or(|op| text(cx, op) != "++") {
            return best;
        }
        let mut inner = None;
        if let Some(left) = child_by_field(current, "left") {
            if left.kind() == "binary_expression" {
                inner = Some(left);
            } else {
                best = best.max(operand_static(cx, left));
            }
        }
        if let Some(right) = child_by_field(current, "right") {
            best = best.max(operand_static(cx, right));
        }
        let Some(next) = inner else {
            return best;
        };
        current = next;
    }
}

fn init_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    let Some(list) = first_of_kind(n, &["initializer_list"]) else {
        return ArgShape::Other;
    };
    let mut keyed = false;
    let mut code = false;
    for element in named_children(list) {
        if super::items::init_pair(element).is_some() {
            keyed = true;
        } else if element.kind() == "block" || element.kind() == "block_expression" {
            code = true;
        }
    }
    if keyed {
        return ArgShape::Other;
    }
    if code {
        return ArgShape::CodeBody;
    }
    ArgShape::Collection
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "multiline_string" => ArgShape::Str {
            longest_static: multiline_static(cx, n),
        },
        "identifier" => ArgShape::Ident(text(cx, n)),
        "binary_expression" if binary_concat(cx, n, "++", &["string", "multiline_string"]) => {
            ArgShape::Concat {
                longest_static: concat_static(cx, n),
            }
        }
        "struct_initializer" | "anonymous_struct_initializer" => init_shape(n),
        "block" => ArgShape::CodeBody,
        _ => ArgShape::Other,
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

fn path_callee<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    trimmed(cx, func)
}

fn stripped_callee<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if func.kind() == "field_expression"
        && let Some(member) = child_by_field(func, "member")
    {
        return text(cx, member);
    }
    let full = trimmed(cx, func);
    let mut cut = full.len();
    for (idx, byte) in full.bytes().enumerate() {
        if byte == b'(' || byte == b'[' {
            cut = idx;
            break;
        }
    }
    full[..cut].trim_end()
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
    if !has_child_kind(holder, "var") && !has_child_kind(holder, "const") {
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
        let simple = zig_is_simple_path(func);
        let root = if simple { callee_root(func) } else { None };
        let args = positional_nodes(n, func)
            .into_iter()
            .map(|a| make_arg(cx, a))
            .collect();
        let callee = if simple {
            path_callee(cx, func)
        } else {
            stripped_callee(cx, func)
        };
        return Some(CallHead {
            callee,
            kind: CallKind::Plain,
            callee_root: root.map(|r| text(cx, r)),
            is_simple_path: simple,
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
