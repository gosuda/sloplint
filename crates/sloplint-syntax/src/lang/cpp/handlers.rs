//! Cpp handlers, catch statements, and ambient facts.

use sloplint_core::facts::{AmbientKind, CallKind, Pos, ScrutineeKind, Stmt};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_named_child, first_named_child_matching, named_children, narrow, text,
};
use crate::lang::c_family::{self, CPP_DECLARATORS, word};

pub(crate) fn wildcard_info(cx: &Cx<'_, '_>) -> (ScrutineeKind, u32, bool) {
    let mut body = None;
    let mut switch_node = None;
    if let Some(parent) = cx.parent() {
        if parent.kind() == "switch_statement" {
            body = Some(parent);
            switch_node = Some(parent);
        } else if parent.kind() == "compound_statement" {
            body = Some(parent);
            switch_node = cx
                .parent_of(parent)
                .filter(|found| found.kind() == "switch_statement");
        }
    }
    let scrutinee = switch_node
        .map(scrutinee_of)
        .unwrap_or(ScrutineeKind::Other);
    let (variants, enum_like) = body
        .map(|found| count_labels(cx, found))
        .unwrap_or((0, false));
    (scrutinee, narrow(variants), enum_like)
}

fn scrutinee_of(switch_node: Node<'_>) -> ScrutineeKind {
    let mut value =
        child_by_field(switch_node, "condition").and_then(|cond| child_by_field(cond, "value"));
    if value.is_some_and(|v| v.kind() == "parenthesized_expression") {
        value = value.and_then(first_named_child);
    }
    match value.map(|v| v.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_expression") => ScrutineeKind::Field,
        Some("qualified_identifier") => ScrutineeKind::Path,
        _ => ScrutineeKind::Other,
    }
}

fn count_labels(cx: &Cx<'_, '_>, body: Node<'_>) -> (usize, bool) {
    let declared = super::types::declared_names(cx);
    let mut variants = 0;
    let mut qualified = 0;
    let mut resolved = 0;
    for sib in named_children(body) {
        if sib.kind() != "case_statement" {
            continue;
        }
        let Some(value) = child_by_field(sib, "value") else {
            continue;
        };
        variants += 1;
        if value.kind() == "qualified_identifier" {
            qualified += 1;
        } else if value.kind() == "identifier" && declared.contains(&text(cx, value)) {
            resolved += 1;
        }
    }
    (variants, qualified >= 2 || resolved >= 2)
}

pub(crate) fn catch_binding<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let params = child_by_field(n, "parameters")?;
    let decl_node = first_named_child_matching(params, &["parameter_declaration"])?;
    let found = c_family::unwrap(
        child_by_field(decl_node, "declarator")?,
        CPP_DECLARATORS.wrap,
    );
    (found.kind() == "identifier").then_some(text(cx, found))
}

pub(crate) fn catch_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, bound: Option<&'s str>) -> Stmt<'s> {
    if stmt.kind() == "expression_statement" {
        let expr = child_by_field(stmt, "expression").or_else(|| first_named_child(stmt));
        if expr.is_some_and(|e| e.kind() == "call_expression") {
            let callee = expr
                .and_then(|e| child_by_field(e, "function"))
                .map(|f| text(cx, f))
                .unwrap_or("");
            return Stmt::Call {
                callee,
                kind: CallKind::Plain,
            };
        }
        return Stmt::Other;
    }
    if stmt.kind() == "throw_statement" {
        let thrown = first_named_child(stmt);
        match thrown {
            None => return Stmt::Rethrow,
            Some(found) if found.kind() == "identifier" && Some(text(cx, found)) == bound => {
                return Stmt::Rethrow;
            }
            _ => return Stmt::Other,
        }
    }
    Stmt::Other
}

pub(crate) fn global_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if word(
        cx,
        n,
        &["type_qualifier", "storage_class_specifier"],
        &["const", "constexpr", "extern"],
    ) {
        return;
    }
    let (names, func) = c_family::decl_info(cx, n, CPP_DECLARATORS.wrap);
    if func {
        return;
    }
    if let Some((name, pos)) = names.into_iter().next() {
        out.push((AmbientKind::GlobalMutable, name, pos));
    }
}

pub(crate) fn env_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if callee_holds_a_paren(cx, n) {
        return;
    }
    c_family::env_ambient(cx, n, out, CPP_DECLARATORS.envs);
}

fn callee_holds_a_paren(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "function")
        .is_some_and(|f| text(cx, f).bytes().any(|b| b == b'(' || b == b')'))
}
