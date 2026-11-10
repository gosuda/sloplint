use sloplint_core::facts::{Arg, ArgShape};
use tree_sitter::Node;

use crate::hooks::{
    Cx, Link, child_by_field, first_named_child, first_named_child_matching, last_segment, narrow,
    only_named_child_matching, pos_of, string_arg_shape, text,
};
const RUST_CALLEE: u8 = 12;

fn unwrap_parens(n: Node<'_>) -> Node<'_> {
    let mut cur = n;
    while cur.kind() == "parenthesized_expression" {
        let Some(inner) = first_named_child(cur) else {
            return cur;
        };
        cur = inner;
    }
    cur
}

pub(super) fn callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    let cur = unwrap_parens(func);
    if is_simple_path(cur) {
        return text(cx, cur);
    }
    match cur.kind() {
        "field_expression" => child_by_field(cur, "field").map_or("", |field| text(cx, field)),
        "scoped_identifier" => child_by_field(cur, "name").map_or("", |name| text(cx, name)),
        _ => "",
    }
}

pub(super) fn is_simple_path(n: Node<'_>) -> bool {
    const PATH_LINK: [&str; 6] = [
        "identifier",
        "field_expression",
        "scoped_identifier",
        "self",
        "super",
        "crate",
    ];
    let mut cur = n;
    loop {
        cur = unwrap_parens(cur);
        match cur.kind() {
            "identifier" | "self" | "super" | "crate" => return true,
            "field_expression" => {
                let Some(value) = child_by_field(cur, "value") else {
                    return false;
                };
                if !PATH_LINK.contains(&value.kind()) {
                    return false;
                }
                cur = value;
            }
            "scoped_identifier" => {
                let Some(path) = child_by_field(cur, "path") else {
                    return false;
                };
                if !PATH_LINK.contains(&path.kind()) {
                    return false;
                }
                cur = path;
            }
            "generic_function" => {
                let Some(func) = child_by_field(cur, "function") else {
                    return false;
                };
                cur = func;
            }
            _ => return false,
        }
    }
}

pub(super) fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if !is_simple_path(n) {
        return None;
    }
    let root = cx.chain_root(RUST_CALLEE, n, |cur| match cur.kind() {
        "field_expression" => child_by_field(cur, "value").map_or(Link::Root(cur), Link::Next),
        "scoped_identifier" => child_by_field(cur, "path").map_or(Link::Root(cur), Link::Next),
        "parenthesized_expression" => first_named_child(cur).map_or(Link::Root(cur), Link::Next),
        "generic_function" => child_by_field(cur, "function").map_or(Link::Root(cur), Link::Next),
        _ => Link::Root(cur),
    });
    (!root.is_empty()).then_some(root)
}
pub(super) fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    let value_is_call = child_by_field(parent, "value").is_some_and(|value| value.id() == n.id());
    if !value_is_call {
        return false;
    }
    match parent.kind() {
        "let_declaration" => is_named_binding(parent),
        "const_item" | "static_item" => true,
        _ => false,
    }
}

fn is_named_binding(parent: Node<'_>) -> bool {
    let Some(pattern) = child_by_field(parent, "pattern") else {
        return false;
    };
    match pattern.kind() {
        "identifier" => true,
        "mut_pattern" => first_named_child_matching(pattern, &["identifier"]).is_some(),
        _ => false,
    }
}

fn unwrap_call(n: Node<'_>) -> Option<Node<'_>> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "call_expression" | "macro_invocation" => return Some(cur),
            "expression_statement"
            | "return_expression"
            | "try_expression"
            | "await_expression" => {
                cur = first_named_child(cur)?;
            }
            _ => return None,
        }
    }
}

pub(super) fn sole_call_id(body: Node<'_>) -> Option<usize> {
    let only = only_named_child_matching(body, |c| {
        c.kind() != "line_comment" && c.kind() != "block_comment"
    })?;
    unwrap_call(only).map(|c| c.id())
}

fn token_opener_shape<'s>(cx: &Cx<'_, 's>, tt: Node<'_>) -> ArgShape<'s> {
    let mut cursor = tt.walk();
    let opener = tt
        .children(&mut cursor)
        .filter(|c| !c.is_named())
        .map(|c| text(cx, c))
        .find(|t| matches!(*t, "(" | "[" | "{"));
    match opener {
        Some("[") => ArgShape::Collection,
        Some("{") => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}

fn is_format_macro(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mac = child_by_field(n, "macro").map_or("", |m| text(cx, m));
    matches!(last_segment(mac, &["::"]), "format" | "format_args")
}

fn str_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    match string_arg_shape(cx, n, &["string_content"]) {
        ArgShape::Str { longest_static } => longest_static,
        _ => 0,
    }
}

fn track_static(best: Option<u32>, len: u32) -> Option<u32> {
    Some(match best {
        Some(current) => current.max(len),
        None => len,
    })
}

fn first_group_static_len(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    let tt = first_named_child_matching(n, &["token_tree"])?;
    let mut best = None;
    let mut cursor = tt.walk();
    for child in tt.children(&mut cursor) {
        if !child.is_named() && text(cx, child) == "," {
            return best;
        }
        if matches!(child.kind(), "string_literal" | "raw_string_literal") {
            best = track_static(best, str_longest(cx, child));
        }
    }
    best
}

fn macro_body_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if is_format_macro(cx, n)
        && let Some(len) = first_group_static_len(cx, n)
    {
        return ArgShape::Concat {
            longest_static: len,
        };
    }
    let Some(tt) = first_named_child_matching(n, &["token_tree"]) else {
        return ArgShape::Other;
    };
    token_opener_shape(cx, tt)
}

fn collection_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    let mut cursor = n.walk();
    let holds_code = n
        .children(&mut cursor)
        .any(|c| matches!(c.kind(), "closure_expression" | "block"));
    if holds_code {
        ArgShape::CodeBody
    } else {
        ArgShape::Collection
    }
}

fn concat_static_len(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    const STR_KINDS: [&str; 2] = ["string_literal", "raw_string_literal"];
    let mut best = None;
    let mut pending = vec![n];
    while let Some(cur) = pending.pop() {
        if cur.kind() == "binary_expression"
            && child_by_field(cur, "operator").is_some_and(|op| text(cx, op) == "+")
        {
            pending.extend(
                [child_by_field(cur, "left"), child_by_field(cur, "right")]
                    .into_iter()
                    .flatten(),
            );
        } else if STR_KINDS.contains(&cur.kind()) {
            best = track_static(best, str_longest(cx, cur));
        }
    }
    best
}

fn shaped_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" | "raw_string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        "array_expression" | "tuple_expression" => collection_shape(n),
        "reference_expression" => ref_arg_shape(cx, n),
        "closure_expression" | "block" | "match_expression" | "if_expression"
        | "for_expression" | "while_expression" | "loop_expression" | "async_block"
        | "unsafe_block" | "const_block" => ArgShape::CodeBody,
        "macro_invocation" => macro_body_shape(cx, n),
        "call_expression" => conversion_shape(cx, n),
        _ => match concat_static_len(cx, n) {
            Some(len) => ArgShape::Concat {
                longest_static: len,
            },
            None => ArgShape::Other,
        },
    }
}

fn ref_arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let Some(inner) = child_by_field(n, "value") else {
        return ArgShape::Other;
    };
    match inner.kind() {
        "array_expression" | "tuple_expression" => collection_shape(inner),
        "string_literal" | "raw_string_literal" => string_arg_shape(cx, inner, &["string_content"]),
        "macro_invocation" => macro_body_shape(cx, inner),
        _ => ArgShape::Other,
    }
}

fn receiver_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" | "raw_string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "macro_invocation" if is_format_macro(cx, n) => match first_group_static_len(cx, n) {
            Some(len) => ArgShape::Concat {
                longest_static: len,
            },
            None => ArgShape::Other,
        },
        _ => ArgShape::Other,
    }
}

fn conversion_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let Some(func) = child_by_field(n, "function") else {
        return ArgShape::Other;
    };
    let Some(args) = child_by_field(n, "arguments") else {
        return ArgShape::Other;
    };
    match func.kind() {
        "field_expression" => {
            let name = child_by_field(func, "field").map_or("", |f| text(cx, f));
            if !matches!(name, "to_string" | "to_owned" | "into" | "as_str") {
                return ArgShape::Other;
            }
            let Some(recv) = child_by_field(func, "value") else {
                return ArgShape::Other;
            };
            receiver_shape(cx, recv)
        }
        "scoped_identifier" => {
            let name = child_by_field(func, "name").map_or("", |f| text(cx, f));
            let path = child_by_field(func, "path").map_or("", |p| text(cx, p));
            if name != "from" || path != "String" {
                return ArgShape::Other;
            }
            let Some(first) = first_named_child(args) else {
                return ArgShape::Other;
            };
            receiver_shape(cx, first)
        }
        _ => ArgShape::Other,
    }
}

fn has_field_assign(cx: &Cx<'_, '_>, group: &[Node<'_>]) -> bool {
    group.iter().any(|n| !n.is_named() && text(cx, *n) == "=")
}

fn longest_str_shape<'s>(cx: &Cx<'_, 's>, group: &[Node<'_>]) -> Option<ArgShape<'s>> {
    let mut best = 0;
    let mut found = false;
    for n in group
        .iter()
        .filter(|n| matches!(n.kind(), "string_literal" | "raw_string_literal"))
    {
        found = true;
        if let ArgShape::Str { longest_static } = string_arg_shape(cx, *n, &["string_content"]) {
            best = best.max(longest_static);
        }
    }
    found.then_some(ArgShape::Str {
        longest_static: best,
    })
}

fn single_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" | "raw_string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        "token_tree" => token_opener_shape(cx, n),
        _ => ArgShape::Other,
    }
}

fn group_shape<'s>(cx: &Cx<'_, 's>, group: &[Node<'_>]) -> ArgShape<'s> {
    if has_field_assign(cx, group) {
        return ArgShape::Other;
    }
    let mut named = group.iter().filter(|n| n.is_named());
    let Some(first) = named.next() else {
        return ArgShape::Other;
    };
    if named.next().is_some() {
        return longest_str_shape(cx, group).unwrap_or(ArgShape::Other);
    }
    single_shape(cx, *first)
}

pub(super) fn macro_args<'s>(cx: &Cx<'_, 's>, tt: Node<'_>) -> Vec<Arg<'s>> {
    let mut groups: Vec<Vec<Node<'_>>> = vec![Vec::new()];
    let mut cursor = tt.walk();
    for child in tt.children(&mut cursor) {
        if matches!(child.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if !child.is_named() {
            let t = text(cx, child);
            if t == "," {
                groups.push(Vec::new());
                continue;
            }
            if matches!(t, "(" | ")" | "[" | "]" | "{" | "}") {
                continue;
            }
        }
        if let Some(last) = groups.last_mut() {
            last.push(child);
        }
    }
    if groups
        .first()
        .is_some_and(|g| g.len() >= 2 && text(cx, g[0]) == "target" && text(cx, g[1]) == ":")
    {
        groups.remove(0);
    }
    groups
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(|g| {
            let start = g.first().map_or(0, |n| pos_of(*n).line);
            let end = g.last().map_or(0, |n| narrow(n.end_position().row) + 1);
            Arg {
                shape: group_shape(cx, &g),
                start_line: start,
                end_line: end,
            }
        })
        .collect()
}

pub(super) fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    child_by_field(n, "arguments").map_or(Vec::new(), |a| {
        let mut cursor = a.walk();
        a.children(&mut cursor)
            .filter(|c| c.is_named() && !matches!(c.kind(), "line_comment" | "block_comment"))
            .map(|c| shaped_arg(cx, c))
            .collect()
    })
}
