//! Cpp call heads, argument shapes, and assertion facts.

use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    Cx, binary_concat, child_by_field, named_children, pos_of, string_arg_shape, text,
};
use crate::lang::c_family::{self, bin_op, collect_fragments, first_arg};

pub(crate) fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "namespace_identifier" | "this" | "base" => {
                return Some(text(cx, current));
            }
            "field_expression" => {
                let next = child_by_field(current, "argument")?;
                current = next
            }
            "member_access_expression" => {
                let next = child_by_field(current, "expression")?;
                current = next
            }
            "qualified_identifier" => {
                if let Some(scope) = child_by_field(current, "scope") {
                    current = scope;
                } else {
                    current = child_by_field(current, "name")?;
                }
            }
            _ => return None,
        }
    }
}

pub(crate) fn is_simple_path(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier"
            | "field_identifier"
            | "namespace_identifier"
            | "type_identifier"
            | "this"
            | "base"
            | "destructor_name" => return true,
            "field_expression" => {
                let Some(next) = child_by_field(current, "argument") else {
                    return false;
                };
                current = next;
            }
            "member_access_expression" => {
                let Some(next) = child_by_field(current, "expression") else {
                    return false;
                };
                current = next;
            }
            "qualified_identifier" => return qualified_ok(current),
            _ => return false,
        }
    }
}

fn qualified_ok(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        if current.kind() != "qualified_identifier" {
            return false;
        }
        if child_by_field(current, "scope")
            .is_some_and(|s| !matches!(s.kind(), "namespace_identifier" | "identifier"))
        {
            return false;
        }
        match child_by_field(current, "name") {
            None => return false,
            Some(found) if found.kind() == "qualified_identifier" => current = found,
            Some(found) => {
                return matches!(
                    found.kind(),
                    "identifier" | "field_identifier" | "type_identifier" | "destructor_name"
                );
            }
        }
    }
}

pub(crate) fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    c_family::call_args(cx, args_node, arg_shape)
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "raw_string_literal" => {
            let mut segments = Vec::new();
            collect_fragments(cx, n, &["raw_string_content"], &mut segments);
            ArgShape::Str {
                longest_static: longest_static_chunk(&segments),
            }
        }
        "concatenated_string" => concat_shape(cx, n),
        "binary_expression"
            if binary_concat(
                cx,
                n,
                "+",
                &[
                    "string_literal",
                    "concatenated_string",
                    "raw_string_literal",
                ],
            ) =>
        {
            plus_shape(cx, n)
        }
        "identifier" => ArgShape::Ident(text(cx, n)),
        "initializer_list" => init_shape(n),
        "lambda_expression" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}

fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if !concat_known(n) {
        return ArgShape::Other;
    }
    let runs = literal_runs(cx, n);
    if runs.is_empty() {
        return ArgShape::Other;
    }
    let refs: Vec<&str> = runs.iter().map(String::as_str).collect();
    let longest = longest_static_chunk(&refs);
    if concat_dynamic(n) {
        ArgShape::Concat {
            longest_static: longest,
        }
    } else {
        ArgShape::Str {
            longest_static: longest,
        }
    }
}

fn concat_known(n: Node<'_>) -> bool {
    named_children(n).iter().all(|c| {
        matches!(
            c.kind(),
            "string_literal" | "raw_string_literal" | "identifier"
        )
    })
}

fn concat_dynamic(n: Node<'_>) -> bool {
    named_children(n).iter().any(|c| c.kind() == "identifier")
}

fn literal_runs(cx: &Cx<'_, '_>, n: Node<'_>) -> Vec<String> {
    let mut runs: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in named_children(n) {
        if c.kind() == "string_literal" || c.kind() == "raw_string_literal" {
            drain_literal(cx, c, &mut current, &mut runs);
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

fn drain_literal(cx: &Cx<'_, '_>, n: Node<'_>, current: &mut String, runs: &mut Vec<String>) {
    for piece in named_children(n) {
        if piece.kind() == "string_content" || piece.kind() == "raw_string_content" {
            current.push_str(text(cx, piece));
        } else if piece.kind() == "escape_sequence" && !current.is_empty() {
            runs.push(std::mem::take(current));
        }
    }
}

fn plus_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let mut best = 0;
    let mut current = n;
    while let (Some(left), Some(op), Some(right)) =
        (current.child(0), current.child(1), current.child(2))
    {
        if current.child_count() != 3 || text(cx, op) != "+" {
            break;
        }
        best = best.max(operand_static(cx, right));
        if left.kind() == "binary_expression" {
            current = left;
        } else {
            best = best.max(operand_static(cx, left));
            break;
        }
    }
    ArgShape::Concat {
        longest_static: best,
    }
}

fn operand_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    match n.kind() {
        "string_literal" => {
            let mut segments = Vec::new();
            collect_fragments(cx, n, &["string_content"], &mut segments);
            longest_static_chunk(&segments)
        }
        "raw_string_literal" => {
            let mut segments = Vec::new();
            collect_fragments(cx, n, &["raw_string_content"], &mut segments);
            longest_static_chunk(&segments)
        }
        "concatenated_string" => {
            let runs = literal_runs(cx, n);
            let refs: Vec<&str> = runs.iter().map(String::as_str).collect();
            longest_static_chunk(&refs)
        }
        _ => 0,
    }
}

pub(crate) fn callee_text<'s>(cx: &Cx<'_, 's>, mut func: Node<'_>) -> &'s str {
    loop {
        match func.kind() {
            "call_expression" => match child_by_field(func, "function") {
                Some(next) => func = next,
                None => return text(cx, func),
            },
            "field_expression" => {
                if receiver_holds_call_or_index(func) {
                    return member_name(cx, func);
                }
                return text(cx, func);
            }
            _ => return text(cx, func),
        }
    }
}

fn receiver_holds_call_or_index(node: Node<'_>) -> bool {
    let mut current = node;
    loop {
        match current.kind() {
            "field_expression" => match child_by_field(current, "argument") {
                Some(next) => current = next,
                None => return false,
            },
            "call_expression" | "subscript_expression" => return true,
            "parenthesized_expression" => match current.named_child(0) {
                Some(next) => current = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

fn member_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    child_by_field(func, "field")
        .map(|name| text(cx, name))
        .unwrap_or_else(|| text(cx, func))
}

fn init_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    let mut code = false;
    for c in named_children(n) {
        if c.kind() == "initializer_pair" {
            return ArgShape::Other;
        }
        if c.kind() == "lambda_expression" {
            code = true;
        }
    }
    if code {
        ArgShape::CodeBody
    } else {
        ArgShape::Collection
    }
}

pub(crate) fn runtime_symbol_lookup(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    let func = child_by_field(n, "function")?;
    let message = match text(cx, func) {
        "dlsym" | "dlvsym" => "runtime symbol lookup (dlsym)",
        "GetProcAddress" => "runtime symbol lookup (GetProcAddress)",
        _ => return None,
    };
    match name_argument(n, 1) {
        Some(found) if plain_name(found) => None,
        _ => Some(message),
    }
}
fn name_argument(n: Node<'_>, index: usize) -> Option<Node<'_>> {
    let args = child_by_field(n, "arguments")?;
    let mut position = 0;
    for c in named_children(args) {
        if c.kind() == "comment" {
            continue;
        }
        if position == index {
            return Some(c);
        }
        position += 1;
    }
    None
}
fn plain_name(n: Node<'_>) -> bool {
    matches!(n.kind(), "string_literal" | "raw_string_literal")
}

pub(crate) fn gtest_assert<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> Assertion {
    let mut has_message = false;
    if let Some(parent) = cx.parent()
        && parent.kind() == "binary_expression"
        && child_by_field(parent, "left").is_some_and(|l| l == n)
        && bin_op(cx, parent).is_some_and(|op| op == "<<")
    {
        has_message = true;
    }
    let suffix = &callee["ASSERT_".len()..];
    let comparison = matches!(suffix, "EQ" | "NE" | "STREQ" | "STRNE");
    Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape: if comparison {
            AssertShape::TopEqNe
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}

pub(crate) fn catch2_assert<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Assertion {
    let arg = first_arg(n).filter(|a| a.kind() == "binary_expression");
    let comparison = arg.is_some_and(|a| bin_op(cx, a).is_some_and(|op| op == "==" || op == "!="));
    Assertion {
        pos: pos_of(n),
        has_message: false,
        comparison,
        shape: if comparison {
            AssertShape::TopEqNe
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}
