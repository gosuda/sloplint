use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, binary_concat, child_by_field, find_map_named_child, first_named_child_matching,
    named_children, narrow, pos_of, string_arg_shape, text, trimmed,
};

use super::syntax::STRING_KINDS;

pub(crate) fn is_subscript(call: Node<'_>) -> bool {
    first_suffix_child(call, "value_arguments")
        .and_then(|list| list.child(0))
        .is_some_and(|open| open.kind() == "[")
}

fn suffix_children<'a>(call: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    named_children(call)
        .into_iter()
        .filter(|c| c.kind() == "call_suffix")
        .flat_map(named_children)
        .filter(|c| c.kind() == kind)
        .collect()
}

fn first_suffix_child<'a>(call: Node<'a>, kind: &str) -> Option<Node<'a>> {
    find_map_named_child(call, |c| {
        if c.kind() != "call_suffix" {
            return None;
        }
        first_named_child_matching(c, &[kind])
    })
}

pub(crate) fn value_arguments(call: Node<'_>) -> Vec<Node<'_>> {
    suffix_children(call, "value_arguments")
        .into_iter()
        .flat_map(named_children)
        .filter(|c| c.kind() == "value_argument")
        .collect()
}

fn first_value_argument(call: Node<'_>) -> Option<Node<'_>> {
    first_suffix_child(call, "value_arguments")
        .and_then(|list| first_named_child_matching(list, &["value_argument"]))
}

fn callee_root<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> Option<&'s str> {
    let mut current = callee;
    loop {
        current = match current.kind() {
            "simple_identifier" => return Some(text(cx, current)),
            "self_expression" => return Some("self"),
            "navigation_expression" => child_by_field(current, "target")?,
            _ => return None,
        };
    }
}

fn swift_is_simple_path(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "simple_identifier" | "self_expression" => return true,
            "navigation_expression" => match child_by_field(current, "target") {
                Some(next) => current = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

fn swift_initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    parent.kind() == "property_declaration"
        && child_by_field(parent, "value").is_some_and(|v| v.id() == n.id())
}

fn shape_of<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "line_string_literal" | "multi_line_string_literal" => {
            string_arg_shape(cx, value, &["line_str_text", "multi_line_str_text"])
        }
        "raw_string_literal" => string_arg_shape(cx, value, &["raw_str_part"]),
        "additive_expression" if binary_concat(cx, value, "+", STRING_KINDS) => ArgShape::Concat,
        "array_literal" | "dictionary_literal" => ArgShape::Collection,
        "lambda_literal" => ArgShape::CodeBody,
        "simple_identifier" => ArgShape::Ident(trimmed(cx, value)),
        _ => ArgShape::Other,
    }
}

fn arg<'s>(shape: ArgShape<'s>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape,
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Vec<Arg<'s>> {
    let positional = value_arguments(call)
        .into_iter()
        .filter_map(|argument| child_by_field(argument, "value"))
        .map(|value| arg(shape_of(cx, value), value));
    let trailing = suffix_children(call, "lambda_literal")
        .into_iter()
        .map(|lambda| arg(ArgShape::CodeBody, lambda));
    positional.chain(trailing).collect()
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if is_subscript(n) {
        return None;
    }
    let callee = n.named_child(0)?;
    Some(CallHead {
        callee: trimmed(cx, callee),
        kind: CallKind::Plain,
        callee_root: callee_root(cx, callee),
        is_simple_path: swift_is_simple_path(callee),
        initializes_binding: swift_initializes_binding(cx, n),
        args: call_args(cx, n),
    })
}

fn xctest_shape(callee: &str) -> Option<(usize, bool)> {
    match callee {
        "XCTAssert" | "XCTAssertTrue" | "XCTAssertFalse" | "XCTAssertNil" | "XCTAssertNotNil" => {
            Some((1, false))
        }
        "XCTAssertEqual"
        | "XCTAssertNotEqual"
        | "XCTAssertGreaterThan"
        | "XCTAssertGreaterThanOrEqual"
        | "XCTAssertLessThan"
        | "XCTAssertLessThanOrEqual" => Some((2, true)),
        "XCTFail" => Some((0, false)),
        _ => None,
    }
}

fn swift_assert_shape(first: Option<Node<'_>>) -> AssertShape {
    match first.map(|v| v.kind()) {
        Some("simple_identifier") => AssertShape::BareName,
        Some("equality_expression") => AssertShape::TopEqNe,
        _ => AssertShape::Other,
    }
}

pub(crate) fn assertion(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = trimmed(cx, n.named_child(0)?);
    let arguments = value_arguments(n);
    let first = first_value_argument(n).and_then(|a| child_by_field(a, "value"));
    let shape = swift_assert_shape(first);
    let (has_message, comparison) = match callee {
        "assert" | "precondition" => {
            let comparison = first.is_some_and(|v| {
                matches!(v.kind(), "comparison_expression" | "equality_expression")
            });
            (arguments.len() >= 2, comparison)
        }
        _ => {
            let (operands, comparison) = xctest_shape(callee)?;
            let positional = arguments
                .iter()
                .filter(|a| child_by_field(**a, "name").is_none())
                .count();
            let labeled = arguments.iter().any(|a| {
                child_by_field(*a, "name")
                    .is_some_and(|label| trimmed(cx, label).trim_end_matches(':') == "message")
            });
            (positional > operands || labeled, comparison)
        }
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

pub(crate) fn dynamic(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let first = first_value_argument(n).and_then(|a| child_by_field(a, "value"))?;
    let message = match trimmed(cx, n.named_child(0)?) {
        "NSSelectorFromString" => "NSSelectorFromString() with computed name",
        "NSClassFromString" => "NSClassFromString() with computed name",
        _ => return None,
    };
    (first.kind() != "line_string_literal").then_some(message)
}
