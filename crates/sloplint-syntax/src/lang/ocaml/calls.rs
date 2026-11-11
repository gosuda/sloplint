use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind, Pos, Stmt};
use tree_sitter::Node;

use super::funcs::pattern_ident;
use crate::hooks::{
    CallHead, Cx, any_named_child, child_by_field, first_of_kind, has_child_kind, last_named_child,
    named_children, narrow, pos_of, string_arg_shape, text, trimmed,
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
    let mut cursor = n.walk();
    let Some(arg) = n.children_by_field_name("argument", &mut cursor).next() else {
        return;
    };
    if let Some(root) = write_root(cx, arg) {
        out.push(root);
    }
}

pub(crate) fn incr_write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    if n.kind() != "application_expression" || !is_incr(cx, n) {
        return None;
    }
    let mut cursor = n.walk();
    let arg = n.children_by_field_name("argument", &mut cursor).next()?;
    write_root(cx, arg)
}

fn arg_at<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    arg_at(n, arg_shape(cx, n))
}

fn is_concat_op(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "operator")
        .is_some_and(|op| op.kind() == "concat_operator" && text(cx, op) == "^")
}

fn concat_fragments<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fragments: &mut Vec<&'s str>) {
    let mut stack = vec![n];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "string" | "quoted_string" => {
                let content = if node.kind() == "string" {
                    "string_content"
                } else {
                    "quoted_string_content"
                };
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == content {
                        fragments.push(text(cx, child));
                    }
                }
            }
            "parenthesized_expression" => {
                if let Some(inner) = child_by_field(node, "expression") {
                    stack.push(inner);
                }
            }
            "infix_expression" => {
                if !is_concat_op(cx, node) {
                    continue;
                }
                if let Some(left) = child_by_field(node, "left") {
                    stack.push(left);
                }
                if let Some(right) = child_by_field(node, "right") {
                    stack.push(right);
                }
            }
            _ => {}
        }
    }
}

fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if !is_concat_op(cx, n) {
        return ArgShape::Other;
    }
    let mut fragments: Vec<&str> = Vec::new();
    concat_fragments(cx, n, &mut fragments);
    ArgShape::Concat {
        longest_static: sloplint_core::text::longest_static_chunk(&fragments),
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let mut current = n;
    while current.kind() == "parenthesized_expression" {
        let Some(next) = child_by_field(current, "expression") else {
            return ArgShape::Other;
        };
        current = next;
    }
    match current.kind() {
        "string" => string_arg_shape(cx, current, &["string_content"]),
        "quoted_string" => string_arg_shape(cx, current, &["quoted_string_content"]),
        "infix_expression" => concat_shape(cx, current),
        "fun_expression"
        | "function_expression"
        | "match_expression"
        | "if_expression"
        | "let_expression" => ArgShape::CodeBody,
        "list_expression" | "array_expression" | "tuple_expression" => {
            if collection_has_closure(current) {
                ArgShape::CodeBody
            } else {
                ArgShape::Collection
            }
        }
        "record_expression" => ArgShape::Other,
        "value_path" | "value_name" => ArgShape::Ident(text(cx, current)),
        _ => ArgShape::Other,
    }
}

fn collection_has_closure(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    while let Some(node) = stack.pop() {
        if node.kind() == "fun_expression" || node.kind() == "function_expression" {
            return true;
        }
        stack.extend(named_children(node));
    }
    false
}

fn path_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        match current.kind() {
            "identifier" | "value_name" => return Some(text(cx, current)),
            "value_path" => {
                if has_child_kind(current, "parenthesized_operator") {
                    return None;
                }
                return text(cx, current).split('.').next();
            }
            "field_get_expression" => {
                current = child_by_field(current, "record")?;
            }
            _ => return None,
        }
    }
}

fn ocaml_is_simple_path(func: Node<'_>) -> bool {
    let mut current = func;
    loop {
        match current.kind() {
            "identifier" | "value_name" => return true,
            "value_path" => return !has_child_kind(current, "parenthesized_operator"),
            "field_get_expression" => {
                let Some(next) = child_by_field(current, "record") else {
                    return false;
                };
                current = next;
            }
            _ => return false,
        }
    }
}

fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "let_binding" {
        return false;
    }
    if first_of_kind(parent, &["parameter"]).is_some() {
        return false;
    }
    child_by_field(parent, "body").is_some_and(|body| body.id() == n.id())
}

fn fallback_callee<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    let mut current = func;
    loop {
        let next = match current.kind() {
            "application_expression" => child_by_field(current, "function"),
            "parenthesized_expression" | "prefix_expression" => {
                child_by_field(current, "expression")
            }
            _ => None,
        };
        let Some(next) = next else {
            return text(cx, current);
        };
        current = next;
    }
}

pub(crate) fn call_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
    if n.kind() == "new_expression" {
        let class = child_by_field(n, "class_path")?;
        let created = CallHead {
            callee: text(cx, class),
            kind: CallKind::New,
            callee_root: None,
            is_simple_path: false,
            initializes_binding: initializes_binding(cx, n),
            args: Vec::new(),
        };
        return Some(created);
    }
    if n.kind() != "application_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    let (callee, callee_root) = match func.kind() {
        "identifier" | "value_name" | "value_path" | "field_get_expression" => {
            (text(cx, func), path_root(cx, func))
        }
        _ => (fallback_callee(cx, func), None),
    };
    let mut cursor = n.walk();
    let mut args = Vec::new();
    for arg in n.children_by_field_name("argument", &mut cursor) {
        push_call_arg(cx, arg, &mut args);
    }
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root,
        is_simple_path: ocaml_is_simple_path(func),
        initializes_binding: initializes_binding(cx, n),
        args,
    })
}

fn push_call_arg<'s>(cx: &Cx<'_, 's>, arg: Node<'_>, args: &mut Vec<Arg<'s>>) {
    if arg.kind() != "labeled_argument" {
        args.push(make_arg(cx, arg));
        return;
    }
    if let Some(expr) = child_by_field(arg, "expression") {
        args.push(make_arg(cx, expr));
        return;
    }
    let Some(label) = first_of_kind(arg, &["label_name"]) else {
        args.push(make_arg(cx, arg));
        return;
    };
    args.push(arg_at(arg, ArgShape::Ident(text(cx, label))));
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

fn body_stmt<'s>(cx: &Cx<'_, 's>, case: Node<'_>, body: Node<'_>) -> Stmt<'s> {
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
    let pat = child_by_field(case, "pattern");
    let bound = pat
        .and_then(|p| pattern_ident(cx, p))
        .map(|(name, _)| name)
        .unwrap_or_else(|| pat.map(|p| text(cx, p)).unwrap_or(""));
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

pub(crate) fn case_stmts<'s>(cx: &Cx<'_, 's>, case: Node<'_>, out: &mut Vec<Stmt<'s>>) {
    let Some(body) = last_named_child(case) else {
        return;
    };
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "parenthesized_expression" {
            if let Some(inner) = child_by_field(node, "expression") {
                stack.push(inner);
            }
        } else if node.kind() == "sequence_expression" {
            let mut kids: Vec<_> = named_children(node)
                .into_iter()
                .filter(|kid| kid.kind() != "attribute_id" && kid.kind() != "comment")
                .collect();
            kids.reverse();
            stack.extend(kids);
        } else if node.kind() != "unit" {
            out.push(body_stmt(cx, case, node));
        }
    }
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
            shape: if comparison {
                AssertShape::TopEqNe
            } else if bare {
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
