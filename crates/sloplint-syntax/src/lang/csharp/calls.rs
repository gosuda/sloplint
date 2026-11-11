use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use super::{EQUALITY_ASSERTS, STRING_KINDS, cut_generics};
use crate::hooks::{
    CallHead, Cx, child_by_field, find_named_child, first_named_child_matching, named_children,
    narrow, pos_of, string_arg_shape, text,
};

fn strip_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    if n.kind() == "generic_name"
        && let Some(i) = first_named_child_matching(n, &["identifier"])
    {
        return text(cx, i);
    }
    text(cx, n)
}

fn member_final_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    if func.kind() != "member_access_expression" && func.kind() != "qualified_name" {
        return None;
    }
    child_by_field(func, "name").map(|m| strip_name(cx, m))
}

fn conditional_final_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    if func.kind() != "conditional_access_expression" {
        return None;
    }
    let b = find_named_child(func, |c| c.kind() == "member_binding_expression")?;
    child_by_field(b, "name").map(|m| strip_name(cx, m))
}

fn final_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if let Some(n) = member_final_name(cx, func) {
        return n;
    }
    if let Some(n) = conditional_final_name(cx, func) {
        return n;
    }
    if func.kind() == "generic_name" {
        return strip_name(cx, func);
    }
    if func.kind() == "identifier" {
        return text(cx, func);
    }
    ""
}

fn plain_step(n: Node<'_>) -> Option<Node<'_>> {
    if n.kind() != "member_access_expression" && n.kind() != "qualified_name" {
        return None;
    }
    if child_by_field(n, "name").is_some_and(|m| m.kind() == "generic_name") {
        return None;
    }
    let field = if n.kind() == "member_access_expression" {
        "expression"
    } else {
        "qualifier"
    };
    child_by_field(n, field)
}

fn plain_chain(func: Node<'_>) -> bool {
    let mut current = func;
    loop {
        if current.kind() == "identifier" || current.kind() == "this" || current.kind() == "base" {
            return true;
        }
        let Some(next) = plain_step(current) else {
            return false;
        };
        current = next;
    }
}

pub(crate) fn callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if plain_chain(func) {
        return text(cx, func);
    }
    final_name(cx, func)
}

fn last_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    final_name(cx, func)
}

pub(crate) fn callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "base" => return Some(text(cx, current)),
            "generic_name" => {
                return first_named_child_matching(current, &["identifier"]).map(|i| text(cx, i));
            }
            "member_access_expression" => child_by_field(current, "expression")?,
            "qualified_name" => child_by_field(current, "qualifier")?,
            "conditional_access_expression" => child_by_field(current, "condition")?,
            _ => return None,
        };
    }
}

fn conditional_next(n: Node<'_>) -> Option<Node<'_>> {
    if first_named_child_matching(n, &["element_binding_expression"]).is_some() {
        return None;
    }
    child_by_field(n, "condition")
}

fn is_simple_path(func: Node<'_>) -> bool {
    let mut current = func;
    loop {
        current = match current.kind() {
            "identifier" | "generic_name" | "this" | "base" => return true,
            "member_access_expression" => match child_by_field(current, "expression") {
                Some(next) => next,
                None => return false,
            },
            "qualified_name" => match child_by_field(current, "qualifier") {
                Some(next) => next,
                None => return false,
            },
            "conditional_access_expression" => match conditional_next(current) {
                Some(next) => next,
                None => return false,
            },
            _ => return false,
        };
    }
}

fn declares_value(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent_of(n) else {
        return false;
    };
    if parent.kind() != "variable_declarator" {
        return false;
    }
    child_by_field(parent, "name").is_some()
}

fn skip_hole(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for h in chars.by_ref() {
        if h == '}' {
            break;
        }
    }
}

fn scan_content(seg: &str, run: &mut u32, best: &mut u32) {
    let mut chars = seg.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                *run = run.saturating_add(1);
            } else {
                *best = (*best).max(*run);
                *run = 0;
                skip_hole(&mut chars);
            }
        } else if c == '}' && chars.peek() == Some(&'}') {
            chars.next();
            *run = run.saturating_add(1);
        } else {
            *run = run.saturating_add(1);
        }
    }
}

fn plain_static(cx: &Cx<'_, '_>, e: Node<'_>) -> u32 {
    let mut run = 0u32;
    let mut best = 0u32;
    let mut cursor = e.walk();
    for child in e.children(&mut cursor) {
        if child.kind() == "escape_sequence" {
            run = run.saturating_add(1);
        } else if child.kind() == "string_literal_content" {
            scan_content(text(cx, child), &mut run, &mut best);
        }
    }
    best.max(run)
}

fn push_plus_operands<'a>(cx: &Cx<'_, '_>, cur: Node<'a>, stack: &mut Vec<Node<'a>>) {
    if cur.child_count() != 3 || !cur.child(1).is_some_and(|o| text(cx, o) == "+") {
        return;
    }
    if let Some(l) = child_by_field(cur, "left") {
        stack.push(l);
    }
    if let Some(r) = child_by_field(cur, "right") {
        stack.push(r);
    }
}

fn verbatim_static(cx: &Cx<'_, '_>, e: Node<'_>) -> u32 {
    let t = text(cx, e);
    let seg = t.get(2..t.len().saturating_sub(1)).unwrap_or(t);
    longest_static_chunk(&[seg])
}

fn raw_static(cx: &Cx<'_, '_>, e: Node<'_>) -> u32 {
    match string_arg_shape(cx, e, &["raw_string_content"]) {
        ArgShape::Str { longest_static } => longest_static,
        _ => 0,
    }
}

fn interpolated_static(cx: &Cx<'_, '_>, e: Node<'_>) -> u32 {
    let mut run = 0u32;
    let mut best = 0u32;
    let mut cursor = e.walk();
    for child in e.children(&mut cursor) {
        if child.kind() == "string_content" {
            scan_content(text(cx, child), &mut run, &mut best);
        } else if child.kind() == "escape_sequence" {
            run = run.saturating_add(1);
        } else if child.kind() == "interpolation" {
            best = best.max(run);
            run = 0;
        }
    }
    best.max(run)
}

fn operand_static(cx: &Cx<'_, '_>, cur: Node<'_>) -> u32 {
    match cur.kind() {
        "string_literal" => plain_static(cx, cur),
        "interpolated_string_expression" => interpolated_static(cx, cur),
        "verbatim_string_literal" => verbatim_static(cx, cur),
        "raw_string_literal" => raw_static(cx, cur),
        _ => 0,
    }
}

fn concat_static(cx: &Cx<'_, '_>, e: Node<'_>) -> Option<u32> {
    let mut stack = vec![e];
    let mut found = false;
    let mut best = 0u32;
    while let Some(cur) = stack.pop() {
        if cur.kind() == "binary_expression" {
            push_plus_operands(cx, cur, &mut stack);
        } else if STRING_KINDS.contains(&cur.kind()) {
            found = true;
            best = best.max(operand_static(cx, cur));
        }
    }
    found.then_some(best)
}

fn object_is_collection(e: Node<'_>) -> bool {
    let Some(init) = child_by_field(e, "initializer") else {
        return false;
    };
    if child_by_field(e, "arguments")
        .is_some_and(|l| named_children(l).iter().any(|a| a.kind() == "argument"))
    {
        return false;
    }
    !named_children(init)
        .iter()
        .any(|c| c.kind() == "assignment_expression")
}

fn contains_closure(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.id() != n.id()
            && matches!(
                cur.kind(),
                "lambda_expression" | "anonymous_method_expression" | "block"
            )
        {
            return true;
        }
        stack.extend(named_children(cur));
    }
    false
}

fn collection_shape(e: Node<'_>) -> ArgShape<'static> {
    if contains_closure(e) {
        ArgShape::CodeBody
    } else {
        ArgShape::Collection
    }
}

fn string_shape<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> Option<ArgShape<'s>> {
    match e.kind() {
        "string_literal" => Some(ArgShape::Str {
            longest_static: plain_static(cx, e),
        }),
        "verbatim_string_literal" => Some(ArgShape::Str {
            longest_static: verbatim_static(cx, e),
        }),
        "raw_string_literal" => Some(ArgShape::Str {
            longest_static: raw_static(cx, e),
        }),
        "interpolated_string_expression" => Some(ArgShape::Concat {
            longest_static: interpolated_static(cx, e),
        }),
        "binary_expression" => match concat_static(cx, e) {
            Some(longest_static) => Some(ArgShape::Concat { longest_static }),
            None => Some(ArgShape::Other),
        },
        _ => None,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> ArgShape<'s> {
    if let Some(s) = string_shape(cx, e) {
        return s;
    }
    match e.kind() {
        "identifier" => ArgShape::Ident(text(cx, e)),
        "lambda_expression" | "anonymous_method_expression" => ArgShape::CodeBody,
        "array_creation_expression"
        | "implicit_array_creation_expression"
        | "collection_expression"
        | "initializer_expression" => collection_shape(e),
        "object_creation_expression" => {
            if object_is_collection(e) {
                collection_shape(e)
            } else {
                ArgShape::Other
            }
        }
        _ => ArgShape::Other,
    }
}

pub(crate) fn arg_expr(n: Node<'_>) -> Option<Node<'_>> {
    let named = child_by_field(n, "name").map(|m| m.id());
    find_named_child(n, |c| Some(c.id()) != named)
}

fn args_from_list<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Arg<'s>> {
    named_children(list)
        .into_iter()
        .filter(|a| a.kind() == "argument")
        .map(|a| Arg {
            shape: arg_expr(a)
                .map(|e| arg_shape(cx, e))
                .unwrap_or(ArgShape::Other),
            start_line: narrow(a.start_position().row) + 1,
            end_line: narrow(a.end_position().row) + 1,
        })
        .collect()
}

fn arg_len(n: Node<'_>) -> u32 {
    let mut count = 0u32;
    if let Some(list) = child_by_field(n, "arguments") {
        for a in named_children(list) {
            if a.kind() == "argument" {
                count += 1;
            }
        }
    }
    count
}

pub(crate) fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|l| first_named_child_matching(l, &["argument"]))
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() == "invocation_expression" {
        let func = child_by_field(n, "function")?;
        return Some(CallHead {
            callee: callee_text(cx, func),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, func),
            is_simple_path: is_simple_path(func),
            initializes_binding: declares_value(cx, n),
            args: child_by_field(n, "arguments")
                .map(|l| args_from_list(cx, l))
                .unwrap_or_default(),
        });
    }
    if n.kind() != "object_creation_expression" {
        return None;
    }
    let ty = child_by_field(n, "type")?;
    Some(CallHead {
        callee: cut_generics(text(cx, ty)),
        kind: CallKind::New,
        callee_root: None,
        is_simple_path: true,
        initializes_binding: declares_value(cx, n),
        args: child_by_field(n, "arguments")
            .map(|l| args_from_list(cx, l))
            .unwrap_or_default(),
    })
}

pub(crate) fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "invocation_expression" {
        return None;
    }
    let last = last_name(cx, child_by_field(n, "function")?);
    if last != "GetMethod"
        && last != "GetProperty"
        && last != "GetField"
        && last != "GetType"
        && last != "InvokeMember"
    {
        return None;
    }
    if first_arg(n)
        .and_then(arg_expr)
        .is_none_or(|e| STRING_KINDS.contains(&e.kind()))
    {
        return None;
    }
    if last == "InvokeMember" {
        Some("reflective member invocation")
    } else {
        Some("reflection lookup by computed name")
    }
}

pub(crate) fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "invocation_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func.kind() != "member_access_expression" {
        return None;
    }
    let expr = child_by_field(func, "expression")?;
    let name = child_by_field(func, "name")
        .map(|m| text(cx, m))
        .unwrap_or("");
    let head = last_name(cx, expr);
    let debug = head == "Debug" && name == "Assert";
    if head != "Assert" && !debug {
        return None;
    }
    let index = match name {
        "True" | "False" | "IsTrue" | "IsFalse" | "Null" | "NotNull" | "IsNull" | "IsNotNull" => 1,
        "Equal" | "NotEqual" | "AreEqual" | "AreNotEqual" | "Same" | "NotSame" | "Contains"
        | "That" => 2,
        "Assert" if debug => 1,
        _ => return None,
    };
    let shape = match name {
        "True" | "False" | "IsTrue" | "IsFalse" | "Null" | "NotNull" | "IsNull" | "IsNotNull" => {
            if first_arg(n)
                .and_then(arg_expr)
                .is_some_and(|e| e.kind() == "identifier")
            {
                AssertShape::BareName
            } else {
                AssertShape::Other
            }
        }
        "Equal" | "NotEqual" | "AreEqual" | "AreNotEqual" => AssertShape::TopEqNe,
        _ => AssertShape::Other,
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message: arg_len(n) > index,
        comparison: EQUALITY_ASSERTS.contains(&name),
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
