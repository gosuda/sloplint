use std::collections::{BTreeMap, BTreeSet};

use crate::hooks::{Cx, FileSetKind, child_by_field, named_children, narrow, pos_of, text};
use sloplint_core::facts::{ScrutineeKind, WildcardArm};
use tree_sitter::Node;

pub(super) const NULLABLE_CTORS: [&str; 4] = ["Some", "None", "Ok", "Err"];

fn is_scoped_constant(name: &str) -> bool {
    let mut has_letter = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            has_letter = true;
        } else if c != '_' && !c.is_ascii_digit() {
            return false;
        }
    }
    has_letter
}

fn split_prefix(full: &str) -> (&str, &str) {
    match full.rfind("::") {
        Some(at) => (&full[..at], &full[at + 2..]),
        None => ("", full),
    }
}

fn ctor_group<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> Option<(&'s str, &'s str)> {
    match pat.kind() {
        "tuple_struct_pattern" | "struct_pattern" => {
            let ty = child_by_field(pat, "type")?;
            Some(split_prefix(text(cx, ty)))
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let (prefix, name) = split_prefix(text(cx, pat));
            if is_scoped_constant(name) {
                None
            } else {
                Some((prefix, name))
            }
        }
        _ => None,
    }
}

fn is_nullable_name(name: &str) -> bool {
    NULLABLE_CTORS.contains(&name)
}

fn is_nullable_ctor(prefix: &str, name: &str) -> bool {
    if !is_nullable_name(name) {
        return false;
    }
    prefix
        .rsplit("::")
        .next()
        .is_some_and(|scope| scope.is_empty() || scope == "Option" || scope == "Result")
}

fn file_variant_names<'c, 's>(cx: &'c Cx<'_, 's>) -> &'c [&'s str] {
    cx.file_names(FileSetKind::Member, || {
        let mut names = Vec::new();
        let mut stack = Vec::new();
        if let Some(root) = cx.root() {
            stack.push(root);
        }
        while let Some(node) = stack.pop() {
            if node.kind() == "enum_variant"
                && let Some(name) = child_by_field(node, "name")
            {
                names.push(text(cx, name));
            }
            stack.extend(named_children(node));
        }
        names
    })
}

fn record_ctor<'s>(
    groups: &mut BTreeMap<&'s str, BTreeSet<&'s str>>,
    nullable: &mut bool,
    prefix: &'s str,
    name: &'s str,
) {
    if is_nullable_ctor(prefix, name) {
        *nullable = true;
    } else {
        groups.entry(prefix).or_default().insert(name);
    }
}

fn arm_pattern<'a>(pat: Node<'a>) -> Option<Node<'a>> {
    if pat.kind() != "match_pattern" {
        return Some(pat);
    }
    let guarded = child_by_field(pat, "condition").map(|cond| cond.id());
    let mut parts = named_children(pat).into_iter().filter(|child| {
        Some(child.id()) != guarded
            && child.kind() != "line_comment"
            && child.kind() != "block_comment"
    });
    let only = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(only)
}

fn starts_uppercase(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_uppercase())
}

fn insert_bare_name<'s>(
    cx: &Cx<'_, 's>,
    groups: &mut BTreeMap<&'s str, BTreeSet<&'s str>>,
    nullable: &mut bool,
    name: &'s str,
) {
    if is_nullable_name(name) {
        if file_variant_names(cx).contains(&name) {
            groups.entry("").or_default().insert(name);
        } else {
            *nullable = true;
        }
    } else if !is_scoped_constant(name) && starts_uppercase(name) {
        groups.entry("").or_default().insert(name);
    }
}

fn collect_names<'s>(
    cx: &Cx<'_, 's>,
    pat: Node<'_>,
    groups: &mut BTreeMap<&'s str, BTreeSet<&'s str>>,
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
            insert_bare_name(cx, groups, nullable, text(cx, node));
            continue;
        }
        if node.kind() == "scoped_identifier" || node.kind() == "scoped_type_identifier" {
            if let Some((prefix, name)) = ctor_group(cx, node) {
                record_ctor(groups, nullable, prefix, name);
            }
            continue;
        }
        if node.kind() == "tuple_struct_pattern" || node.kind() == "struct_pattern" {
            if let Some((prefix, name)) = ctor_group(cx, node) {
                record_ctor(groups, nullable, prefix, name);
            }
            let skip = child_by_field(node, "type").map(|ty| ty.id());
            stack.extend(
                named_children(node)
                    .into_iter()
                    .filter(|child| Some(child.id()) != skip),
            );
            continue;
        }
        stack.extend(named_children(node));
    }
}

fn sibling_facts<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(u32, bool)> {
    let parent = cx.parent()?;
    if parent.kind() != "match_block" {
        return None;
    }
    let mut cursor = parent.walk();
    let mut groups: BTreeMap<&'s str, BTreeSet<&'s str>> = BTreeMap::new();
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
        collect_names(cx, inner, &mut groups, &mut nullable);
    }
    let widest = groups.values().map(BTreeSet::len).max().unwrap_or(0);
    Some((narrow(widest), nullable))
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
    let (variant_count, nullable) = sibling_facts(cx, n)?;
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 2,
        scrutinee,
        variant_count,
        has_nullable_ctor: nullable,
    })
}
