use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind, FieldCopy};
use tree_sitter::Node;

use super::{REFLECTION, first_code};
use crate::hooks::{
    CallHead, Cx, binary_concat, find_named_child, first_of_kind, has_child_kind, last_named_child,
    last_segment, named_children, narrow, pos_of, rfind_named_child, string_arg_shape, text,
};

pub(crate) fn value_args(n: Node<'_>) -> Vec<Node<'_>> {
    first_of_kind(n, &["value_arguments"]).map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|child| child.kind() == "value_argument")
            .collect()
    })
}

fn is_named_arg(arg: Node<'_>) -> bool {
    has_child_kind(arg, "=")
}

fn arg_value(arg: Node<'_>) -> Option<Node<'_>> {
    last_named_child(arg)
}

fn callee_node(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| {
        child.kind() != "line_comment"
            && child.kind() != "block_comment"
            && !matches!(
                child.kind(),
                "value_arguments" | "annotated_lambda" | "type_arguments"
            )
    })
}

pub(crate) fn callee_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    callee_node(n).map(|callee| text(cx, callee))
}

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
    }
}

fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "navigation_expression" => first_code(current)?,
            "identifier" => return Some(text(cx, current)),
            "this_expression" => return Some("this"),
            "super_expression" => return Some("super"),
            _ => return None,
        };
    }
}

fn kotlin_simple(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "this_expression" | "super_expression" => return true,
            "navigation_expression" | "parenthesized_expression" => {
                let Some(next) = first_code(current) else {
                    return false;
                };
                current = next;
            }
            _ => return false,
        }
    }
}

fn declares_value(cx: &Cx<'_, '_>) -> bool {
    cx.parent()
        .is_some_and(|p| p.kind() == "property_declaration")
}

fn value_shape<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "string_literal" | "multiline_string_literal" => {
            string_arg_shape(cx, value, &["string_content"])
        }
        "binary_expression" if binary_concat(cx, value, "+", &["string_literal"]) => {
            ArgShape::Concat
        }
        "collection_literal" => ArgShape::Collection,
        "annotated_lambda" | "lambda_literal" | "anonymous_function" => ArgShape::CodeBody,
        "identifier" => {
            let name = text(cx, value);
            if matches!(name, "true" | "false") {
                ArgShape::Other
            } else {
                ArgShape::Ident(name)
            }
        }
        _ => ArgShape::Other,
    }
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = callee_node(n)?;
    let mut args: Vec<Arg<'s>> = value_args(n)
        .into_iter()
        .filter_map(|arg| {
            let value = arg_value(arg)?;
            Some(span_arg(arg, value_shape(cx, value)))
        })
        .collect();
    if let Some(lambda) = first_of_kind(n, &["annotated_lambda"]) {
        args.push(span_arg(lambda, ArgShape::CodeBody));
    }
    Some(CallHead {
        callee: text(cx, callee),
        kind: CallKind::Plain,
        callee_root: callee_root(cx, callee),
        is_simple_path: kotlin_simple(callee),
        initializes_binding: declares_value(cx),
        args,
    })
}

fn copy_source<'s>(cx: &Cx<'_, 's>, args: &[Node<'_>]) -> Option<&'s str> {
    let mut found: Option<&'s str> = None;
    for arg in args {
        let label = text(cx, first_of_kind(*arg, &["identifier"])?);
        let value = arg_value(*arg)?;
        if value.kind() != "navigation_expression" {
            return None;
        }
        let object = text(cx, first_code(value).filter(|o| o.kind() == "identifier")?);
        let member = rfind_named_child(value, |child| child.kind() == "identifier")?;
        if text(cx, member) != label || found.is_some_and(|known| known != object) {
            return None;
        }
        found = Some(object);
    }
    found
}

pub(crate) fn struct_copy<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = callee_node(n)?;
    if callee.kind() == "navigation_expression" {
        return None;
    }
    let args = value_args(n);
    let named: Vec<Node<'_>> = args
        .iter()
        .copied()
        .filter(|arg| is_named_arg(*arg))
        .collect();
    if named.is_empty() {
        return None;
    }
    let source = if named.len() == args.len() {
        copy_source(cx, &named)
    } else {
        None
    };
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(named.len()),
        source: if source == Some("this") { None } else { source },
    })
}

pub(crate) fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = text(cx, callee_node(n)?);
    let last = last_segment(callee, &["."]);
    let on_class = callee
        .rsplit_once('.')
        .is_some_and(|(qualifier, _)| last_segment(qualifier, &["."]) == "Class");
    let reflective = REFLECTION.contains(&last) || (last == "forName" && on_class);
    if !reflective {
        return None;
    }
    let first = arg_value(value_args(n).into_iter().next()?)?;
    (!matches!(first.kind(), "string_literal" | "multiline_string_literal"))
        .then_some("reflection by name")
}

pub(crate) fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = text(cx, callee_node(n)?);
    let count = value_args(n).len() + usize::from(has_child_kind(n, "annotated_lambda"));
    let last = last_segment(callee, &["."]);
    let shape = match last {
        "assertTrue" | "assertFalse" | "assertNull" | "assertNotNull" => {
            let bare = value_args(n)
                .into_iter()
                .next()
                .and_then(arg_value)
                .is_some_and(|v| v.kind() == "identifier");
            if bare {
                AssertShape::BareName
            } else {
                AssertShape::Other
            }
        }
        "assertEquals" | "assertNotEquals" | "assertSame" | "assertNotSame" => AssertShape::TopEqNe,
        _ => AssertShape::Other,
    };
    let (has_message, comparison) = match last {
        "assertTrue" | "assertFalse" | "assertNull" | "assertNotNull" => (count > 1, false),
        "assertEquals" | "assertNotEquals" | "assertSame" | "assertNotSame" => (count > 2, true),
        "fail" => (count > 0, false),
        _ => return None,
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
