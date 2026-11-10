use sloplint_core::facts::{AssertShape, Assertion, CallKind, FieldCopy, ScrutineeKind, Stmt};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, count_named_child, find_named_child, first_of_kind, has_child_kind,
    last_segment, named_children, narrow, pos_of, rfind_named_child, strip_quotes, text,
};

use super::calls::{call_args, ruby_call_kind, ruby_callee};
use super::field_text;

pub(crate) fn has_constant_pattern(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        let kind = current.kind();
        if kind == "constant" || kind == "scope_resolution" {
            return true;
        }
        stack.extend(named_children(current));
    }
    false
}
pub(crate) fn sibling_has_constant(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    for sib in named_children(parent) {
        if sib.id() == n.id() || (sib.kind() != "when" && sib.kind() != "in_clause") {
            continue;
        }
        if let Some(pattern) = child_by_field(sib, "pattern") {
            if has_constant_pattern(pattern) {
                return true;
            }
            continue;
        }
        for child in named_children(sib) {
            if child.kind() == "pattern" && has_constant_pattern(child) {
                return true;
            }
        }
    }
    false
}
pub(crate) fn strip_arrow(t: &str) -> &str {
    t.trim()
        .strip_prefix("=>")
        .map(str::trim)
        .unwrap_or(t.trim())
}

pub(crate) fn case_scrutinee_kind(n: Node<'_>) -> ScrutineeKind {
    let Some(cond) = named_children(n)
        .into_iter()
        .find(|c| !matches!(c.kind(), "when" | "in_clause" | "else" | "comment"))
    else {
        return ScrutineeKind::Other;
    };
    match cond.kind() {
        "identifier" => ScrutineeKind::Ident,
        "call" | "instance_variable" => ScrutineeKind::Field,
        "scope_resolution" => ScrutineeKind::Path,
        _ => ScrutineeKind::Other,
    }
}

fn collect_constant_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "constant" => {
                let name = text(cx, current);
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            "scope_resolution" => {
                let name = last_segment(text(cx, current), &["::"]);
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            _ => stack.extend(named_children(current)),
        }
    }
}

pub(crate) fn sibling_variant_count(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let Some(parent) = cx.parent() else {
        return 0;
    };
    let mut names: Vec<&str> = Vec::new();
    for sib in named_children(parent) {
        if sib.id() == n.id() || (sib.kind() != "when" && sib.kind() != "in_clause") {
            continue;
        }
        if let Some(pattern) = child_by_field(sib, "pattern") {
            collect_constant_names(cx, pattern, &mut names);
            continue;
        }
        for child in named_children(sib) {
            if child.kind() == "pattern" {
                collect_constant_names(cx, child, &mut names);
            }
        }
    }
    narrow(names.len())
}

pub(crate) fn bare_assert_shape(cx: &Cx<'_, '_>, n: Node<'_>) -> AssertShape {
    let args = call_args(n);
    if args.len() != 1 {
        return AssertShape::Other;
    }
    let only = args[0];
    match only.kind() {
        "identifier" | "instance_variable" | "class_variable" | "global_variable" | "constant" => {
            AssertShape::BareName
        }
        "call" => {
            let bare = child_by_field(only, "receiver").is_some()
                && first_of_kind(only, &["argument_list"])
                    .is_none_or(|list| list.named_child_count() == 0)
                && !has_child_kind(only, "block")
                && !has_child_kind(only, "do_block");
            if bare {
                AssertShape::BareName
            } else {
                AssertShape::Other
            }
        }
        "binary" => {
            let op = text(cx, only);
            if op.contains("==") || op.contains("!=") {
                AssertShape::TopEqNe
            } else {
                AssertShape::Other
            }
        }
        _ => AssertShape::Other,
    }
}

pub(crate) fn ruby_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: &str) -> Stmt<'s> {
    if stmt.kind() == "identifier" && text(cx, stmt) == "raise" {
        return Stmt::Rethrow;
    }
    if stmt.kind() != "call" {
        return Stmt::Other;
    }
    let method = field_text(cx, stmt, "method").unwrap_or("");
    if method != "raise" {
        return Stmt::Call {
            callee: ruby_callee(cx, stmt),
            kind: ruby_call_kind(cx, stmt),
        };
    }
    match first_of_kind(stmt, &["argument_list"]) {
        None => Stmt::Rethrow,
        Some(list) => {
            let count = u32::try_from(list.named_child_count()).unwrap_or(u32::MAX);
            let mut rest = (0..count)
                .filter_map(|i| list.named_child(i))
                .filter(|c| c.kind() != "comment");
            let Some(one) = rest.next() else {
                return Stmt::Rethrow;
            };
            if rest.next().is_none() && !binding.is_empty() && text(cx, one) == binding {
                return Stmt::Rethrow;
            }
            Stmt::Call {
                callee: ruby_callee(cx, stmt),
                kind: CallKind::Plain,
            }
        }
    }
}
pub(crate) fn pair_key<'s>(cx: &Cx<'_, 's>, key: Node<'_>) -> Option<&'s str> {
    match key.kind() {
        "hash_key_symbol" => Some(text(cx, key)),
        "simple_symbol" => text(cx, key).strip_prefix(':'),
        "string" => Some(strip_quotes(text(cx, key))),
        _ => None,
    }
}
pub(crate) fn pair_source<'s>(cx: &Cx<'_, 's>, value: Node<'_>, key: &str) -> Option<&'s str> {
    if value.kind() != "call" {
        return None;
    }
    let recv = child_by_field(value, "receiver")?;
    if recv.kind() != "identifier" || field_text(cx, value, "method") != Some(key) {
        return None;
    }
    if let Some(list) = first_of_kind(value, &["argument_list"])
        && list.named_child_count() != 0
    {
        return None;
    }
    if has_child_kind(value, "block") || has_child_kind(value, "do_block") {
        return None;
    }
    Some(text(cx, recv))
}
pub(crate) fn hash_copy<'s>(
    cx: &Cx<'_, 's>,
    scope: Node<'_>,
    pairs: &[Node<'_>],
) -> Option<FieldCopy<'s>> {
    if pairs.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    for pair in pairs {
        if pair.named_child_count() != 2 {
            return None;
        }
        let (Some(key_node), Some(val_node)) = (pair.named_child(0), pair.named_child(1)) else {
            return None;
        };
        let found = pair_source(cx, val_node, pair_key(cx, key_node)?)?;
        if source.is_some_and(|s| s != found) {
            return None;
        }
        source = Some(found);
    }
    Some(FieldCopy {
        pos: pos_of(scope),
        fields: narrow(pairs.len()),
        source,
    })
}
pub(crate) fn is_assign_left(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    cx.parent_of(n)
        .is_some_and(|p| p.kind() == "assignment" || p.kind() == "operator_assignment")
}
pub(crate) fn ruby_env_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    for child in named_children(n) {
        if matches!(
            child.kind(),
            "string" | "simple_symbol" | "identifier" | "constant"
        ) {
            return text(cx, child);
        }
    }
    "ENV"
}
pub(crate) fn minitest_arity(method: &str) -> Option<usize> {
    let rest = method
        .strip_prefix("assert")
        .or_else(|| method.strip_prefix("refute"))?;
    if !rest.is_empty() && !rest.starts_with('_') {
        return None;
    }
    let base = method
        .strip_prefix("assert_")
        .or_else(|| method.strip_prefix("refute_"))
        .unwrap_or(method);
    Some(match base {
        "operator" | "compare" => 3,
        "equal" | "not_equal" | "same" | "not_same" | "includes" | "not_includes" | "match"
        | "no_match" | "in_delta" | "in_epsilon" | "kind_of" | "not_kind_of" | "instance_of"
        | "not_instance_of" | "respond_to" | "not_respond_to" | "predicate" | "not_predicate" => 2,
        _ => 1,
    })
}
pub(crate) fn rspec_assertion<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Assertion> {
    let recv = child_by_field(n, "receiver")?;
    if recv.kind() != "call" || field_text(cx, recv, "method") != Some("expect") {
        return None;
    }
    let list = first_of_kind(n, &["argument_list"])?;
    let first = find_named_child(list, |c| c.kind() != "comment")?;
    let matcher = if first.kind() == "call" {
        field_text(cx, first, "method").unwrap_or("")
    } else {
        ""
    };
    let last_str = rfind_named_child(list, |c| c.kind() != "comment")
        .is_some_and(|a| a.kind() == "string" || a.kind() == "heredoc_body");
    Some(Assertion {
        pos: pos_of(n),
        has_message: count_named_child(list, |c| c.kind() != "comment") >= 2 && last_str,
        comparison: matches!(
            matcher,
            "eq" | "eql"
                | "equal"
                | "be"
                | "be_within"
                | "include"
                | "match"
                | "contain_exactly"
                | "start_with"
                | "end_with"
        ),
        shape: AssertShape::Other,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
