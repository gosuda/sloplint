use std::collections::BTreeSet;

use crate::hooks::{Cx, child_by_field, named_children, narrow, pos_of, text};
use sloplint_core::facts::{ScrutineeKind, WildcardArm};
use tree_sitter::Node;

const NULLABLE_CTORS: [&str; 4] = ["Some", "None", "Ok", "Err"];

fn ctor_name<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> Option<&'s str> {
    match pat.kind() {
        "tuple_struct_pattern" | "struct_pattern" => {
            let ty = child_by_field(pat, "type")?;
            text(cx, ty).rsplit("::").next()
        }
        "scoped_identifier" | "scoped_type_identifier" => text(cx, pat).rsplit("::").next(),
        _ => None,
    }
}

fn is_nullable_name(name: &str) -> bool {
    NULLABLE_CTORS.contains(&name)
}

fn arm_pattern<'a>(pat: Node<'a>) -> Option<Node<'a>> {
    if pat.kind() != "match_pattern" {
        return Some(pat);
    }
    if child_by_field(pat, "condition").is_some() {
        return None;
    }
    let only = pat.named_child(0)?;
    if pat.named_child(1).is_some() {
        return None;
    }
    Some(only)
}

fn collect_names<'s>(
    cx: &Cx<'_, 's>,
    pat: Node<'_>,
    names: &mut BTreeSet<&'s str>,
    nullable: &mut bool,
) {
    let mut stack = vec![pat];
    while let Some(node) = stack.pop() {
        if node.kind() == "match_pattern" {
            let guarded = child_by_field(node, "condition").map(|cond| cond.id());
            stack.extend(
                named_children(node)
                    .into_iter()
                    .filter(|child| Some(child.id()) != guarded),
            );
            continue;
        }
        if node.kind() == "identifier" {
            let name = text(cx, node);
            if is_nullable_name(name) {
                *nullable = true;
            }
            continue;
        }
        if let Some(name) = ctor_name(cx, node) {
            if is_nullable_name(name) {
                *nullable = true;
            } else {
                names.insert(name);
            }
        }
        stack.extend(named_children(node));
    }
}

fn sibling_facts<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(BTreeSet<&'s str>, bool)> {
    let parent = cx.parent()?;
    if parent.kind() != "match_block" {
        return None;
    }
    let mut cursor = parent.walk();
    let mut names = BTreeSet::new();
    let mut nullable = false;
    for sib in parent.children(&mut cursor) {
        if sib.id() == n.id() || sib.kind() != "match_arm" {
            continue;
        }
        let Some(pat) = child_by_field(sib, "pattern") else {
            continue;
        };
        let Some(inner) = arm_pattern(pat) else {
            continue;
        };
        collect_names(cx, inner, &mut names, &mut nullable);
    }
    Some((names, nullable))
}

fn scrutinee_kind(cx: &Cx<'_, '_>, _n: Node<'_>) -> Option<ScrutineeKind> {
    let mut ancestors = cx.ancestors();
    let block = ancestors.next()?;
    if block.kind() != "match_block" {
        return None;
    }
    let expr = ancestors.next()?;
    if expr.kind() != "match_expression" {
        return None;
    }
    let value = child_by_field(expr, "value")?;
    match value.kind() {
        "identifier" => Some(ScrutineeKind::Ident),
        "field_expression" => Some(ScrutineeKind::Field),
        "scoped_identifier" => Some(ScrutineeKind::Path),
        _ => Some(ScrutineeKind::Other),
    }
}

fn bare_wildcard(cx: &Cx<'_, '_>, pat: Node<'_>) -> bool {
    let inner = if pat.kind() == "match_pattern" {
        if child_by_field(pat, "condition").is_some() {
            return false;
        }
        let only = pat.named_child(0).unwrap_or(pat);
        if pat.named_child(1).is_some() {
            return false;
        }
        only
    } else {
        pat
    };
    text(cx, inner) == "_" && inner.named_child_count() == 0
}

pub(super) fn wildcard_arm(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<WildcardArm> {
    let pat = child_by_field(n, "pattern")?;
    if !bare_wildcard(cx, pat) {
        return None;
    }
    let scrutinee = scrutinee_kind(cx, n)?;
    let (names, nullable) = sibling_facts(cx, n)?;
    let variant_count = narrow(names.len());
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 2,
        scrutinee,
        variant_count,
        has_nullable_ctor: nullable,
    })
}
