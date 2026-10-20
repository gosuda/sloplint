use sloplint_core::facts::{Arg, ArgShape, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, binary_concat, child_by_field, named_children, narrow, pos_of, sole_call_id,
    string_arg_shape, text,
};

pub(crate) fn ts_call_stmt(n: Node<'_>) -> Option<usize> {
    let inner = match n.kind() {
        "return_statement" | "expression_statement" => named_children(n).into_iter().next()?,
        _ => return None,
    };
    let mut cur = inner;
    loop {
        if cur.kind() == "call_expression" {
            return Some(cur.id());
        }
        if cur.kind() == "await_expression" {
            cur = named_children(cur).into_iter().next()?;
        } else {
            return None;
        }
    }
}

pub(crate) fn ts_sole_call(body: Node<'_>) -> Option<usize> {
    sole_call_id(
        body,
        |block| {
            named_children(block)
                .into_iter()
                .filter(|c| c.kind() != "comment")
                .collect()
        },
        ts_call_stmt,
    )
}

pub(crate) fn ts_pat<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                out.push((text(cx, cur), pos_of(cur)));
            }
            "object_pattern" | "array_pattern" | "rest_pattern" => {
                stack.extend(named_children(cur));
            }
            "pair_pattern" => {
                if let Some(v) = child_by_field(cur, "value") {
                    stack.push(v);
                }
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(l) = child_by_field(cur, "left") {
                    stack.push(l);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn ts_declared<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<(&'s str, Pos)> {
    let mut out = Vec::new();
    for d in named_children(n) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = child_by_field(d, "name") {
            ts_pat(cx, name, &mut out);
        }
    }
    out
}

pub(crate) fn ts_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" | "this" => return Some((text(cx, cur), pos_of(cur))),
            "member_expression" => cur = child_by_field(cur, "object")?,
            "subscript_expression" => cur = child_by_field(cur, "object")?,
            "parenthesized_expression" => cur = named_children(cur).into_iter().next()?,
            _ => return None,
        }
    }
}

pub(crate) fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" | "this" | "super" => return Some(text(cx, cur)),
            "member_expression" | "subscript_expression" => cur = child_by_field(cur, "object")?,
            "parenthesized_expression" => cur = cur.named_child(0)?,
            _ => return None,
        }
    }
}

pub(crate) fn ts_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_fragment"]),
        "template_string" => string_arg_shape(cx, n, &["string_fragment"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(cx, n, "+", &["string", "template_string"]) => ArgShape::Concat,
        _ => ArgShape::Other,
    }
}

pub(crate) fn ts_args<'s>(cx: &Cx<'_, 's>, args: Node<'_>) -> Vec<Arg<'s>> {
    let items = match args.kind() {
        "template_string" => vec![args],
        _ => named_children(args),
    };
    items
        .into_iter()
        .map(|c| Arg {
            shape: ts_shape(cx, c),
            start_line: pos_of(c).line,
            end_line: narrow(c.end_position().row) + 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ts_sole_call;

    fn sole_call_in_ts(source: &str) -> Option<usize> {
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser
            .set_language(&language)
            .expect("typescript grammar loads");
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let function = tree.root_node().child(0).expect("one top-level function");
        let body = function.child_by_field_name("body").expect("function body");
        ts_sole_call(body)
    }

    #[test]
    fn returned_call_is_the_sole_call() {
        assert!(sole_call_in_ts("function f() { return g(); }").is_some());
    }

    // Adapter drift, pinned: the JavaScript adapter skips a leading comment before
    // the returned expression; the TypeScript adapter does not.
    #[test]
    fn leading_comment_hides_the_sole_call_in_typescript() {
        assert_eq!(
            sole_call_in_ts("function f() { return /* c */ g(); }"),
            None
        );
    }
}
