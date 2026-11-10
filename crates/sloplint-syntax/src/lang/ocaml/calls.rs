use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind, Pos, Stmt};
use tree_sitter::Node;

use super::funcs::pattern_ident;
use crate::hooks::{
    CallHead, Cx, any_named_child, child_by_field, first_named_child, first_named_child_matching,
    first_of_kind, has_child_kind, last_named_child, named_children, narrow, pos_of,
    string_arg_shape, text, trimmed,
};

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        let kind = current.kind();
        if kind == "identifier" {
            return Some((text(cx, current), pos_of(current)));
        } else if kind == "field_get_expression" {
            current = child_by_field(current, "record")?;
        } else if kind == "parenthesized_expression" {
            current = child_by_field(current, "expression")?;
        } else if kind == "value_path" {
            if has_child_kind(current, "module_path") {
                return None;
            }
            let name = first_of_kind(current, &["value_name"])?;
            return Some((text(cx, name), pos_of(name)));
        } else {
            return None;
        }
    }
}

pub(crate) fn infix_write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let assign = child_by_field(n, "operator");
    if !assign.is_some_and(|op| op.kind() == "assign_operator") {
        return None;
    }
    write_root(cx, child_by_field(n, "left")?)
}

fn is_incr_callee(cx: &Cx<'_, '_>, f: Node<'_>) -> bool {
    text(cx, f) == "incr" || text(cx, f) == "decr"
}

fn is_incr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "function").is_some_and(|f| is_incr_callee(cx, f))
}

pub(crate) fn write_roots_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "infix_expression" {
        if let Some(root) = infix_write_root(cx, n) {
            out.push(root);
        }
        return;
    }
    if n.kind() != "application_expression" {
        return;
    }
    if !is_incr(cx, n) {
        return;
    }
    let Some(arg) = first_named_child_matching(n, &["argument"]) else {
        return;
    };
    let target = first_named_child(arg).unwrap_or(arg);
    if let Some(root) = write_root(cx, target) {
        out.push(root);
    }
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let operator = child_by_field(n, "operator");
    let caret = operator.is_some_and(|op| op.kind() == "concat_operator" && text(cx, op) == "^");
    if !caret {
        return ArgShape::Other;
    }
    let quoted = any_named_child(n, |c| c.kind() == "string" || c.kind() == "quoted_string");
    if quoted {
        ArgShape::Concat
    } else {
        ArgShape::Other
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "quoted_string" => string_arg_shape(cx, n, &["quoted_string_content"]),
        "infix_expression" => concat_shape(cx, n),
        "value_path" => {
            if has_child_kind(n, "module_path") {
                ArgShape::Other
            } else {
                ArgShape::Ident(text(cx, n))
            }
        }
        _ => ArgShape::Other,
    }
}

fn ocaml_is_simple_path(func: Node<'_>) -> bool {
    if func.kind() == "identifier" || func.kind() == "value_name" {
        return true;
    }
    if func.kind() != "value_path" {
        return false;
    }
    !has_child_kind(func, "module_path")
}

pub(crate) fn call_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
    if n.kind() == "new_expression" {
        let class = child_by_field(n, "class_path")?;
        let created = CallHead {
            callee: text(cx, class),
            kind: CallKind::New,
            callee_root: None,
            is_simple_path: false,
            initializes_binding: false,
            args: Vec::new(),
        };
        return Some(created);
    }
    if n.kind() != "application_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    let callee = trimmed(cx, func);
    let mut cursor = n.walk();
    let mut args = Vec::new();
    for arg in n.children_by_field_name("argument", &mut cursor) {
        if arg.kind() == "labeled_argument" {
            match child_by_field(arg, "expression") {
                Some(expr) => args.push(make_arg(cx, expr)),
                None => args.push(make_arg(cx, arg)),
            }
        } else {
            args.push(make_arg(cx, arg));
        }
    }
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root: callee.split('.').next(),
        is_simple_path: ocaml_is_simple_path(func),
        initializes_binding: false,
        args,
    })
}
fn is_rel_compare(cx: &Cx<'_, '_>, op: Node<'_>) -> bool {
    if op.kind() != "rel_operator" {
        return false;
    }
    let symbol = text(cx, op);
    symbol == "=" || symbol == "==" || symbol == "<>" || symbol == "!="
}

fn is_raise(cx: &Cx<'_, '_>, func: Option<Node<'_>>) -> bool {
    func.is_some_and(|f| {
        let callee = text(cx, f);
        callee == "raise" || callee == "raise_notrace" || callee == "Stdlib.raise"
    })
}

pub(crate) fn case_stmt<'s>(cx: &Cx<'_, 's>, case: Node<'_>) -> Stmt<'s> {
    let Some(body) = last_named_child(case) else {
        return Stmt::Other;
    };
    if body.kind() != "application_expression" {
        return Stmt::Other;
    }
    let func = child_by_field(body, "function");
    if !is_raise(cx, func) {
        let callee = func.map(|f| trimmed(cx, f)).unwrap_or("");
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    let bound = child_by_field(case, "pattern")
        .and_then(|p| pattern_ident(cx, p))
        .map(|(name, _)| name)
        .unwrap_or("");
    for arg in named_children(body) {
        if Some(arg.id()) == func.map(|f| f.id()) {
            continue;
        }
        if text(cx, arg) == bound {
            return Stmt::Rethrow;
        }
        return Stmt::Other;
    }
    Stmt::Other
}

fn is_bare_assert_argument(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "value_name" => return true,
            "value_path" => return !has_child_kind(current, "module_path"),
            "parenthesized_expression" => match child_by_field(current, "expression") {
                Some(next) => current = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

fn is_string_arg(_cx: &Cx<'_, '_>, c: Node<'_>) -> bool {
    if c.kind() == "string" || c.kind() == "quoted_string" {
        return true;
    }
    if c.kind() != "labeled_argument" {
        return false;
    }
    child_by_field(c, "expression")
        .is_some_and(|e| e.kind() == "string" || e.kind() == "quoted_string")
}

pub(crate) fn assertion_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
    if n.kind() == "assert_expression" {
        let comparison = child_by_field(n, "expression").is_some_and(|e| {
            let body = match e.kind() {
                "parenthesized_expression" => child_by_field(e, "expression").unwrap_or(e),
                _ => e,
            };
            body.kind() == "infix_expression"
                && child_by_field(body, "operator").is_some_and(|op| is_rel_compare(cx, op))
        });
        let bare =
            !comparison && child_by_field(n, "expression").is_some_and(is_bare_assert_argument);
        let checked = Assertion {
            pos: pos_of(n),
            has_message: false,
            comparison,
            shape: if bare {
                AssertShape::BareName
            } else {
                AssertShape::Other
            },
            in_typecheck: false,
            value_is_bool_literal: false,
        };
        return Some(checked);
    }
    if n.kind() != "application_expression" {
        return None;
    }
    let callee = child_by_field(n, "function")
        .map(|f| text(cx, f))
        .unwrap_or("");
    if callee != "Alcotest.check" && !callee.starts_with("Alcotest.(check") {
        return None;
    }
    let has_message = any_named_child(n, |c| is_string_arg(cx, c));
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison: false,
        shape: AssertShape::Other,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
