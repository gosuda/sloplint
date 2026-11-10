use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, child_by_field, first_named_child, named_children, narrow, pos_of, slice,
    string_arg_shape, text, trimmed,
};

pub(crate) fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    let shape = match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    };
    Arg {
        shape,
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

pub(crate) fn positional_nodes<'a>(n: Node<'a>, func: Node<'a>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        if child.id() == func.id() || matches!(child.kind(), "comment" | "block_comment") {
            continue;
        }
        out.push(child);
    }
    out
}

fn member_parent<'t>(cx: &Cx<'t, '_>, call: Node<'_>) -> Option<Node<'t>> {
    cx.parent().filter(|parent| {
        parent.kind() == "member_expression"
            && parent
                .child(parent.child_count().saturating_sub(1))
                .is_some_and(|last| last.id() == call.id())
    })
}

pub(crate) fn qualified_callee<'s>(cx: &Cx<'_, 's>, call: Node<'_>, func: Node<'_>) -> &'s str {
    if let Some(parent) = member_parent(cx, call) {
        return slice(cx.src, parent.start_byte(), func.end_byte()).trim();
    }
    trimmed(cx, func)
}

fn odin_callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        if current.kind() == "identifier" {
            return Some(text(cx, current));
        }
        current = first_named_child(current)?;
    }
}

fn odin_member_root<'s>(cx: &Cx<'_, 's>, parent: Node<'_>) -> Option<&'s str> {
    let mut current = first_named_child(parent)?;
    loop {
        if current.kind() == "identifier" {
            return Some(text(cx, current));
        }
        if current.kind() != "member_expression" {
            return None;
        }
        current = first_named_child(current)?;
    }
}

fn odin_qualified_simple(parent: Node<'_>, call: Node<'_>, func: Node<'_>) -> bool {
    if func.kind() != "identifier" {
        return false;
    }
    named_children(parent)
        .into_iter()
        .filter(|c| c.id() != call.id())
        .all(|c| c.kind() == "identifier")
}

pub(crate) fn call_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function").or_else(|| first_named_child(n))?;
    let args = positional_nodes(n, func)
        .into_iter()
        .map(|a| make_arg(cx, a))
        .collect();
    let callee = qualified_callee(cx, n, func);
    let member = member_parent(cx, n);
    let callee_root = match member {
        Some(parent) => odin_member_root(cx, parent).or_else(|| odin_callee_root(cx, func)),
        None => odin_callee_root(cx, func),
    };
    let is_simple_path = match member {
        Some(parent) => odin_qualified_simple(parent, n, func),
        None => func.kind() == "identifier",
    };
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root,
        is_simple_path,
        initializes_binding: false,
        args,
    })
}

fn odin_is_compare(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "binary_expression" {
        return false;
    }
    let equal = |c: Node<'_>| text(cx, c) == "==" || text(cx, c) == "!=";
    (0..n.child_count()).any(|i| n.child(i).is_some_and(equal))
}

fn is_reflect_callee(callee: &str) -> bool {
    callee.contains(".struct_field_by_name") || callee.ends_with("reflect.enum_from_name")
}

fn is_formatted_assert(callee: &str) -> bool {
    callee == "fmt.assertf"
        || callee == "fmt.ensuref"
        || callee.ends_with(".assertf")
        || callee.ends_with(".ensuref")
}

fn is_plain_assert(callee: &str) -> bool {
    callee == "assert"
        || callee == "ensure"
        || callee.ends_with(".assert")
        || callee.ends_with(".ensure")
}

pub(crate) fn dynamic_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function").or_else(|| first_named_child(n))?;
    if !is_reflect_callee(qualified_callee(cx, n, func)) {
        return None;
    }
    let args = positional_nodes(n, func);
    if args.len() < 2 {
        return None;
    }
    (args[1].kind() != "string").then_some("runtime name lookup (reflect)")
}

pub(crate) fn assertion_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function").or_else(|| first_named_child(n))?;
    let callee = qualified_callee(cx, n, func);
    if is_formatted_assert(callee) {
        let messaged = Assertion {
            pos: pos_of(n),
            has_message: true,
            comparison: false,
            shape: AssertShape::Other,
            in_typecheck: false,
            value_is_bool_literal: false,
        };
        return Some(messaged);
    }
    if !is_plain_assert(callee) {
        return None;
    }
    let args = positional_nodes(n, func);
    let first = args.first().copied();
    let comparison = first.is_some_and(|a| odin_is_compare(cx, a));
    let bare = !comparison && first.is_some_and(|a| a.kind() == "identifier");
    let arity = args.len() >= 2;
    Some(Assertion {
        pos: pos_of(n),
        has_message: arity,
        comparison,
        shape: if bare {
            AssertShape::BareName
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

#[cfg(test)]
mod tests {
    use super::positional_nodes;

    fn call_in(source: &str) -> (tree_sitter::Tree, usize, usize) {
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_odin::LANGUAGE.into();
        parser.set_language(&language).expect("odin grammar loads");
        let tree = parser.parse(source, None).expect("parser returns a tree");
        fn find_call(n: tree_sitter::Node<'_>) -> Option<(usize, usize)> {
            if n.kind() == "call_expression" {
                return Some((n.start_byte(), n.end_byte()));
            }
            (0..n.named_child_count())
                .find_map(|i| n.named_child(u32::try_from(i).ok()?).and_then(find_call))
        }
        let range = find_call(tree.root_node()).expect("a call_expression in the tree");
        (tree, range.0, range.1)
    }

    #[test]
    fn comment_between_arguments_is_not_an_argument() {
        let (tree, start, end) = call_in("main :: proc() {\n\tf(a, /* c */ b)\n}\n");
        let root = tree.root_node();
        let call = root.descendant_for_byte_range(start, end).unwrap();
        assert_eq!(call.kind(), "call_expression");
        let func = call
            .child_by_field_name("function")
            .expect("a function field");
        let kinds: Vec<&str> = positional_nodes(call, func)
            .iter()
            .map(|n| n.kind())
            .collect();
        assert_eq!(
            kinds,
            vec!["identifier", "identifier"],
            "a comment inside the call must not become an argument"
        );
    }
}
