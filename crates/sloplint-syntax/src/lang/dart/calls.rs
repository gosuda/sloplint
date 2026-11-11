use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, any_named_child, binary_concat, child_by_field, first_named_child,
    first_named_child_matching, last_named_child, named_children, narrow, pos_of, text,
};

use super::syntax::{COMMENTS, cut_generics, label_text};

fn callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "super" => return Some(text(cx, current)),
            "member_expression" | "null_aware_member_expression" => {
                child_by_field(current, "object")?
            }
            _ => return None,
        };
    }
}

fn has_interpolation(n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "template_substitution"
            || c.kind().starts_with("string_literal")
                && any_named_child(c, |g| g.kind() == "template_substitution")
    })
}

fn string_static<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> (u32, bool) {
    let mut segs = Vec::new();
    let mut dynamic = false;
    for c in named_children(e) {
        if c.kind() == "template_substitution" {
            dynamic = true;
        } else if c.kind().starts_with("template_chars") {
            segs.push(text(cx, c));
        } else {
            for g in named_children(c) {
                if g.kind() == "template_substitution" {
                    dynamic = true;
                } else if g.kind().starts_with("template_chars") {
                    segs.push(text(cx, g));
                }
            }
        }
    }
    (longest_static_chunk(&segs), dynamic)
}

fn concat_static(cx: &Cx<'_, '_>, e: Node<'_>) -> u32 {
    let mut longest_static = 0;
    let mut pending = vec![e];
    while let Some(node) = pending.pop() {
        if node.kind() == "string_literal" {
            longest_static = longest_static.max(string_static(cx, node).0);
        } else if node.kind() == "additive_expression"
            && node.child(1).is_some_and(|op| text(cx, op) == "+")
        {
            pending.extend([node.child(0), node.child(2)].into_iter().flatten());
        }
    }
    longest_static
}

fn collection_shape(n: Node<'_>) -> ArgShape<'static> {
    for c in named_children(n) {
        if c.kind() == "pair" {
            return ArgShape::Other;
        }
        if c.kind() == "function_expression" {
            return ArgShape::CodeBody;
        }
    }
    ArgShape::Collection
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> ArgShape<'s> {
    if e.kind() == "string_literal" {
        let (longest_static, dynamic) = string_static(cx, e);
        if dynamic {
            return ArgShape::Concat { longest_static };
        }
        return ArgShape::Str { longest_static };
    }
    if e.kind() == "additive_expression" && binary_concat(cx, e, "+", &["string_literal"]) {
        return ArgShape::Concat {
            longest_static: concat_static(cx, e),
        };
    }
    if e.kind() == "list_literal" || e.kind() == "set_or_map_literal" {
        return collection_shape(e);
    }
    if e.kind() == "function_expression" {
        return ArgShape::CodeBody;
    }
    if e.kind() == "identifier" {
        return ArgShape::Ident(text(cx, e));
    }
    ArgShape::Other
}

fn arg_value(arg: Node<'_>) -> Node<'_> {
    if arg.kind() == "named_argument" {
        last_named_child(arg).unwrap_or(arg)
    } else {
        arg
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    let Some(list) = child_by_field(n, "arguments") else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|a| !COMMENTS.contains(&a.kind()))
        .map(|a| {
            let e = arg_value(a);
            Arg {
                shape: arg_shape(cx, e),
                start_line: narrow(a.start_position().row) + 1,
                end_line: narrow(a.end_position().row) + 1,
            }
        })
        .collect()
}

fn dart_is_simple_path(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "this" | "super" => return true,
            "member_expression" | "null_aware_member_expression" => {
                match child_by_field(current, "object") {
                    Some(next) => current = next,
                    None => return false,
                }
            }
            _ => return false,
        }
    }
}

fn dart_initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "initialized_variable_definition"
        && parent.kind() != "initialized_identifier"
        && parent.kind() != "static_final_declaration"
    {
        return false;
    }
    child_by_field(parent, "value").is_some_and(|v| v.id() == n.id())
}

pub(crate) fn callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    let kind = func.kind();
    if (kind == "member_expression" || kind == "null_aware_member_expression")
        && !dart_is_simple_path(func)
        && let Some(property) = child_by_field(func, "property")
    {
        return text(cx, property);
    }
    cut_generics(text(cx, func))
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() == "call_expression" {
        let func = child_by_field(n, "function")?;
        return Some(CallHead {
            callee: callee_text(cx, func),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, func),
            is_simple_path: dart_is_simple_path(func),
            initializes_binding: dart_initializes_binding(cx, n),
            args: call_args(cx, n),
        });
    }
    if n.kind() != "new_expression" && n.kind() != "const_object_expression" {
        return None;
    }
    let ty = child_by_field(n, "type")?;
    Some(CallHead {
        callee: cut_generics(text(cx, ty)),
        kind: CallKind::New,
        callee_root: None,
        is_simple_path: false,
        initializes_binding: false,
        args: call_args(cx, n),
    })
}

fn is_reason_arg(cx: &Cx<'_, '_>, k: Node<'_>) -> bool {
    first_named_child(k).is_some_and(|l| l.kind() == "label" && label_text(cx, l) == "reason")
}

fn is_equals_call(cx: &Cx<'_, '_>, s: Node<'_>) -> bool {
    s.kind() == "call_expression"
        && child_by_field(s, "function").is_some_and(|f| {
            let c = cut_generics(text(cx, f));
            c == "equals" || c == "same"
        })
}

fn expect_tail(cx: &Cx<'_, '_>, args_node: Node<'_>) -> (bool, bool) {
    let mut has_message = false;
    let mut positional = 0u32;
    let mut second = None;
    let mut cursor = args_node.walk();
    for k in args_node.named_children(&mut cursor) {
        if COMMENTS.contains(&k.kind()) {
            continue;
        }
        if k.kind() == "named_argument" {
            has_message |= is_reason_arg(cx, k);
            continue;
        }
        positional += 1;
        if positional == 2 {
            second = Some(k);
        }
    }
    (has_message, second.is_some_and(|s| is_equals_call(cx, s)))
}

fn dart_assert_shape(cond: Node<'_>) -> AssertShape {
    match cond.kind() {
        "identifier" => AssertShape::BareName,
        "equality_expression" => AssertShape::TopEqNe,
        _ => AssertShape::Other,
    }
}

pub(crate) fn assertion(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() == "assert_statement" {
        let a = first_named_child_matching(n, &["assertion"])?;
        let cond = first_named_child(a);
        return Some(Assertion {
            pos: pos_of(n),
            has_message: a.named_child_count() >= 2,
            comparison: cond.is_some_and(|k| k.kind() == "equality_expression"),
            shape: cond.map_or(AssertShape::Other, dart_assert_shape),
            in_typecheck: false,
            value_is_bool_literal: false,
        });
    }
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func.kind() != "identifier" || text(cx, func) != "expect" {
        return None;
    }
    let args_node = child_by_field(n, "arguments")?;
    let (has_message, comparison) = expect_tail(cx, args_node);
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape: AssertShape::Other,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

pub(crate) fn dynamic(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func.kind() != "identifier" || text(cx, func) != "Symbol" {
        return None;
    }
    let args = child_by_field(n, "arguments")?;
    let mut cursor = args.walk();
    let first = args
        .named_children(&mut cursor)
        .find(|c| !COMMENTS.contains(&c.kind()))?;
    let value = arg_value(first);
    if value.kind() != "string_literal" {
        return Some("symbol built at runtime");
    }
    has_interpolation(value).then_some("symbol built at runtime")
}
