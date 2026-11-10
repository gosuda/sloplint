use std::collections::{BTreeMap, BTreeSet};

use sloplint_core::facts::{
    AssertShape, Assertion, FieldCopy, Handler, ScrutineeKind, Stmt, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    Cx, find_named_child, first_named_child, first_named_child_matching, first_of_kind,
    last_named_child, named_children, narrow, pos_of, strip_quotes, text, trimmed,
};

use super::calls::{ps_command_call, ps_invokation_call};
use super::funcs::single_named;

fn switch_condition_variant<'s>(cx: &Cx<'_, 's>, cond: Node<'_>) -> Option<(&'s str, &'s str)> {
    let mut stack = vec![cond];
    while let Some(current) = stack.pop() {
        if current.kind() == "member_access"
            && let (Some(ty), Some(member)) = (
                first_of_kind(current, &["type_literal"]),
                first_of_kind(current, &["member_name"]),
            )
        {
            return Some((text(cx, ty), text(cx, member)));
        }
        stack.extend(named_children(current));
    }
    None
}

fn switch_variant_facts<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (bool, u32) {
    let Some(parent) = cx.parent() else {
        return (false, 0);
    };
    let mut by_type: BTreeMap<&'s str, BTreeSet<&'s str>> = BTreeMap::new();
    for sib in named_children(parent) {
        if sib.id() == n.id() || sib.kind() != "switch_clause" {
            continue;
        }
        let Some(cond) = first_of_kind(sib, &["switch_clause_condition"]) else {
            continue;
        };
        if let Some((ty, variant)) = switch_condition_variant(cx, cond) {
            by_type.entry(ty).or_default().insert(variant);
        }
    }
    let best = by_type.values().map(|set| set.len()).max().unwrap_or(0);
    (best >= 2, narrow(best))
}

pub(crate) fn ps_match_arm(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "switch_clause" {
        return None;
    }
    let arm = |enum_like: bool, variant_count: u32| WildcardArm {
        pos: pos_of(n),
        enum_like,
        scrutinee: ScrutineeKind::Other,
        variant_count,
        has_nullable_ctor: false,
    };
    match first_of_kind(n, &["switch_clause_condition"]) {
        None => Some(arm(false, 0)),
        Some(cond) => {
            if text(cx, cond) != "default" {
                return None;
            }
            let (enum_like, variant_count) = switch_variant_facts(cx, n);
            Some(arm(enum_like, variant_count))
        }
    }
}

fn ps_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>) -> Stmt<'s> {
    if stmt.kind() == "command" {
        return match ps_command_call(cx, stmt) {
            Some(head) => Stmt::Call {
                callee: head.callee,
                kind: head.kind,
            },
            None => Stmt::Other,
        };
    }
    if stmt.kind() == "invokation_expression" {
        return match ps_invokation_call(cx, stmt) {
            Some(head) => Stmt::Call {
                callee: head.callee,
                kind: head.kind,
            },
            None => Stmt::Other,
        };
    }
    if stmt.kind() == "pipeline" || stmt.kind() == "pipeline_chain" {
        return match single_named(stmt) {
            Some(one) => ps_stmt(cx, one),
            None => Stmt::Other,
        };
    }
    if stmt.kind() == "flow_control_statement" && trimmed(cx, stmt).starts_with("throw") {
        return match last_named_child(stmt) {
            None => Stmt::Rethrow,
            Some(last) => {
                let t = text(cx, last);
                let known = t == "$_" || t == "$PSItem" || t == "$_.Exception";
                if known { Stmt::Rethrow } else { Stmt::Other }
            }
        };
    }
    Stmt::Other
}

fn is_comparison_param(t: &str) -> bool {
    matches!(
        t,
        "-Be"
            | "-BeExactly"
            | "-BeGreaterThan"
            | "-BeGreaterOrEqual"
            | "-BeLessThan"
            | "-BeLessOrEqual"
            | "-BeLike"
            | "-Match"
            | "-Contain"
    )
}

pub(crate) fn ps_handler<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let block = first_of_kind(n, &["statement_block"])?;
    let list = first_of_kind(block, &["statement_list"]).unwrap_or(block);
    let mut stmts = Vec::new();
    for stmt in named_children(list) {
        if stmt.kind() == "comment" {
            continue;
        }
        stmts.push(ps_stmt(cx, stmt));
    }
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

pub(crate) fn ps_struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "hash_literal_expression" {
        return None;
    }
    let entries: Vec<Node<'_>> = match first_of_kind(n, &["hash_literal_body"]) {
        Some(body) => named_children(body),
        None => named_children(n),
    }
    .into_iter()
    .filter(|c| c.kind() == "hash_entry")
    .collect();
    if entries.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    for &entry in &entries {
        let key = find_named_child(entry, |c| c.kind() == "key_expression")?;
        let value = find_named_child(entry, |c| c.kind() != "key_expression")?;
        let want = strip_quotes(text(cx, key));
        let unwrapped = super::calls::ps_arg_value(value);
        if unwrapped.kind() != "member_access" {
            return None;
        }
        let obj = first_named_child(unwrapped)?;
        if obj.kind() != "variable" {
            return None;
        }
        let member = first_of_kind(unwrapped, &["member_name"])?;
        if text(cx, member) != want {
            return None;
        }
        let found = super::funcs::ps_var_name(text(cx, obj));
        if source.is_some_and(|s| s != found) {
            return None;
        }
        source = Some(found);
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(entries.len()),
        source,
    })
}

pub(crate) fn ps_assertion(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
    if n.kind() != "command" {
        return None;
    }
    let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
    if !text(cx, name).eq_ignore_ascii_case("Should") {
        return None;
    }
    let mut has_message = false;
    let mut comparison = false;
    let kids = match first_of_kind(n, &["command_elements"]) {
        Some(list) => named_children(list),
        None => named_children(n),
    };
    for child in kids {
        if child.kind() != "command_parameter" {
            continue;
        }
        let t = trimmed(cx, child);
        if t.starts_with("-Because") {
            has_message = true;
        }
        if is_comparison_param(t) {
            comparison = true;
        }
    }
    let shape = AssertShape::Other;
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
