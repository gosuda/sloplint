//! Shared tree shapes for the Dafny adapter: transparent expression
//! wrappers, callee and pattern helpers, call-argument shapes, and the
//! single-call delegation probe.

use sloplint_core::facts::{Arg, ArgShape, Pos, ScrutineeKind};
use tree_sitter::Node;

use crate::hooks::{Cx, first_named_child_matching, named_children, narrow, pos_of, text};

pub(crate) const COMMENT_KINDS: &[&str] = &["line_comment", "block_comment"];

/// Transparent expression wrappers between a use site and its head.
/// Each carries its operands as named children; the leftmost one keeps the
/// callee, scrutinee, or asserted value.
pub(crate) const CHAIN_KINDS: &[&str] = &[
    "equiv_expression",
    "implies_explies_expression",
    "implies_expression",
    "logical_expression",
    "relational_expression",
    "shift_term",
    "term",
    "factor",
    "bitvector_factor",
    "as_expression",
    "unary_expression",
    "parenthesized_expression",
    "generic_expression",
    "literal_expression",
];

const TYPE_SUFFIX: &str = "_type";

pub(crate) fn is_type_kind(kind: &str) -> bool {
    kind == "named_type" || kind.ends_with(TYPE_SUFFIX)
}

pub(crate) fn unwrap_expr(mut node: Node<'_>) -> Node<'_> {
    loop {
        if !CHAIN_KINDS.contains(&node.kind()) {
            return node;
        }
        let Some(first) = node.named_child(0) else {
            return node;
        };
        node = first;
    }
}

pub(crate) fn leftmost_ident<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = node;
    loop {
        match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "name_segment" => {
                let inner = first_named_child_matching(current, &["identifier"])?;
                return Some((text(cx, inner), pos_of(inner)));
            }
            "qualified_name" => {
                let inner = first_named_child_matching(current, &["identifier"])?;
                return Some((text(cx, inner), pos_of(inner)));
            }
            _ if CHAIN_KINDS.contains(&current.kind()) => {
                current = current.named_child(0)?;
            }
            "suffix_expression" => {
                current = current.named_child(0)?;
            }
            _ => return None,
        }
    }
}

pub(crate) fn last_ident<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> Option<&'s str> {
    let mut found = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            found = Some(text(cx, child));
        }
    }
    found
}

/// Split a callee expression into its head name, its root segment, and
/// whether it is a plain path rather than a computed lookup.
pub(crate) fn callee_parts<'s>(
    cx: &Cx<'_, 's>,
    node: Node<'_>,
) -> (&'s str, Option<&'s str>, bool) {
    let mut current = node;
    loop {
        match current.kind() {
            "identifier" => {
                let name = text(cx, current);
                return (name, Some(name), true);
            }
            "name_segment" => {
                let Some(inner) = first_named_child_matching(current, &["identifier"]) else {
                    return (text(cx, current), None, false);
                };
                let name = text(cx, inner);
                return (name, Some(name), true);
            }
            "qualified_name" => {
                let mut cursor = current.walk();
                let mut first = None;
                let mut last = None;
                for child in current.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = text(cx, child);
                        if first.is_none() {
                            first = Some(name);
                        }
                        last = Some(name);
                    }
                }
                return (last.unwrap_or(""), first, true);
            }
            "qualified_member_expression" => {
                let name = last_ident(cx, current).unwrap_or("");
                let root = first_named_child_matching(current, &["qualified_name"])
                    .and_then(|q| leftmost_ident(cx, q))
                    .map(|(root, _)| root);
                return (name, root, true);
            }
            "generic_expression" => {
                let Some(inner) = current.named_child(0) else {
                    return (text(cx, current), None, false);
                };
                current = inner;
            }
            "suffix_expression" => {
                // `recv.member`: the member is the trailing identifier child.
                let mut cursor = current.walk();
                let mut trailing = None;
                for child in current.children(&mut cursor) {
                    if child.is_named() && child.kind() == "identifier" {
                        trailing = Some(child);
                    }
                }
                if let Some(member) = trailing {
                    let root = current
                        .named_child(0)
                        .and_then(|recv| leftmost_ident(cx, recv))
                        .map(|(root, _)| root);
                    return (text(cx, member), root, true);
                }
                let Some(recv) = current.named_child(0) else {
                    return (text(cx, current), None, false);
                };
                current = recv;
            }
            _ if CHAIN_KINDS.contains(&current.kind()) => {
                let Some(first) = current.named_child(0) else {
                    return (text(cx, current), None, false);
                };
                current = first;
            }
            _ => return (text(cx, current), None, false),
        }
    }
}

pub(crate) fn has_actual_binding(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        // `f()` carries no `actual_binding` node — its parens are anonymous
        // children — but an application is still a call. A `(` right after
        // `.` is a datatype update `p.(x := 1)`, not a call.
        (child.is_named() && child.kind() == "actual_binding")
            || (child.kind() == "(" && child.prev_sibling().is_none_or(|prev| prev.kind() != "."))
    })
}

pub(crate) fn pattern_names<'s>(cx: &Cx<'_, 's>, pattern: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    match pattern.kind() {
        "id_pattern" => {
            if let Some(id) = first_named_child_matching(pattern, &["identifier"]) {
                out.push((text(cx, id), pos_of(id)));
            }
        }
        "case_pattern" if pattern.named_child_count() == 1 => {
            // `var (a, b)` wraps each element in a `case_pattern`; a lone
            // identifier inside is a bound name, not a constructor.
            if let Some(id) = first_named_child_matching(pattern, &["identifier"]) {
                out.push((text(cx, id), pos_of(id)));
            } else if let Some(inner) = pattern.named_child(0) {
                pattern_names(cx, inner, out);
            }
        }
        "constructor_pattern" | "tuple_pattern" | "disjunctive_pattern" | "case_pattern" => {
            for child in named_children(pattern) {
                if child.kind() == "identifier" || child.kind() == "integer_literal" {
                    continue;
                }
                pattern_names(cx, child, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn wildcard_pattern(cx: &Cx<'_, '_>, pattern: Node<'_>) -> bool {
    if pattern.kind() != "id_pattern" {
        return false;
    }
    // Only `_` is unambiguously a wildcard: a bare name may equally be a
    // nullary datatype constructor, and flagging it would punish explicit
    // variant matches. A typed `id_pattern` only matches its type.
    if named_children(pattern)
        .iter()
        .any(|child| is_type_kind(child.kind()))
    {
        return false;
    }
    first_named_child_matching(pattern, &["identifier"]).is_some_and(|id| text(cx, id) == "_")
}

/// A wrapper that passes its operand through unchanged: a precedence
/// chain node carrying exactly one non-comment named child whose first
/// child is that operand, or parentheses. `-(x)` leads with the `-`
/// operator — the value is computed, so the wrapper is not transparent.
fn transparent_wrap(node: Node<'_>) -> bool {
    node.kind() == "parenthesized_expression"
        || (first_operand(node).is_some()
            && node
                .named_children(&mut node.walk())
                .filter(|c| !COMMENT_KINDS.contains(&c.kind()))
                .count()
                == 1
            && node.child(0).is_some_and(|c| c.is_named()))
}

/// The first named child that is not a comment — `(/* note */ x)` still
/// yields `x`.
fn first_operand(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|c| !COMMENT_KINDS.contains(&c.kind()))
}

pub(crate) fn scrutinee_kind(cx: &Cx<'_, '_>, scrutinee: Node<'_>) -> ScrutineeKind {
    let mut current = scrutinee;
    loop {
        match current.kind() {
            "identifier" | "name_segment" => return ScrutineeKind::Ident,
            "qualified_name" => return ScrutineeKind::Path,
            "suffix_expression" => {
                if has_actual_binding(current) {
                    return ScrutineeKind::Other;
                }
                // The member read must hang off a bare name or `this`:
                // `-p.x` parses as `(-p).x` — a field of a computed base,
                // not a field of the scrutinee value.
                let mut base = first_operand(current);
                loop {
                    let Some(node) = base else {
                        return ScrutineeKind::Other;
                    };
                    match node.kind() {
                        "identifier" | "name_segment" | "qualified_name" | "this_expression" => {
                            break
                        }
                        "suffix_expression" => base = first_operand(node),
                        _ if CHAIN_KINDS.contains(&node.kind()) && transparent_wrap(node) => {
                            base = first_operand(node);
                        }
                        _ => return ScrutineeKind::Other,
                    }
                }
                let mut cursor = current.walk();
                let mut trailing_ident = false;
                for child in current.children(&mut cursor) {
                    // A field name follows `.`; the update target in
                    // `p.(x := 1)` follows `(` and is not a field access.
                    // `obj./* note */state` parks a comment between the
                    // dot and the name — step back over comments.
                    if child.is_named() && child.kind() == "identifier" {
                        let mut prev = child.prev_sibling();
                        while let Some(node) = prev {
                            if COMMENT_KINDS.contains(&node.kind()) {
                                prev = node.prev_sibling();
                            } else {
                                break;
                            }
                        }
                        if prev.is_some_and(|node| node.kind() == ".") {
                            trailing_ident = true;
                        }
                    }
                }
                return if trailing_ident {
                    ScrutineeKind::Field
                } else {
                    ScrutineeKind::Other
                };
            }
            _ if CHAIN_KINDS.contains(&current.kind()) => {
                if !transparent_wrap(current) {
                    return ScrutineeKind::Other;
                }
                let Some(first) = first_operand(current) else {
                    return ScrutineeKind::Other;
                };
                current = first;
            }
            _ => {
                let _ = cx;
                return ScrutineeKind::Other;
            }
        }
    }
}

pub(crate) fn strip_string_quotes(literal: &str) -> &str {
    let inner = literal.strip_prefix('@').unwrap_or(literal);
    if inner.len() >= 2 {
        &inner[1..inner.len() - 1]
    } else {
        ""
    }
}

fn string_inner_len(node_text: &str) -> u32 {
    let inner = node_text.strip_prefix('@').unwrap_or(node_text);
    narrow(inner.len().saturating_sub(2))
}

fn static_run_len(cx: &Cx<'_, '_>, node: Node<'_>) -> u32 {
    let unwrapped = unwrap_expr(node);
    if unwrapped.kind() == "string_literal" {
        string_inner_len(text(cx, unwrapped))
    } else {
        0
    }
}

/// The longest static run of a `+` string concatenation, if the value joins
/// at least one string literal with `+` at its top level.
fn concat_static_len(cx: &Cx<'_, '_>, value: Node<'_>) -> Option<u32> {
    let mut current = value;
    loop {
        if current.kind() == "term" {
            let mut plus = false;
            let mut longest = 0;
            let mut cursor = current.walk();
            for child in current.children(&mut cursor) {
                if child.is_named() {
                    longest = longest.max(static_run_len(cx, child));
                } else if text(cx, child) == "+" {
                    plus = true;
                }
            }
            return (plus && longest > 0).then_some(longest);
        }
        if CHAIN_KINDS.contains(&current.kind()) {
            current = current.named_child(0)?;
        } else {
            return None;
        }
    }
}

pub(crate) fn arg_shape<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    if let Some(longest) = concat_static_len(cx, value) {
        return ArgShape::Concat {
            longest_static: longest,
        };
    }
    let unwrapped = unwrap_expr(value);
    match unwrapped.kind() {
        "string_literal" => ArgShape::Str {
            longest_static: string_inner_len(text(cx, unwrapped)),
        },
        "identifier" => ArgShape::Ident(text(cx, unwrapped)),
        "name_segment" => ArgShape::Ident(
            first_named_child_matching(unwrapped, &["identifier"])
                .map(|id| text(cx, id))
                .unwrap_or(""),
        ),
        "lambda_expression" => ArgShape::CodeBody,
        "set_display"
        | "multiset_display"
        | "seq_display"
        | "map_display"
        | "set_comprehension"
        | "map_comprehension"
        | "seq_construction"
        | "multiset_construction" => ArgShape::Collection,
        _ => ArgShape::Other,
    }
}

pub(crate) fn arg_of<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> Arg<'s> {
    // `name := value` bindings carry the value as their last named child.
    let value = node
        .named_child(narrow(node.named_child_count().saturating_sub(1)))
        .unwrap_or(node);
    let shape = arg_shape(cx, value);
    Arg {
        shape,
        start_line: pos_of(node).line,
        end_line: narrow(node.end_position().row) + 1,
    }
}

pub(crate) fn call_args<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Vec<Arg<'s>> {
    named_children(call)
        .into_iter()
        .filter(|child| child.kind() == "actual_binding")
        .map(|binding| arg_of(cx, binding))
        .collect()
}

/// The call node id a single-expression body delegates to, when the body is
/// exactly one call. Block bodies need exactly one code statement.
pub(crate) fn sole_call_in(node: Node<'_>) -> Option<usize> {
    // Descend from a declaration to its body first.
    let mut current = match node.kind() {
        "method_decl" | "constructor_decl" | "iterator_decl" => {
            first_named_child_matching(node, &["block_statement"])?
        }
        "function_decl" => first_named_child_matching(node, &["function_body"])?,
        "lambda_expression" => {
            node.named_child(narrow(node.named_child_count().saturating_sub(1)))?
        }
        _ => node,
    };
    loop {
        match current.kind() {
            "call_expression" => return Some(current.id()),
            "suffix_expression" if has_actual_binding(current) => return Some(current.id()),
            "function_body" => {
                current = current.named_child(0)?;
            }
            "block_statement" | "if_alternative_block" => {
                let mut code = named_children(current)
                    .into_iter()
                    .filter(|child| !COMMENT_KINDS.contains(&child.kind()));
                let only = code.next()?;
                if code.next().is_some() {
                    return None;
                }
                current = only;
            }
            "expression_statement" => {
                current = current.named_child(0)?;
            }
            _ if CHAIN_KINDS.contains(&current.kind()) => {
                current = current.named_child(0)?;
            }
            _ => return None,
        }
    }
}
