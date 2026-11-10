//! Java call heads, argument shapes, dynamic uses, and assertions.

use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use tree_sitter::Node;

use super::{COMMENTS, REFLECTION, code_children};
use crate::hooks::{
    CallHead, Cx, Link, child_by_field, count_named_child, find_named_child,
    first_named_child_matching, named_children, narrow, pos_of, rfind_named_child, slice,
    string_arg_shape, text,
};

fn arguments_of(n: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(n, "arguments").map_or_else(Vec::new, code_children)
}

fn chain_has_complex(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "this" | "super" => return false,
            "field_access" => match child_by_field(current, "object") {
                Some(next) => current = next,
                None => return false,
            },
            _ => return true,
        }
    }
}

fn creation_name<'s>(cx: &Cx<'_, 's>, ty: Node<'_>) -> &'s str {
    let mut current = ty;
    loop {
        match current.kind() {
            "generic_type" => {
                if let Some(inner) = first_named_child_matching(current, &["type_identifier"]) {
                    return text(cx, inner);
                }
                match first_named_child_matching(current, &["scoped_type_identifier"]) {
                    Some(scoped) => current = scoped,
                    None => return text(cx, ty),
                }
            }
            "scoped_type_identifier" => {
                if let Some(inner) =
                    rfind_named_child(current, |child| child.kind() == "type_identifier")
                {
                    return text(cx, inner);
                }
                match rfind_named_child(current, |child| child.kind() == "generic_type") {
                    Some(generic) => current = generic,
                    None => return text(cx, ty),
                }
            }
            _ => return text(cx, current),
        }
    }
}

pub(crate) fn method_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let name = child_by_field(n, "name")?;
    match child_by_field(n, "object") {
        Some(object) if chain_has_complex(object) => Some(text(cx, name)),
        Some(object) => Some(slice(cx.src, object.start_byte(), name.end_byte())),
        None => Some(text(cx, name)),
    }
}

fn float_literal(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        if current.kind() == "decimal_floating_point_literal" {
            return true;
        }
        if current.kind() != "unary_expression" {
            return false;
        }
        if !child_by_field(current, "operator").is_some_and(|op| op.kind() == "-") {
            return false;
        }
        match child_by_field(current, "operand") {
            Some(next) => current = next,
            None => return false,
        }
    }
}

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
    }
}

const RECEIVER_CHAIN: u8 = 2;
fn is_chain_root(t: &str) -> bool {
    !matches!(t, "true" | "false" | "null")
        && !t.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || !b.is_ascii())
}

fn receiver_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "identifier" | "this" | "super" | "field_access" | "method_invocation" => {}
        _ => return None,
    }
    let root = cx.chain_root(RECEIVER_CHAIN, n, |current| match current.kind() {
        "field_access" | "method_invocation" => match child_by_field(current, "object") {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        },
        _ => Link::Root(current),
    });
    is_chain_root(root).then_some(root)
}

fn str_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    match string_arg_shape(cx, n, &["string_fragment", "multiline_string_fragment"]) {
        ArgShape::Str { longest_static } => longest_static,
        _ => 0,
    }
}

fn concat_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    let mut best = 0;
    let mut found = false;
    let mut pending = vec![n];
    while let Some(current) = pending.pop() {
        match current.kind() {
            "string_literal" => {
                found = true;
                best = best.max(str_static(cx, current));
            }
            "parenthesized_expression" => pending.extend(named_children(current)),
            "binary_expression" => {
                let op = current.child(1);
                if current.child_count() != 3 || op.is_none_or(|o| text(cx, o) != "+") {
                    continue;
                }
                pending.extend(named_children(current));
            }
            _ => {}
        }
    }
    found.then_some(best)
}

fn initializer_has_code(n: Node<'_>) -> bool {
    let mut pending = vec![n];
    while let Some(current) = pending.pop() {
        match current.kind() {
            "lambda_expression" => return true,
            "array_initializer" => pending.extend(named_children(current)),
            _ => {}
        }
    }
    false
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => {
            string_arg_shape(cx, n, &["string_fragment", "multiline_string_fragment"])
        }
        "binary_expression" => match concat_longest(cx, n) {
            Some(longest_static) => ArgShape::Concat { longest_static },
            None => ArgShape::Other,
        },
        "identifier" => ArgShape::Ident(text(cx, n)),
        "lambda_expression" => ArgShape::CodeBody,
        "array_creation_expression"
            if child_by_field(n, "value")
                .is_some_and(|value| value.kind() == "array_initializer") =>
        {
            let value = child_by_field(n, "value");
            if value.is_some_and(initializer_has_code) {
                ArgShape::CodeBody
            } else {
                ArgShape::Collection
            }
        }
        _ => ArgShape::Other,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arguments_of(n)
        .into_iter()
        .map(|arg| span_arg(arg, arg_shape(cx, arg)))
        .collect()
}

fn declares_value(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    cx.parent()
        .filter(|parent| parent.kind() == "variable_declarator")
        .and_then(|parent| child_by_field(parent, "value"))
        .is_some_and(|value| value.id() == n.id())
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "method_invocation" => {
            let name = child_by_field(n, "name")?;
            let object = child_by_field(n, "object");
            let simple = object.is_none_or(|found| !chain_has_complex(found));
            let callee_root = match object {
                Some(found) if chain_has_complex(found) => None,
                Some(found) => receiver_root(cx, found),
                None => Some(text(cx, name)),
            };
            Some(CallHead {
                callee: method_callee(cx, n)?,
                kind: CallKind::Plain,
                callee_root,
                is_simple_path: simple,
                initializes_binding: declares_value(cx, n),
                args: call_args(cx, n),
            })
        }
        "object_creation_expression" => {
            let ty = child_by_field(n, "type")?;
            let name = creation_name(cx, ty);
            let root = is_chain_root(name).then_some(name);
            Some(CallHead {
                callee: name,
                kind: CallKind::New,
                callee_root: root,
                is_simple_path: true,
                initializes_binding: declares_value(cx, n),
                args: call_args(cx, n),
            })
        }
        _ => None,
    }
}

pub(crate) fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "method_invocation" {
        return None;
    }
    let name = text(cx, child_by_field(n, "name")?);
    let on_class = child_by_field(n, "object").is_some_and(|object| text(cx, object) == "Class");
    if !((name == "forName" && on_class) || REFLECTION.contains(&name)) {
        return None;
    }
    let args = child_by_field(n, "arguments")?;
    let mut cursor = args.walk();
    let first = args
        .named_children(&mut cursor)
        .find(|child| !COMMENTS.contains(&child.kind()))?;
    (first.kind() != "string_literal").then_some("reflection by name")
}

fn string_valued(n: Node<'_>) -> bool {
    let mut pending = vec![n];
    while let Some(current) = pending.pop() {
        match current.kind() {
            "string_literal" => return true,
            "parenthesized_expression" => pending.extend(named_children(current)),
            "binary_expression"
                if child_by_field(current, "operator").is_some_and(|op| op.kind() == "+") =>
            {
                pending.extend(
                    [
                        child_by_field(current, "left"),
                        child_by_field(current, "right"),
                    ]
                    .into_iter()
                    .flatten(),
                );
            }
            _ => {}
        }
    }
    false
}

fn equality_has_message(args: &[Node<'_>]) -> bool {
    match args {
        [first, _, third] => string_valued(*first) || !float_literal(*third),
        [_, _, _, _, ..] => true,
        _ => false,
    }
}

fn method_assertion(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    let name = text(cx, child_by_field(n, "name")?);
    let args = arguments_of(n);
    let count = args.len();
    let (has_message, comparison) = match name {
        "assertTrue" | "assertFalse" | "assertNull" | "assertNotNull" | "assertDoesNotThrow" => {
            (count > 1, false)
        }
        "fail" => (count > 0, false),
        "assertThrows" | "assertThat" => (count > 2, false),
        "assertSame" | "assertNotSame" => (count > 2, true),
        "assertEquals" | "assertNotEquals" | "assertArrayEquals" | "assertIterableEquals" => {
            (equality_has_message(&args), true)
        }
        _ => return None,
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape: if comparison {
            AssertShape::TopEqNe
        } else if args.first().is_some_and(|arg| arg.kind() == "identifier") {
            AssertShape::BareName
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

fn assert_statement(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let first = find_named_child(n, |child| child.kind() != "comment");
    let comparison = first.is_some_and(|node| {
        node.kind() == "binary_expression"
            && child_by_field(node, "operator")
                .is_some_and(|op| matches!(text(cx, op), "==" | "!="))
    });
    let shape = if comparison {
        AssertShape::TopEqNe
    } else if first.is_some_and(|node| node.kind() == "identifier") {
        AssertShape::BareName
    } else {
        AssertShape::Other
    };
    Assertion {
        pos: pos_of(n),
        has_message: count_named_child(n, |child| child.kind() != "comment") >= 2,
        comparison,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}

pub(crate) fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    match n.kind() {
        "assert_statement" => Some(assert_statement(cx, n)),
        "method_invocation" => method_assertion(cx, n),
        _ => None,
    }
}
