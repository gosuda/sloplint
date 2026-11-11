//! Bindings, writes, and ambient state for JavaScript.

use super::funcs::closure_shadow_params;
use super::{first_code, top_level};
use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, find_named_child, named_children, pos_of, text,
};
use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

const ENV_OBJECT: &str = "process.env";

enum PatternFrame<'t> {
    Pattern(Node<'t>),
    Member(Node<'t>),
}

pub(crate) fn declared_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    for declarator in named_children(n) {
        let Some(name) = child_by_field(declarator, "name") else {
            continue;
        };
        if declarator.kind() == "variable_declarator" {
            collect_pattern(cx, name, out);
        }
    }
}

fn collect_pattern<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending = vec![PatternFrame::Pattern(n)];
    while let Some(frame) = pending.pop() {
        match frame {
            PatternFrame::Pattern(node) => expand_pattern(cx, node, out, &mut pending),
            PatternFrame::Member(node) => expand_member(cx, node, out, &mut pending),
        }
    }
}

fn expand_pattern<'s, 't>(
    cx: &Cx<'_, 's>,
    n: Node<'t>,
    out: &mut Vec<(&'s str, Pos)>,
    pending: &mut Vec<PatternFrame<'t>>,
) {
    match n.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => {
            out.push((text(cx, n), pos_of(n)));
        }
        "object_pattern" => {
            pending.extend(
                named_children(n)
                    .into_iter()
                    .rev()
                    .map(PatternFrame::Member),
            );
        }
        "array_pattern" => {
            pending.extend(
                named_children(n)
                    .into_iter()
                    .rev()
                    .map(PatternFrame::Pattern),
            );
        }
        "rest_pattern" => pending.extend(first_code(n).map(PatternFrame::Pattern)),
        "assignment_pattern" => {
            pending.extend(child_by_field(n, "left").map(PatternFrame::Pattern));
        }
        "parenthesized_expression" => {
            pending.extend(first_code(n).map(PatternFrame::Pattern));
        }
        _ => {}
    }
}

fn expand_member<'s, 't>(
    cx: &Cx<'_, 's>,
    n: Node<'t>,
    out: &mut Vec<(&'s str, Pos)>,
    pending: &mut Vec<PatternFrame<'t>>,
) {
    match n.kind() {
        "shorthand_property_identifier_pattern" => out.push((text(cx, n), pos_of(n))),
        "pair_pattern" => pending.extend(child_by_field(n, "value").map(PatternFrame::Pattern)),
        "object_assignment_pattern" => {
            pending.extend(child_by_field(n, "left").map(PatternFrame::Pattern));
        }
        "rest_pattern" => pending.extend(first_code(n).map(PatternFrame::Pattern)),
        _ => {}
    }
}

pub(crate) fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some((text(cx, current), pos_of(current))),
            "member_expression" | "subscript_expression" => child_by_field(current, "object")?,
            "parenthesized_expression" => first_code(current)?,
            _ => return None,
        };
    }
}

pub(crate) fn decl_names_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    match n.kind() {
        "lexical_declaration" => {
            if child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "let") {
                declared_names(cx, n, out);
            }
        }
        "variable_declaration" => declared_names(cx, n, out),
        "for_in_statement" => for_in_var_names(cx, n, out),
        _ => {}
    }
}

fn for_in_var_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if !child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "var") {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        collect_pattern(cx, left, out);
    }
}

pub(crate) fn pattern_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut found = Vec::new();
    collect_pattern(cx, n, &mut found);
    out.extend(found.into_iter().map(|(name, _)| name));
}

pub(crate) fn shadow_kind_for(kind: &str) -> bool {
    kind == "lexical_declaration" || kind == "for_in_statement" || kind == "catch_clause"
}

pub(crate) fn function_body_depth(depth: usize, cx: &Cx<'_, '_>) -> usize {
    let mut parent = depth;
    for ancestor in cx.ancestors() {
        parent = parent.saturating_sub(1);
        if is_fn_like(ancestor.kind()) {
            return parent.saturating_add(1);
        }
    }
    depth.saturating_sub(1)
}

pub(crate) fn shadow_names_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if n.kind() == "catch_clause" {
        if let Some(param) = child_by_field(n, "parameter") {
            pattern_names(cx, param, out);
        }
        return;
    }
    if n.kind() == "for_in_statement" {
        shadow_for_in_names(cx, n, out);
        return;
    }
    if is_fn_like(n.kind()) {
        out.extend(closure_shadow_params(cx, n));
        if let Some(body) = child_by_field(n, "body") {
            hoisted_shadows_for(cx, body, out);
        }
        return;
    }
    if !child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "const") {
        return;
    }
    let mut found = Vec::new();
    declared_names(cx, n, &mut found);
    out.extend(found.into_iter().map(|(name, _)| name));
}

fn shadow_for_in_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if !child_by_field(n, "kind").is_some_and(|kind| {
        let keyword = text(cx, kind);
        keyword == "let" || keyword == "const"
    }) {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        pattern_names(cx, left, out);
    }
}

pub(crate) fn write_roots_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    let target = match n.kind() {
        "assignment_expression" | "augmented_assignment_expression" => child_by_field(n, "left"),
        "update_expression" => child_by_field(n, "argument"),
        _ => None,
    };
    let Some(target) = target else {
        return;
    };
    let mut names = Vec::new();
    collect_pattern(cx, target, &mut names);
    if names.is_empty() {
        names.extend(write_root(cx, target));
    }
    out.extend(names);
}

fn is_write_target(parent: Option<Node<'_>>, n: Node<'_>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    match parent.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            child_by_field(parent, "left").is_some_and(|left| left.id() == n.id())
        }
        "update_expression" => {
            child_by_field(parent, "argument").is_some_and(|argument| argument.id() == n.id())
        }
        _ => false,
    }
}

fn is_env_base(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "member_expression" {
        return false;
    }
    let (Some(object), Some(property)) =
        (child_by_field(n, "object"), child_by_field(n, "property"))
    else {
        return false;
    };
    if text(cx, property) != "env" {
        return false;
    }
    match object.kind() {
        "identifier" => text(cx, object) == "process",
        "meta_property" => text(cx, object) == "import.meta",
        _ => false,
    }
}

fn env_rooted(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        if is_env_base(cx, current) {
            return true;
        }
        match current.kind() {
            "member_expression" | "subscript_expression" => {
                let Some(object) = child_by_field(current, "object") else {
                    return false;
                };
                current = object;
            }
            _ => return false,
        }
    }
}

fn under_env_store(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let ancestors: Vec<Node<'_>> = cx.ancestors().collect();
    let mut child = n;
    for (index, node) in ancestors.iter().enumerate() {
        match node.kind() {
            "member_expression" | "subscript_expression" => {
                if !child_by_field(*node, "object").is_some_and(|object| object.id() == child.id())
                {
                    return false;
                }
                if is_write_target(ancestors.get(index + 1).copied(), *node) {
                    return true;
                }
                child = *node;
            }
            _ => return false,
        }
    }
    false
}

fn push_env_or_store<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if is_env_base(cx, n) && !is_write_target(cx.parent(), n) && !under_env_store(cx, n) {
        out.push((AmbientKind::EnvRead, text(cx, n), pos_of(n)));
        return;
    }
    if is_write_target(cx.parent(), n) && env_rooted(cx, n) {
        out.push((AmbientKind::GlobalWrite, ENV_OBJECT, pos_of(n)));
    }
}

fn file_mutables<'a, 's>(cx: &'a Cx<'_, 's>) -> &'a [&'s str] {
    cx.file_names(FileSetKind::Scope, || {
        let mut out = Vec::new();
        if let Some(root) = cx.root() {
            for stmt in code_children(root) {
                collect_top_mutable(cx, stmt, &mut out);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    })
}

fn collect_top_mutable<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, out: &mut Vec<&'s str>) {
    let decl = if stmt.kind() == "export_statement" {
        find_named_child(stmt, |child| {
            child.kind() == "lexical_declaration" || child.kind() == "variable_declaration"
        })
    } else if stmt.kind() == "lexical_declaration" || stmt.kind() == "variable_declaration" {
        Some(stmt)
    } else {
        None
    };
    let Some(decl) = decl else {
        return;
    };
    if decl.kind() == "lexical_declaration"
        && child_by_field(decl, "kind").map(|kind| text(cx, kind)) != Some("let")
    {
        return;
    }
    let mut names = Vec::new();
    declared_names(cx, decl, &mut names);
    out.extend(names.into_iter().map(|(name, _)| name));
}
fn is_file_mutable(cx: &Cx<'_, '_>, name: &str) -> bool {
    file_mutables(cx).binary_search(&name).is_ok()
}

fn is_fn_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "generator_function"
            | "class_static_block"
    )
}

pub(crate) fn hoisted_shadows_for<'s>(cx: &Cx<'_, 's>, body: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        for child in named_children(node) {
            hoist_child(cx, child, &mut stack, out);
        }
    }
}

fn hoist_child<'s, 't>(
    cx: &Cx<'_, 's>,
    child: Node<'t>,
    stack: &mut Vec<Node<'t>>,
    out: &mut Vec<&'s str>,
) {
    if is_fn_like(child.kind()) {
        return;
    }
    if child.kind() == "variable_declaration" {
        let mut found = Vec::new();
        declared_names(cx, child, &mut found);
        out.extend(found.into_iter().map(|(name, _)| name));
        return;
    }
    if child.kind() == "for_in_statement" {
        hoist_for_in(cx, child, out);
    }
    if child.child_count() > 0 {
        stack.push(child);
    }
}

fn hoist_for_in<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if !child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "var") {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        pattern_names(cx, left, out);
    }
}
fn push_global_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if !in_function {
        return;
    }
    let target = match n.kind() {
        "assignment_expression" | "augmented_assignment_expression" => child_by_field(n, "left"),
        "update_expression" => child_by_field(n, "argument"),
        _ => None,
    };
    let Some(target) = target else {
        return;
    };
    let mut names = Vec::new();
    collect_pattern(cx, target, &mut names);
    if names.is_empty() {
        names.extend(write_root(cx, target));
    }
    for (name, pos) in names {
        if is_file_mutable(cx, name) {
            out.push((AmbientKind::GlobalWrite, name, pos));
        }
    }
}

fn push_subscript_store<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "subscript_expression" && is_write_target(cx.parent(), n) && env_rooted(cx, n) {
        out.push((AmbientKind::GlobalWrite, ENV_OBJECT, pos_of(n)));
    }
}

fn push_declaration_global<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function || !top_level(cx) {
        return;
    }
    if n.kind() == "lexical_declaration"
        && child_by_field(n, "kind").map(|kind| text(cx, kind)) != Some("let")
    {
        return;
    }
    let mut names = Vec::new();
    declared_names(cx, n, &mut names);
    out.extend(
        names
            .into_iter()
            .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
    );
}

pub(crate) fn ambient_sites<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "lexical_declaration" | "variable_declaration" => {
            push_declaration_global(cx, n, in_function, out);
        }
        "member_expression" => push_env_or_store(cx, n, out),
        "subscript_expression" => push_subscript_store(cx, n, out),
        "assignment_expression" | "augmented_assignment_expression" | "update_expression" => {
            push_global_write(cx, n, in_function, out);
        }
        _ => {}
    }
}
