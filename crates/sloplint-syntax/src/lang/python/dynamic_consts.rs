use std::collections::HashMap;

use tree_sitter::Node;

use crate::hooks::{Cx, FileSetKind, child_by_field, code_children, named_children, text};

use super::dynamic::{is_plain_string, is_plain_value};

pub(crate) fn pair_table_ok(cx: &Cx<'_, '_>, left: Node<'_>, right: Node<'_>, name: &str) -> bool {
    let Some(targets) = flat_targets(cx, left) else {
        return false;
    };
    let Some(at) = position_of(&targets, name) else {
        return false;
    };
    rows_all_literal(cx, right, targets.len(), at)
}

fn flat_targets<'s>(cx: &Cx<'_, 's>, left: Node<'_>) -> Option<Vec<&'s str>> {
    if !matches!(
        left.kind(),
        "pattern_list" | "tuple_pattern" | "list_pattern"
    ) {
        return None;
    }
    let kids = code_children(left);
    if kids.len() < 2 {
        return None;
    }
    let mut out = Vec::new();
    for kid in kids {
        if kid.kind() != "identifier" {
            return None;
        }
        out.push(text(cx, kid));
    }
    Some(out)
}

fn position_of(targets: &[&str], name: &str) -> Option<usize> {
    let mut found = None;
    for (idx, target) in targets.iter().enumerate() {
        if *target == name && found.replace(idx).is_some() {
            return None;
        }
    }
    found
}

fn rows_all_literal(cx: &Cx<'_, '_>, right: Node<'_>, width: usize, at: usize) -> bool {
    if !matches!(right.kind(), "list" | "tuple") {
        return false;
    }
    let rows = code_children(right);
    if rows.is_empty() {
        return false;
    }
    rows.iter().all(|row| row_fits(cx, *row, width, at))
}

fn row_fits(cx: &Cx<'_, '_>, row: Node<'_>, width: usize, at: usize) -> bool {
    if !matches!(row.kind(), "list" | "tuple") {
        return false;
    }
    let cells = code_children(row);
    if cells.len() != width {
        return false;
    }
    cells.iter().enumerate().all(|(idx, cell)| {
        if idx == at {
            is_plain_value(*cell, cx)
        } else {
            is_any_literal(*cell, cx)
        }
    })
}

fn is_any_literal(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    if is_plain_value(n, cx) {
        return true;
    }
    matches!(n.kind(), "integer" | "float" | "none" | "true" | "false")
}

pub(crate) fn is_module_string_const(cx: &Cx<'_, '_>, arg: Node<'_>) -> bool {
    if arg.kind() != "identifier" {
        return false;
    }
    file_string_consts(cx).contains(&text(cx, arg))
}

fn file_string_consts<'c, 's>(cx: &'c Cx<'_, 's>) -> &'c [&'s str] {
    cx.file_names(FileSetKind::Member, || collect_string_consts(cx))
}

const FILE_CAP: usize = 100000;

fn collect_string_consts<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for stmt in named_children(root) {
        if let Some(name) = module_string_target(cx, stmt) {
            candidates.push(name);
        }
    }
    if candidates.is_empty() {
        return Vec::new();
    }
    let counts = count_bindings(cx, root);
    candidates
        .into_iter()
        .filter(|name| counts.get(name).is_some_and(|at| *at == 1))
        .collect()
}

fn module_string_target<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>) -> Option<&'s str> {
    let assign = unwrap_assign(stmt)?;
    let left = child_by_field(assign, "left")?;
    if left.kind() != "identifier" {
        return None;
    }
    let right = child_by_field(assign, "right")?;
    if !is_plain_string(right) {
        return None;
    }
    Some(text(cx, left))
}

fn unwrap_assign(stmt: Node<'_>) -> Option<Node<'_>> {
    if stmt.kind() == "assignment" {
        return Some(stmt);
    }
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let mut found = None;
    for kid in code_children(stmt) {
        if kid.kind() != "assignment" || found.is_some() {
            return None;
        }
        found = Some(kid);
    }
    found
}

fn count_bindings<'s>(cx: &Cx<'_, 's>, root: Node<'_>) -> HashMap<&'s str, usize> {
    let mut counts = HashMap::new();
    let mut stack = vec![root];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > FILE_CAP {
            return HashMap::new();
        }
        count_node(cx, cur, &mut counts);
        stack.extend(named_children(cur));
    }
    counts
}

fn count_node<'s>(cx: &Cx<'_, 's>, cur: Node<'_>, counts: &mut HashMap<&'s str, usize>) {
    if let Some(root) = binding_root(cur) {
        count_names(cx, root, counts);
    }
    if whole_subtree_binds(cur) {
        count_names(cx, cur, counts);
    }
    if matches!(cur.kind(), "function_definition" | "class_definition")
        && let Some(name) = child_by_field(cur, "name")
    {
        bump(cx, name, counts);
    }
}

fn binding_root(cur: Node<'_>) -> Option<Node<'_>> {
    if matches!(
        cur.kind(),
        "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause"
    ) {
        return child_by_field(cur, "left");
    }
    if cur.kind() == "named_expression" {
        return child_by_field(cur, "name");
    }
    if matches!(cur.kind(), "except_clause" | "as_pattern") {
        return child_by_field(cur, "alias");
    }
    None
}

fn whole_subtree_binds(cur: Node<'_>) -> bool {
    matches!(
        cur.kind(),
        "parameters"
            | "lambda_parameters"
            | "global_statement"
            | "nonlocal_statement"
            | "delete_statement"
            | "import_statement"
            | "import_from_statement"
    )
}

fn count_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, counts: &mut HashMap<&'s str, usize>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            bump(cx, cur, counts);
        } else {
            stack.extend(named_children(cur));
        }
    }
}

fn bump<'s>(cx: &Cx<'_, 's>, n: Node<'_>, counts: &mut HashMap<&'s str, usize>) {
    *counts.entry(text(cx, n)).or_insert(0) += 1;
}
