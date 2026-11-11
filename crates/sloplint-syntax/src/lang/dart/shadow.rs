use tree_sitter::Node;

use sloplint_core::facts::Pos;

use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};

use super::funcs::closure_shadow_params;
use super::syntax::has_anon;

pub(crate) fn shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "function_expression" => closure_shadow_params(cx, n, out),
        "for_statement" => for_in_shadow_names(cx, n, out),
        "catch_clause" => catch_shadow_names(cx, n, out),
        "if_statement" => if_case_shadow_names(cx, n, out),
        _ => {}
    }
}

pub(crate) fn switch_case_decl(cx: &Cx<'_, '_>, node: Node<'_>) -> bool {
    cx.parent_of(node).is_some_and(|parent| {
        matches!(
            parent.kind(),
            "switch_statement_case" | "switch_statement_default"
        )
    })
}

pub(crate) fn closure_bare_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = Vec::new();
    for c in named_children(n) {
        if c.kind() == "formal_parameter_list" {
            stack.extend(named_children(c));
        }
    }
    while let Some(cur) = stack.pop() {
        if cur.kind() == "formal_parameter" {
            if child_by_field(cur, "name").is_none() {
                push_direct_identifiers(cx, cur, out);
            }
        } else if cur.kind() == "optional_formal_parameters" {
            stack.extend(named_children(cur));
        }
    }
}

fn push_direct_identifiers<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    for kid in named_children(n) {
        if kid.kind() == "identifier" {
            out.push(text(cx, kid));
        }
    }
}

pub(crate) fn push_pattern_names<'s>(
    cx: &Cx<'_, 's>,
    seed: Vec<Node<'_>>,
    out: &mut Vec<(&'s str, Pos)>,
) {
    let mut stack = seed;
    stack.reverse();
    while let Some(cur) = stack.pop() {
        if cur.kind() == "variable_pattern" {
            if let Some(nm) = child_by_field(cur, "name")
                && text(cx, nm) != "_"
            {
                out.push((text(cx, nm), pos_of(nm)));
            }
        } else if cur.kind() == "identifier" {
            if text(cx, cur) != "_" {
                out.push((text(cx, cur), pos_of(cur)));
            }
        } else if !matches!(
            cur.kind(),
            "qualified"
                | "static_member_shorthand"
                | "symbol_literal"
                | "label"
                | "type"
                | "type_arguments"
                | "dotted_identifier_list"
                | "annotation"
        ) {
            stack.extend(named_children(cur).into_iter().rev());
        }
    }
}

fn for_in_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(value) = child_by_field(n, "value") else {
        return;
    };
    if !has_anon(n, "var")
        && !has_anon(n, "final")
        && !named_children(n)
            .into_iter()
            .any(|c| matches!(c.kind(), "type" | "function_type"))
    {
        return;
    }
    let mut seed = Vec::new();
    let body = child_by_field(n, "body").map(|v| v.id());
    for c in named_children(n) {
        if c.id() != value.id() && Some(c.id()) != body {
            seed.push(c);
        }
    }
    let mut names = Vec::new();
    push_pattern_names(cx, seed, &mut names);
    for (name, _) in names {
        out.push(name);
    }
}

fn catch_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if let Some(e) = child_by_field(n, "exception") {
        out.push(text(cx, e));
    }
    if let Some(s) = child_by_field(n, "stack_trace") {
        out.push(text(cx, s));
    }
}

fn if_case_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut seed = Vec::new();
    let mut in_pattern = false;
    for c in (0..n.child_count()).filter_map(|i| n.child(i)) {
        if !c.is_named() {
            if c.kind() == "case" {
                in_pattern = true;
            } else if in_pattern && (c.kind() == "when" || c.kind() == ")") {
                break;
            }
        } else if in_pattern {
            seed.push(c);
        }
    }
    let mut names = Vec::new();
    push_pattern_names(cx, seed, &mut names);
    for (name, _) in names {
        out.push(name);
    }
}
