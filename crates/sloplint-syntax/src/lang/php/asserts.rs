use sloplint_core::facts::{AssertShape, Assertion};
use tree_sitter::Node;

use super::calls::{ArgsIter, arg_expr, arg_nodes, callee_last, stmt_callee};
use crate::hooks::{Cx, child_by_field, narrow, pos_of, text};

fn two_operand(rest: &str) -> bool {
    matches!(
        rest,
        "Same"
            | "NotSame"
            | "Equals"
            | "NotEquals"
            | "Count"
            | "InstanceOf"
            | "MatchesRegularExpression"
            | "StringContainsString"
    ) || rest.starts_with("GreaterThan")
        || rest.starts_with("LessThan")
        || rest.starts_with("Contains")
}

pub(crate) fn assertion_head(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    let kind = n.kind();
    if kind != "function_call_expression"
        && kind != "member_call_expression"
        && kind != "scoped_call_expression"
        && kind != "nullsafe_member_call_expression"
    {
        return None;
    }
    let (callee, _) = stmt_callee(cx, n);
    let name = callee_last(cx, n, callee);
    if !name.starts_with("assert") {
        return None;
    }
    let two = two_operand(name.strip_prefix("assert").unwrap_or(name));
    if child_by_field(n, "arguments").is_none() {
        let assertion = if two {
            eq_assertion(n)
        } else {
            bare_assertion(n)
        };
        return Some(assertion);
    }
    let mut args = arg_nodes(n);
    let first = args.next();
    let (last, count) = scan_tail(args, first);
    let last_str = last
        .map(arg_expr)
        .is_some_and(|expr| expr.kind() == "string" || expr.kind() == "encapsed_string");
    let shape = if two {
        AssertShape::TopEqNe
    } else {
        assertion_shape(cx, first.map(arg_expr))
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message: count > u32::from(two) + 1 && last_str,
        comparison: two,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

fn eq_assertion(n: Node<'_>) -> Assertion {
    Assertion {
        pos: pos_of(n),
        has_message: false,
        comparison: true,
        shape: AssertShape::TopEqNe,
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}

fn bare_assertion(n: Node<'_>) -> Assertion {
    Assertion {
        pos: pos_of(n),
        has_message: false,
        comparison: false,
        shape: AssertShape::Other,
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}

fn scan_tail<'n>(args: ArgsIter<'n>, first: Option<Node<'n>>) -> (Option<Node<'n>>, u32) {
    let mut last = first;
    let mut count = usize::from(first.is_some());
    for arg in args {
        last = Some(arg);
        count += 1;
    }
    (last, narrow(count))
}

fn assertion_shape(cx: &Cx<'_, '_>, first: Option<Node<'_>>) -> AssertShape {
    if top_eq_ne(cx, first) {
        return AssertShape::TopEqNe;
    }
    if first.is_some_and(|expr| expr.kind() == "variable_name") {
        return AssertShape::BareName;
    }
    AssertShape::Other
}

fn top_eq_ne(cx: &Cx<'_, '_>, first: Option<Node<'_>>) -> bool {
    let Some(expr) = first else {
        return false;
    };
    expr.kind() == "binary_expression"
        && child_by_field(expr, "operator")
            .is_some_and(|op| matches!(text(cx, op), "==" | "!=" | "===" | "!=="))
}
