use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use tree_sitter::Node;

use super::has_operator;
use crate::hooks::{
    CallHead, Cx, child_by_field, first_named_child, last_named_child, named_children, narrow,
    pos_of, slice, string_arg_shape, text, trimmed,
};

fn odin_static_run(cx: &Cx<'_, '_>, operand: Node<'_>) -> u32 {
    let mut node = operand;
    if node.kind() == "parenthesized_expression"
        && let Some(inner) = first_named_child(node)
    {
        node = inner;
    }
    if node.kind() == "string"
        && let ArgShape::Str { longest_static } = string_arg_shape(cx, node, &["string_content"])
    {
        return longest_static;
    }
    0
}

fn odin_concat_static(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    let mut operands = Vec::new();
    let mut current = n;
    loop {
        if current.kind() != "binary_expression" || current.child_count() != 3 {
            operands.push(current);
            break;
        }
        let (Some(left), Some(op), Some(right)) =
            (current.child(0), current.child(1), current.child(2))
        else {
            return None;
        };
        if text(cx, op) != "+" {
            operands.push(current);
            break;
        }
        operands.push(right);
        current = left;
    }
    if !operands.iter().any(|operand| operand.kind() == "string") {
        return None;
    }
    let mut best = 0u32;
    for operand in operands {
        best = best.max(odin_static_run(cx, operand));
    }
    Some(best)
}

fn odin_composite_shape(n: Node<'_>) -> ArgShape<'static> {
    if n.kind() == "map" {
        return ArgShape::Other;
    }
    let mut keyed = false;
    let mut code = false;
    let mut cursor = n.walk();
    for child in n.children(&mut cursor) {
        if child.kind() == "procedure" || child.kind() == "block" {
            code = true;
        }
        if child.kind() != "struct_field" {
            continue;
        }
        let mut inner = child.walk();
        for part in child.children(&mut inner) {
            if part.kind() == "procedure" || part.kind() == "block" {
                code = true;
            } else if !part.is_named() && part.kind() == "=" {
                keyed = true;
            }
        }
    }
    if code {
        ArgShape::CodeBody
    } else if keyed {
        ArgShape::Other
    } else {
        ArgShape::Collection
    }
}

pub(crate) fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    let shape = match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        "binary_expression" => match odin_concat_static(cx, n) {
            Some(longest_static) => ArgShape::Concat { longest_static },
            None => ArgShape::Other,
        },
        "struct" | "map" | "bit_set" | "matrix" => odin_composite_shape(n),
        "procedure" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    };
    Arg {
        shape,
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

pub(crate) fn positional_nodes<'a>(n: Node<'a>, func: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = n.walk();
    let children: Vec<Node<'a>> = n.children(&mut cursor).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        if !child.is_named()
            || child.id() == func.id()
            || matches!(child.kind(), "comment" | "block_comment")
        {
            i += 1;
            continue;
        }
        if let Some((value, next)) = odin_arg_value(&children, i) {
            out.push(value);
            i = next;
            continue;
        }
        out.push(child);
        i += 1;
    }
    out
}

fn odin_skippable(n: Node<'_>) -> bool {
    matches!(n.kind(), "comment" | "block_comment") || (!n.is_named() && n.kind() != "=")
}

fn odin_arg_value<'a>(children: &[Node<'a>], i: usize) -> Option<(Node<'a>, usize)> {
    let mut j = i + 1;
    while j < children.len() && odin_skippable(children[j]) {
        j += 1;
    }
    if j >= children.len() || children[j].is_named() || children[j].kind() != "=" {
        return None;
    }
    let mut k = j + 1;
    while k < children.len() && !children[k].is_named() {
        k += 1;
    }
    (k < children.len()).then_some((children[k], k + 1))
}

fn odin_path_simple(mut n: Node<'_>) -> bool {
    loop {
        match n.kind() {
            "identifier" => return true,
            "member_expression" => {
                if n.named_child_count() != 2 {
                    return false;
                }
                let (Some(object), Some(field)) = (n.named_child(0), n.named_child(1)) else {
                    return false;
                };
                if field.kind() != "identifier" {
                    return false;
                }
                n = object;
            }
            _ => return false,
        }
    }
}

fn odin_callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        match current.kind() {
            "identifier" => return Some(text(cx, current)),
            "member_expression" => {
                if current.named_child_count() != 2 {
                    return None;
                }
                {
                    let next = first_named_child(current)?;
                    current = next
                }
            }
            _ => return None,
        }
    }
}

fn odin_enclosing_member<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Option<Node<'t>> {
    let mut child = n.id();
    let mut current = cx.parent_of(n)?;
    let mut outer = None;
    while current.kind() == "member_expression"
        && last_named_child(current).is_some_and(|last| last.id() == child)
    {
        outer = Some(current);
        child = current.id();
        match cx.parent_of(current) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    outer
}

fn odin_member_callee<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    func: Node<'_>,
    outer: Node<'_>,
) -> (&'s str, Option<&'s str>, bool) {
    if let Some(receiver) = first_named_child(outer)
        && receiver.id() != n.id()
        && odin_path_simple(receiver)
    {
        let callee = slice(cx.src, receiver.start_byte(), func.end_byte());
        return (callee, odin_callee_root(cx, receiver), true);
    }
    (text(cx, func), None, false)
}

fn odin_callee<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    func: Node<'_>,
) -> (&'s str, Option<&'s str>, bool) {
    if func.kind() == "identifier" {
        if let Some(outer) = odin_enclosing_member(cx, n) {
            return odin_member_callee(cx, n, func, outer);
        }
        return (text(cx, func), Some(text(cx, func)), true);
    }
    let simple = odin_path_simple(func);
    let callee = qualified_callee_with(cx, func, simple);
    let root = if simple {
        odin_callee_root(cx, func)
    } else {
        None
    };
    (callee, root, simple)
}

pub(crate) fn qualified_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>, func: Node<'_>) -> &'s str {
    odin_callee(cx, n, func).0
}

fn qualified_callee_with<'s>(cx: &Cx<'_, 's>, func: Node<'_>, simple: bool) -> &'s str {
    if simple {
        return trimmed(cx, func);
    }
    if func.kind() == "member_expression"
        && let Some(last) = named_children(func).last()
        && last.kind() == "identifier"
    {
        return text(cx, *last);
    }
    let full = trimmed(cx, func);
    let mut cut = full.len();
    for (idx, byte) in full.bytes().enumerate() {
        if byte == b'(' || byte == b'[' {
            cut = idx;
            break;
        }
    }
    full[..cut].trim_end()
}

fn odin_value_after_eq(parent: Node<'_>, n: Node<'_>) -> bool {
    let mut seen_eq = false;
    for i in 0..parent.child_count() {
        let Some(child) = parent.child(i) else {
            continue;
        };
        if !child.is_named() {
            if child.kind() == "=" {
                seen_eq = true;
            }
        } else if seen_eq && child.id() == n.id() {
            return true;
        }
    }
    false
}

fn odin_initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    let short = match parent.kind() {
        "assignment_statement" | "variable_declaration" => has_operator(parent, cx, ":="),
        "var_declaration" => odin_value_after_eq(parent, n),
        "const_declaration" => has_operator(parent, cx, "::"),
        _ => false,
    };
    if !short {
        return false;
    }
    if !named_children(parent).iter().any(|c| c.id() == n.id()) {
        return false;
    }
    named_children(parent)
        .iter()
        .find(|c| c.kind() == "identifier")
        .is_some_and(|id| text(cx, *id) != "_")
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
    let (callee, callee_root, simple) = odin_callee(cx, n, func);
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root,
        is_simple_path: simple,
        initializes_binding: odin_initializes_binding(cx, n),
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
    let formatted = is_formatted_assert(callee);
    if !formatted && !is_plain_assert(callee) {
        return None;
    }
    let args = positional_nodes(n, func);
    let first = args.first().copied();
    let comparison = first.is_some_and(|a| odin_is_compare(cx, a));
    let bare = !comparison && first.is_some_and(|a| a.kind() == "identifier");
    Some(Assertion {
        pos: pos_of(n),
        has_message: formatted || args.len() >= 2,
        comparison,
        shape: if bare {
            AssertShape::BareName
        } else if comparison {
            AssertShape::TopEqNe
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
