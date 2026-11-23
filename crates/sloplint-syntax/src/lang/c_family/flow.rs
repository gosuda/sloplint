use sloplint_core::facts::Pos;
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use crate::hooks::{
    Cx, any_named_child, child_by_field, find_named_child, pos_of, rfind_named_child,
};

pub(crate) fn redundant_else(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    let block = match cx.lang {
        Lang::C | Lang::Cpp => "compound_statement",
        Lang::Java | Lang::CSharp => "block",
        _ => return None,
    };
    if n.kind() != "if_statement"
        || !cx.parent().is_some_and(|parent| parent.kind() == block)
        || !ordinary_condition(n, cx.lang)
    {
        return None;
    }
    let alternative = child_by_field(n, "alternative")?;
    let body = if alternative.kind() == "else_clause" {
        find_named_child(alternative, executable)?
    } else {
        alternative
    };
    if body.kind() == "if_statement" || !scope_neutral(body, cx.lang) {
        return None;
    }
    if body.kind() == block {
        find_named_child(body, executable)?;
    } else if !executable(body) {
        return None;
    }
    let consequence = child_by_field(n, "consequence")?;
    let last = if consequence.kind() == block {
        rfind_named_child(consequence, executable)?
    } else {
        consequence
    };
    if !matches!(
        last.kind(),
        "return_statement" | "throw_statement" | "break_statement" | "continue_statement"
    ) {
        return None;
    }
    if alternative.kind() == "else_clause" {
        Some(pos_of(alternative))
    } else {
        let mut cursor = n.walk();
        n.children(&mut cursor)
            .find(|child| child.kind() == "else")
            .map(pos_of)
    }
}

fn ordinary_condition(n: Node<'_>, lang: Lang) -> bool {
    let mut cursor = n.walk();
    if n.children(&mut cursor).any(|child| {
        matches!(child.kind(), "constexpr" | "consteval") || child.kind().starts_with("preproc_")
    }) {
        return false;
    }
    let Some(condition) = child_by_field(n, "condition") else {
        return false;
    };
    if lang == Lang::Cpp
        && (condition.kind() != "condition_clause"
            || child_by_field(condition, "initializer").is_some())
    {
        return false;
    }
    scope_neutral(condition, lang)
}

fn executable(n: Node<'_>) -> bool {
    if matches!(
        n.kind(),
        "comment" | "line_comment" | "block_comment" | "empty_statement"
    ) {
        return false;
    }
    n.kind() != "expression_statement" || any_named_child(n, executable)
}

/// Nested blocks remain in the suggested rewrite, so their declarations and
/// cleanup lifetimes stay enclosed. Bindings outside those blocks would escape.
fn scope_neutral(root: Node<'_>, lang: Lang) -> bool {
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let kind = node.kind();
        if kind.starts_with("preproc_")
            || kind.ends_with("_declaration")
            || kind.ends_with("_definition")
            || matches!(
                kind,
                "declaration"
                    | "declaration_expression"
                    | "local_declaration_statement"
                    | "local_function_statement"
                    | "labeled_statement"
                    | "dangling_label"
            )
            || (lang == Lang::Java
                && (kind.ends_with("_pattern")
                    || (kind == "instanceof_expression"
                        && (child_by_field(node, "name").is_some()
                            || child_by_field(node, "pattern").is_some()))))
            || (lang == Lang::CSharp
                && matches!(
                    kind,
                    "declaration_pattern" | "var_pattern" | "recursive_pattern"
                ))
        {
            return false;
        }
        let retained_scope = node.id() != root.id()
            && matches!(
                kind,
                "compound_statement"
                    | "block"
                    | "lambda_expression"
                    | "anonymous_method_expression"
                    | "class_body"
            );
        if !retained_scope && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return true;
            }
        }
    }
}
