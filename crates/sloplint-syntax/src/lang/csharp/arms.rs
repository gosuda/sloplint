use sloplint_core::facts::{Pos, ScrutineeKind, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{
    Cx, any_named_child, child_by_field, has_child_kind, named_children, pos_of, text,
};

fn switch_subject(n: Node<'_>) -> Option<Node<'_>> {
    named_children(n).into_iter().find(|c| {
        !matches!(
            c.kind(),
            "switch_body" | "switch_section" | "switch_expression_arm"
        )
    })
}

fn scrutinee_of(subject: Node<'_>) -> ScrutineeKind {
    match subject.kind() {
        "identifier" => ScrutineeKind::Ident,
        "member_access_expression" => ScrutineeKind::Field,
        "generic_name" | "qualified_name" => ScrutineeKind::Path,
        _ => ScrutineeKind::Other,
    }
}

fn match_host<'t>(cx: &Cx<'t, '_>, parent: Node<'t>) -> Option<Node<'t>> {
    if parent.kind() == "switch_body" {
        cx.parent_of(parent)
    } else {
        Some(parent)
    }
}

fn blank_arm(pos: Pos) -> WildcardArm {
    WildcardArm {
        pos,
        enum_like: false,
        scrutinee: ScrutineeKind::Other,
        variant_count: 0,
        has_nullable_ctor: false,
    }
}

fn member_qualifiers<'s>(cx: &Cx<'_, 's>, sib: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![sib];
    while let Some(cur) = stack.pop() {
        if cur.id() != sib.id() && cur.kind() == "member_access_expression" {
            if let Some(q) = child_by_field(cur, "expression").map(|e| text(cx, e)) {
                out.push(q);
            }
            continue;
        }
        if cur.id() == sib.id() || is_pattern_kind(cur.kind()) {
            stack.extend(named_children(cur));
        }
    }
}

fn tally_qualifier<'s>(groups: &mut Vec<(&'s str, u32)>, qualifier: &'s str) {
    if let Some(g) = groups.iter_mut().find(|g| g.0 == qualifier) {
        g.1 += 1;
    } else {
        groups.push((qualifier, 1));
    }
}

fn variant_stats<'s>(cx: &Cx<'_, 's>, parent: Node<'_>, self_id: usize) -> (bool, u32) {
    let mut groups: Vec<(&'s str, u32)> = Vec::new();
    let mut quals: Vec<&'s str> = Vec::new();
    for sib in named_children(parent) {
        if sib.id() == self_id || sib.kind() == "comment" {
            continue;
        }
        quals.clear();
        member_qualifiers(cx, sib, &mut quals);
        for q in quals.drain(..) {
            tally_qualifier(&mut groups, q);
        }
    }
    let best = groups.iter().map(|g| g.1).max().unwrap_or(0);
    (best >= 2, best)
}

fn is_pattern_kind(kind: &str) -> bool {
    matches!(
        kind,
        "expression"
            | "pattern"
            | "constant_pattern"
            | "when_clause"
            | "or_pattern"
            | "and_pattern"
            | "negated_pattern"
            | "parenthesized_pattern"
            | "recursive_pattern"
    )
}

pub(crate) fn match_arm<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<WildcardArm> {
    let kind = n.kind();
    if kind != "switch_section" && kind != "switch_expression_arm" {
        return None;
    }
    let wildcard = if kind == "switch_section" {
        has_child_kind(n, "default") || any_named_child(n, |c| c.kind() == "discard")
    } else {
        any_named_child(n, |c| c.kind() == "discard")
    };
    if !wildcard {
        return None;
    }
    let pos = pos_of(n);
    let Some(parent) = cx.parent() else {
        return Some(blank_arm(pos));
    };
    let scrutinee = match_host(cx, parent)
        .and_then(switch_subject)
        .map(scrutinee_of)
        .unwrap_or(ScrutineeKind::Other);
    let (enum_like, variant_count) = variant_stats(cx, parent, n.id());
    Some(WildcardArm {
        pos,
        enum_like,
        scrutinee,
        variant_count,
        has_nullable_ctor: false,
    })
}
