use sloplint_core::facts::{
    AmbientKind, CallKind, FieldCopy, Handler, ImplRef, Pos, ScrutineeKind, Stmt, TypeDef,
    Visibility, WildcardArm,
};
use tree_sitter::Node;

use super::calls::{call_identity, func_node};
use super::funcs::{in_test_dir, seg_last};
use crate::hooks::{
    Container, Cx, FileSetKind, TypeHead, any_named_child, child_by_field, first_named_child,
    has_child_kind, named_children, narrow, pos_of, text, trimmed,
};

pub(crate) fn first_ident<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            return Some((text(cx, cur), pos_of(cur)));
        }
        if cur.kind() != "wildcard" {
            stack.extend(named_children(cur).into_iter().rev());
        }
    }
    None
}

pub(crate) fn collect_idents<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending = vec![n];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "wildcard" => {}
            "identifier" => out.push((text(cx, node), pos_of(node))),
            _ => pending.extend(named_children(node).into_iter().rev()),
        }
    }
}

fn binding_of<'s>(cx: &Cx<'_, 's>, pat: Option<Node<'_>>) -> Option<&'s str> {
    let mut current = pat?;
    loop {
        if current.kind() == "identifier" {
            return Some(text(cx, current));
        }
        if current.kind() == "capture_pattern" {
            return child_by_field(current, "name").map(|m| text(cx, m));
        }
        if current.kind() != "typed_pattern" {
            return None;
        }
        let inner = child_by_field(current, "pattern")?;
        if inner.kind() == "identifier" {
            return Some(text(cx, inner));
        }
        current = inner;
    }
}

fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "throw_expression" {
        let hit = any_named_child(s, |c| {
            c.kind() == "identifier" && Some(text(cx, c)) == binding
        });
        return if hit { Stmt::Rethrow } else { Stmt::Other };
    }
    if s.kind() != "call_expression" {
        return Stmt::Other;
    }
    match call_identity(cx, s).map(|(callee, _, _)| callee) {
        Some(callee) => Stmt::Call {
            callee,
            kind: CallKind::Plain,
        },
        None => Stmt::Other,
    }
}

fn body_stmts<'s>(cx: &Cx<'_, 's>, body: Node<'_>, binding: Option<&'s str>) -> Vec<Stmt<'s>> {
    if body.kind() == "indented_block" {
        named_children(body)
            .into_iter()
            .filter(|c| c.kind() != "comment" && c.kind() != "block_comment")
            .map(|s| stmt_of(cx, s, binding))
            .collect()
    } else {
        vec![stmt_of(cx, body, binding)]
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "field_expression" => child_by_field(current, "value")?,
            "parenthesized_expression" => first_named_child(current)?,
            _ => return None,
        };
    }
}

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "var_definition" && n.kind() != "val_definition" {
        return;
    }
    if let Some(pat) = child_by_field(n, "pattern") {
        collect_idents(cx, pat, out);
    }
}

fn is_compound_write_op(op: &str) -> bool {
    op.ends_with('=')
        && !matches!(
            op,
            "==" | "!=" | "<=" | ">=" | "=/=" | "=!=" | "===" | "!==" | "=:="
        )
}

fn is_compound_write(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "infix_expression"
        && child_by_field(n, "operator").is_some_and(|o| is_compound_write_op(text(cx, o)))
}

pub(crate) fn shadow_decl_kind(kind: &str) -> bool {
    matches!(kind, "for_expression" | "case_clause")
}

fn push_pattern_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<&'s str>) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind() == "identifier" {
            if is_pattern_binder(cx, node) {
                out.push(text(cx, node));
            }
        } else if node.kind() != "wildcard" {
            pending.extend(named_children(node).into_iter().rev());
        }
    }
}

fn is_pattern_binder(cx: &Cx<'_, '_>, node: Node<'_>) -> bool {
    let name = text(cx, node);
    if name.starts_with('`')
        || !name
            .chars()
            .next()
            .is_some_and(|first| first.is_lowercase())
    {
        return false;
    }
    let Some(parent) = cx.parent_of(node) else {
        return true;
    };
    !matches!(
        parent.kind(),
        "type_identifier" | "stable_identifier" | "stable_type_identifier"
    )
}

fn push_generator_pattern<'s>(cx: &Cx<'_, 's>, enumerator: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(head) = first_named_child(enumerator) else {
        return;
    };
    if head.kind() == "guard" {
        return;
    }
    push_pattern_names(cx, head, out);
}

fn push_for_binders<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    for holder in named_children(n) {
        if holder.kind() != "enumerators" {
            continue;
        }
        for enumerator in named_children(holder) {
            if enumerator.kind() != "enumerator" {
                continue;
            }
            push_generator_pattern(cx, enumerator, out);
        }
    }
}

pub(crate) fn shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "for_expression" => push_for_binders(cx, n, out),
        "case_clause" => {
            if let Some(pat) = child_by_field(n, "pattern") {
                push_pattern_names(cx, pat, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn write_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "assignment_expression"
        && cx
            .parent()
            .is_some_and(|parent| parent.kind() == "arguments")
    {
        return;
    }
    if (n.kind() == "assignment_expression" || is_compound_write(cx, n))
        && let Some(l) = child_by_field(n, "left")
    {
        out.extend(write_root(cx, l));
    }
}

fn extend_info<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> (Vec<ImplRef<'s>>, bool) {
    let mut impls = Vec::new();
    let mut testy = in_test_dir(cx.path);
    for t in named_children(c) {
        if t.kind() == "arguments" {
            continue;
        }
        let ttext = trimmed(cx, t);
        testy |= super::funcs::is_suite_base(seg_last(ttext));
        impls.push(ImplRef {
            name: seg_last(ttext),
            pos: pos_of(t),
            is_generic: ttext.contains('['),
            child_is_abstract: false,
        });
    }
    (impls, testy)
}

fn type_vis<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Visibility {
    let restricted = any_named_child(n, |c| {
        c.kind() == "access_modifier"
            || (c.kind() == "modifiers" && has_child_kind(c, "access_modifier"))
    });
    if !restricted {
        return Visibility::Exported;
    }
    let scoped = any_named_child(n, |c| {
        (c.kind() == "access_modifier" && text(cx, c).contains('['))
            || (c.kind() == "modifiers"
                && named_children(c)
                    .iter()
                    .any(|m| m.kind() == "access_modifier" && text(cx, *m).contains('[')))
    });
    if scoped {
        Visibility::Restricted
    } else {
        Visibility::Private
    }
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let kind = n.kind();
    let is_type = kind == "class_definition"
        || kind == "object_definition"
        || kind == "trait_definition"
        || kind == "enum_definition";
    if !is_type {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    let mut vis = type_vis(cx, n);
    match cx.innermost_container() {
        None => {}
        Some(Container::Class { exported, .. }) if !exported => vis = Visibility::Private,
        Some(Container::Trait { public }) if !public => vis = Visibility::Private,
        _ => {}
    }
    let abstraction = kind == "trait_definition"
        || any_named_child(n, |c| {
            c.kind() == "modifiers" && has_child_kind(c, "abstract")
        });
    let mut impls = Vec::new();
    let mut testy = false;
    for c in named_children(n) {
        if c.kind() == "extends_clause" {
            let (more, suite) = extend_info(cx, c);
            impls.extend(more);
            testy |= suite;
        }
    }
    let container = if testy {
        Some(Container::TestModule)
    } else if kind == "trait_definition" {
        Some(Container::Trait {
            public: vis != Visibility::Private,
        })
    } else {
        Some(Container::Class {
            exported: vis == Visibility::Exported,
            implements: !impls.is_empty(),
        })
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility: vis,
            abstraction,
        }),
        impls,
        container,
    })
}

fn match_scrutinee(cx: &Cx<'_, '_>, _n: Node<'_>) -> ScrutineeKind {
    if cx.parent().is_none() {
        return ScrutineeKind::Other;
    }
    let Some(m) = cx.ancestors().nth(1) else {
        return ScrutineeKind::Other;
    };
    if m.kind() != "match_expression" {
        return ScrutineeKind::Other;
    }
    match child_by_field(m, "value").map(|v| v.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}

fn bump_prefix<'s>(groups: &mut Vec<(&'s str, u32)>, prefix: &'s str) {
    if let Some(entry) = groups.iter_mut().find(|(held, _)| *held == prefix) {
        entry.1 += 1;
    } else {
        groups.push((prefix, 1));
    }
}

pub(crate) fn match_arm<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "case_clause" {
        return None;
    }
    let pos = pos_of(n);
    if child_by_field(n, "pattern").is_none_or(|p| p.kind() != "wildcard") {
        return None;
    }
    let Some(parent) = cx.parent() else {
        return Some(WildcardArm {
            pos,
            enum_like: false,
            scrutinee: ScrutineeKind::Other,
            variant_count: 0,
            has_nullable_ctor: false,
        });
    };
    let mut variants = 0u32;
    let mut case_class_count = 0u32;
    let mut stable_groups: Vec<(&'s str, u32)> = Vec::new();
    let mut nullable = false;
    for s in named_children(parent) {
        if s.id() == n.id() || s.kind() != "case_clause" {
            continue;
        }
        let Some(p) = child_by_field(s, "pattern") else {
            continue;
        };
        if p.kind() == "stable_identifier" {
            variants += 1;
            let prefix = trimmed(cx, p)
                .rsplit_once('.')
                .map(|(head, _)| head)
                .unwrap_or("");
            bump_prefix(&mut stable_groups, prefix);
        } else if p.kind() == "case_class_pattern" {
            variants += 1;
            case_class_count += 1;
        }
        if matches!(seg_last(trimmed(cx, p)), "Some" | "None" | "Ok" | "Err") {
            nullable = true;
        }
    }
    let stable_best = stable_groups
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0);
    Some(WildcardArm {
        pos,
        enum_like: case_class_count >= 2 || stable_best >= 2,
        scrutinee: match_scrutinee(cx, n),
        variant_count: variants,
        has_nullable_ctor: nullable,
    })
}

pub(crate) fn handler_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() == "catch_clause" {
        let binding = binding_of(cx, child_by_field(n, "pattern"));
        let body = child_by_field(n, "body")?;
        return Some(Handler {
            pos: pos_of(n),
            stmts: body_stmts(cx, body, binding),
            has_else: false,
            try_imports_only: false,
            in_cleanup: false,
        });
    }
    if n.kind() != "case_clause" {
        return None;
    }
    let parent = cx.parent()?;
    if parent.kind() != "case_block" && parent.kind() != "indented_cases" {
        return None;
    }
    if cx
        .ancestors()
        .nth(1)
        .is_none_or(|g| g.kind() != "catch_clause")
    {
        return None;
    }
    let binding = binding_of(cx, child_by_field(n, "pattern"));
    let pat_id = child_by_field(n, "pattern").map(|p| p.id());
    let mut stmts = Vec::new();
    for c in named_children(n) {
        if Some(c.id()) != pat_id && c.kind() != "guard" {
            stmts.push(stmt_of(cx, c, binding));
        }
    }
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn struct_field<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> Option<&'s str> {
    if a.kind() != "assignment_expression" {
        return None;
    }
    let (Some(l), Some(r)) = (child_by_field(a, "left"), child_by_field(a, "right")) else {
        return None;
    };
    if l.kind() != "identifier" || r.kind() != "field_expression" {
        return None;
    }
    let (Some(v), Some(f)) = (child_by_field(r, "value"), child_by_field(r, "field")) else {
        return None;
    };
    if v.kind() != "identifier" || text(cx, f) != text(cx, l) {
        return None;
    }
    Some(text(cx, v))
}

pub(crate) fn struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func_node(func).unwrap_or(func).kind() != "identifier" {
        return None;
    }
    let kids = super::calls::call_arg_nodes(n);
    if kids.is_empty() {
        return None;
    }
    let mut first: Option<&'s str> = None;
    let mut uniform = true;
    for a in &kids {
        let s = struct_field(cx, *a)?;
        match first {
            None => first = Some(s),
            Some(prev) if prev == s => {}
            Some(_) => uniform = false,
        }
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(kids.len()),
        source: if uniform { first } else { None },
    })
}

fn var_pattern_name<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> Option<&'s str> {
    if c.kind() != "var_definition" {
        return None;
    }
    let pat = child_by_field(c, "pattern")?;
    first_ident(cx, pat).map(|(name, _)| name)
}

fn top_level_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    named_children(root)
        .iter()
        .filter_map(|c| var_pattern_name(cx, *c))
        .collect()
}

fn top_level_var<'s>(cx: &Cx<'_, 's>, _n: Node<'_>, name: &str) -> bool {
    cx.file_names(FileSetKind::Scope, || top_level_names(cx))
        .contains(&name)
}

fn report_assign_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    let Some((name, pos)) = write_root(cx, left) else {
        return;
    };
    if top_level_var(cx, n, name) {
        out.push((AmbientKind::GlobalWrite, name, pos));
    }
}

pub(crate) fn ambient_head<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "var_definition" && !in_function {
        if let Some(pat) = child_by_field(n, "pattern") {
            out.extend(
                first_ident(cx, pat).map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
            );
        }
        return;
    }
    if in_function && (n.kind() == "assignment_expression" || is_compound_write(cx, n)) {
        report_assign_write(cx, n, out);
        return;
    }
    if n.kind() == "field_expression" {
        let (Some(v), Some(f)) = (child_by_field(n, "value"), child_by_field(n, "field")) else {
            return;
        };
        if v.kind() == "identifier" && text(cx, v) == "sys" && text(cx, f) == "env" {
            out.push((AmbientKind::EnvRead, "env", pos_of(n)));
        }
        return;
    }
    if n.kind() != "call_expression" {
        return;
    }
    let Some(f) = child_by_field(n, "function") else {
        return;
    };
    if f.kind() != "field_expression" {
        return;
    }
    let (Some(v), Some(m)) = (child_by_field(f, "value"), child_by_field(f, "field")) else {
        return;
    };
    if text(cx, v) == "System" && text(cx, m) == "getenv" {
        let label = super::calls::call_first_arg(n)
            .map(|a| text(cx, a))
            .unwrap_or_else(|| text(cx, f));
        out.push((AmbientKind::EnvRead, label, pos_of(n)));
    }
}
