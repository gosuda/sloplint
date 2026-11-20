//! Module-level facts for the Dafny adapter: declarations, writes,
//! imports, type definitions, wildcard match arms, and assertions.

use std::borrow::Cow;

use sloplint_core::facts::{
    AssertShape, Assertion, Import, ImportKind, ImportScope, Pos, ScrutineeKind, TypeDef,
    WildcardArm,
};
use tree_sitter::Node;

use super::funcs::{dafny_visibility, is_bool_literal};
use super::shapes::{
    leftmost_ident, pattern_names, scrutinee_kind, strip_string_quotes, unwrap_expr,
    wildcard_pattern,
};
use crate::hooks::{
    Cx, TypeHead, child_by_field, first_named_child_matching, named_children, pos_of, text, trimmed,
};

pub(crate) fn decl_names_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    match n.kind() {
        "local_var_decl" => {
            if let Some(name) = child_by_field(n, "name") {
                out.push((text(cx, name), pos_of(name)));
            }
        }
        "for_statement" => {
            if let Some(id) = first_named_child_matching(n, &["identifier"]) {
                out.push((text(cx, id), pos_of(id)));
            }
        }
        "var_decl_statement" => {
            for child in named_children(n) {
                match child.kind() {
                    "local_var_decl" => {
                        if let Some(name) = child_by_field(child, "name") {
                            out.push((text(cx, name), pos_of(name)));
                        }
                    }
                    "case_pattern" => pattern_names(cx, child, out),
                    _ => {}
                }
            }
        }
        "fn_let_expression" | "let_expression" | "let_or_fail_expression" => {
            for child in named_children(n) {
                if child.kind() == "case_pattern" {
                    pattern_names(cx, child, out);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn write_roots_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "assign_statement" {
        return;
    }
    for child in named_children(n) {
        if child.kind() == "rhs" {
            break;
        }
        if let Some(root) = leftmost_ident(cx, child) {
            out.push(root);
        }
    }
}

pub(crate) fn imports_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let pos = pos_of(n);
    match n.kind() {
        "include_directive" => {
            let Some(literal) = first_named_child_matching(n, &["string_literal"]) else {
                return;
            };
            out.push(Import {
                pos,
                module: Cow::Borrowed(strip_string_quotes(trimmed(cx, literal))),
                name: "",
                alias: None,
                kind: ImportKind::SideEffect,
                scope: ImportScope::Module,
                in_test: false,
                is_reexport: false,
                external_root: None,
            });
        }
        "module_import" => {
            let path = first_named_child_matching(n, &["qualified_name"])
                .map(|q| text(cx, q))
                .unwrap_or("");
            if path.is_empty() {
                return;
            }
            let local = child_by_field(n, "name")
                .map(|name| trimmed(cx, name))
                .unwrap_or("");
            out.push(Import {
                pos,
                module: Cow::Borrowed(path),
                name: local,
                alias: None,
                kind: ImportKind::Namespace,
                scope: ImportScope::Module,
                in_test: false,
                is_reexport: false,
                external_root: None,
            });
        }
        _ => {}
    }
}

pub(crate) fn typedef_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if !matches!(
        n.kind(),
        "class_decl" | "trait_decl" | "datatype_decl" | "newtype_decl" | "synonym_type_decl"
    ) {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let name = trimmed(cx, name_node);
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility: dafny_visibility(name),
            abstraction: n.kind() == "trait_decl",
        }),
        impls: Vec::new(),
        container: None,
    })
}

pub(crate) fn match_arm_for(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<WildcardArm> {
    let pattern = named_children(n)
        .into_iter()
        .find(|child| child.kind() != "attribute")?;
    if !wildcard_pattern(cx, pattern) {
        return None;
    }
    let mut variant_count = 0u32;
    let mut scrutinee = None;
    let mut found = false;
    for ancestor in cx.ancestors() {
        if !matches!(
            ancestor.kind(),
            "match_statement" | "match_expression" | "fn_match_expression"
        ) {
            continue;
        }
        found = true;
        let mut cursor = ancestor.walk();
        for child in ancestor.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            if child.kind() == "case_stmt_clause"
                || child.kind() == "case_expr_clause"
                || child.kind() == "fn_case_expr_clause"
            {
                variant_count += 1;
            } else if scrutinee.is_none() {
                scrutinee = Some(child);
            }
        }
        break;
    }
    if !found {
        return None;
    }
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 2,
        scrutinee: scrutinee
            .map(|node| scrutinee_kind(cx, node))
            .unwrap_or(ScrutineeKind::Other),
        variant_count,
        has_nullable_ctor: false,
    })
}

pub(crate) fn assertion_for(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() == "assert_statement" || n.kind() == "fn_assert_expression" {
        // Dafny `assert` takes no message by design; flagging it would
        // demand syntax the language has no form for.
        return Some(Assertion {
            pos: pos_of(n),
            has_message: false,
            comparison: false,
            shape: AssertShape::Other,
            in_typecheck: false,
            value_is_bool_literal: false,
        });
    }
    if n.kind() != "expect_statement" && n.kind() != "fn_expect_expression" {
        return None;
    }
    // The message is the expression after `,`; anything after `;` is the
    // `fn_expect` continuation and does not count as a message.
    let mut tested = None;
    let mut message = false;
    let mut seen_comma = false;
    let mut cursor = n.walk();
    for child in n.children(&mut cursor) {
        if !child.is_named() {
            let token = text(cx, child);
            if token == "," {
                seen_comma = true;
            } else if token == ";" {
                break;
            }
            continue;
        }
        if child.kind() == "attribute" {
            continue;
        }
        if tested.is_none() {
            tested = Some(child);
        } else if seen_comma {
            message = true;
        }
    }
    let tested = tested?;
    let unwrapped = unwrap_expr(tested);
    let shape = match unwrapped.kind() {
        "identifier" | "name_segment" => AssertShape::BareName,
        "relational_expression" => {
            let mut cursor = unwrapped.walk();
            let top_eq = unwrapped.children(&mut cursor).any(|child| {
                !child.is_named() && matches!(text(cx, child), "==" | "!=" | "==#" | "!=#")
            });
            if top_eq {
                AssertShape::TopEqNe
            } else {
                AssertShape::Other
            }
        }
        _ => AssertShape::Other,
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message: message,
        comparison: shape == AssertShape::TopEqNe,
        shape,
        in_typecheck: false,
        value_is_bool_literal: is_bool_literal(unwrapped),
    })
}
