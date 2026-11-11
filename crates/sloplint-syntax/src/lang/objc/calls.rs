//! ObjC call heads, message targets, and argument shapes.

use sloplint_core::facts::{Arg, ArgShape};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    Cx, Link, binary_concat, child_by_field, first_named_child, named_children, narrow,
    string_arg_shape, text,
};
use crate::lang::c_family::collect_fragments;

const RECEIVER_CHAIN: u8 = 14;
const RECEIVER_IDENT: u8 = 15;

pub(crate) fn msg_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let root = cx.chain_root(RECEIVER_CHAIN, n, receiver_step);
    let leaf = cx.chain_root(RECEIVER_IDENT, n, leaf_step);
    (root == leaf && is_ident_text(root)).then_some(root)
}
fn is_ident_text(root: &str) -> bool {
    let mut chars = root.chars();
    match chars.next() {
        Some(first) if !first.is_ascii_digit() => {}
        _ => return false,
    }
    root.chars()
        .all(|c| !c.is_ascii() || c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
fn receiver_step<'n>(current: Node<'n>) -> Link<'n> {
    if current.kind() == "message_expression" {
        return Link::Root(current);
    }
    if current.kind() == "field_expression"
        && let Some(next) = child_by_field(current, "argument")
    {
        return Link::Next(next);
    }
    Link::Root(current)
}
fn leaf_step<'n>(current: Node<'n>) -> Link<'n> {
    if current.kind() == "message_expression" {
        return Link::Root(current);
    }
    if current.kind() == "field_expression"
        && let Some(next) = child_by_field(current, "argument")
    {
        return Link::Next(next);
    }
    if current.kind() == "identifier" {
        return Link::Root(current);
    }
    match first_named_child(current) {
        Some(inner) => Link::Next(inner),
        None => Link::Root(current),
    }
}
pub(crate) fn first_selector<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut cursor = n.walk();
    n.children_by_field_name("method", &mut cursor)
        .find(|c| c.kind() == "identifier")
        .map(|c| text(cx, c))
}
pub(crate) fn mk_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}
pub(crate) fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    let mut args = Vec::new();
    for c in named_children(args_node) {
        if c.kind() == "comment" {
            continue;
        }
        args.push(mk_arg(cx, c));
    }
    args
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if n.kind() == "at_expression" {
        return match first_named_child(n) {
            Some(found) if found.kind() == "string_literal" => {
                string_arg_shape(cx, found, &["string_content"])
            }
            _ => ArgShape::Other,
        };
    }
    match n.kind() {
        "string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "concatenated_string" => concat_shape(cx, n),
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat {
            longest_static: plus_longest(cx, n),
        },
        "identifier" => ArgShape::Ident(text(cx, n)),
        "compound_literal_expression" => {
            child_by_field(n, "value").map_or(ArgShape::Other, init_arg_shape)
        }
        "initializer_list" => init_arg_shape(n),
        "array_literal" => array_arg_shape(n),
        "dictionary_literal" => ArgShape::Other,
        "block_literal" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}
fn plus_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut best = 0;
    let mut current = n;
    while let (Some(left), Some(right)) = (current.child(0), current.child(2)) {
        best = best.max(operand_longest(cx, right));
        if left.kind() == "binary_expression" {
            current = left;
        } else {
            best = best.max(operand_longest(cx, left));
            break;
        }
    }
    best
}
fn operand_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    if n.kind() == "string_literal" {
        let mut segments = Vec::new();
        collect_fragments(cx, n, &["string_content"], &mut segments);
        return longest_static_chunk(&segments);
    }
    if n.kind() != "concatenated_string" {
        return 0;
    }
    let mut joined = String::new();
    for c in named_children(n) {
        if c.kind() != "string_literal" {
            continue;
        }
        let mut pieces = Vec::new();
        collect_fragments(cx, c, &["string_content"], &mut pieces);
        joined.push_str(&pieces.concat());
    }
    longest_static_chunk(&[joined.as_str()])
}
fn init_arg_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    let mut code = false;
    for c in named_children(n) {
        match c.kind() {
            "comment" => {}
            "initializer_pair" => return ArgShape::Other,
            "block_literal" => code = true,
            "parenthesized_expression"
                if first_named_child(c).is_some_and(|inner| inner.kind() == "block_literal") =>
            {
                code = true;
            }
            _ => {}
        }
    }
    if code {
        return ArgShape::CodeBody;
    }
    ArgShape::Collection
}
fn array_arg_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    for c in named_children(n) {
        if c.kind() == "block_literal" {
            return ArgShape::CodeBody;
        }
    }
    ArgShape::Collection
}
fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let mut runs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut dynamic = false;
    for c in named_children(n) {
        if c.kind() == "comment" {
            continue;
        }
        if c.kind() == "string_literal" {
            let mut pieces = Vec::new();
            collect_fragments(cx, c, &["string_content"], &mut pieces);
            current.push_str(&pieces.concat());
            continue;
        }
        if c.kind() == "identifier" {
            if !current.is_empty() {
                runs.push(current);
                current = String::new();
            }
            dynamic = true;
            continue;
        }
        return ArgShape::Other;
    }
    if !current.is_empty() {
        runs.push(current);
    }
    let longest = runs
        .iter()
        .map(|run| longest_static_chunk(&[run.as_str()]))
        .max()
        .unwrap_or(0);
    if dynamic {
        return ArgShape::Concat {
            longest_static: longest,
        };
    }
    ArgShape::Str {
        longest_static: longest,
    }
}
