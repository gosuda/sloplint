use crate::hooks::{
    Container, Cx, TypeHead, any_named_child, child_by_field, find_map_named_child,
    first_named_child, has_child_kind, named_children, narrow, pos_of, text, trimmed,
};
use sloplint_core::facts::{
    AmbientKind, CallKind, FieldCopy, Handler, ImplRef, Pos, ScrutineeKind, Stmt, TypeDef,
    WildcardArm,
};
use tree_sitter::Node;

use super::funcs::visibility_of;
use super::syntax::{cut_generics, has_anon, label_text, last_ident};

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "local_variable_declaration" {
        return;
    }
    for c in named_children(n) {
        if c.kind() == "initialized_variable_definition"
            && !has_anon(c, "final")
            && !has_anon(c, "const")
            && let Some(nm) = child_by_field(c, "name")
        {
            out.push((text(cx, nm), pos_of(nm)));
        }
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "super" => return Some((text(cx, current), pos_of(current))),
            "member_expression" | "index_expression" => child_by_field(current, "object")?,
            "assignable_expression" => match child_by_field(current, "object") {
                Some(object) => object,
                None => first_named_child(current)?,
            },
            "parenthesized_expression" => first_named_child(current)?,
            _ => return None,
        };
    }
}

pub(crate) fn write_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "assignment_expression" {
        if let Some(l) = child_by_field(n, "left") {
            out.extend(write_root(cx, l));
        }
        return;
    }
    if n.kind() == "postfix_expression" {
        if let Some(a) = child_by_field(n, "argument") {
            out.extend(write_root(cx, a));
        }
        return;
    }
    if n.kind() != "unary_expression" {
        return;
    }
    let first_anon = (0..n.child_count())
        .filter_map(|i| n.child(i))
        .find(|c| !c.is_named());
    if !first_anon.is_some_and(|c| c.kind() == "++" || c.kind() == "--") {
        return;
    }
    for c in named_children(n) {
        if c.kind() == "assignable_expression" {
            out.extend(write_root(cx, c));
        }
    }
}

fn top_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    find_map_named_child(n, |c| {
        if c.kind() != "initialized_identifier_list" && c.kind() != "static_final_declaration_list"
        {
            return None;
        }
        find_map_named_child(c, |e| {
            if e.kind() != "initialized_identifier" && e.kind() != "static_final_declaration" {
                return None;
            }
            child_by_field(e, "name")
        })
    })
    .map(|nm| (text(cx, nm), pos_of(nm)))
}

fn push_impl<'s>(cx: &Cx<'_, 's>, t: Node<'_>, impls: &mut Vec<ImplRef<'s>>) {
    impls.push(ImplRef {
        name: last_ident(text(cx, t)),
        pos: pos_of(t),
        is_generic: false,
        child_is_abstract: false,
    });
}

fn iface_impls<'s>(cx: &Cx<'_, 's>, c: Node<'_>, impls: &mut Vec<ImplRef<'s>>) {
    for t in named_children(c) {
        if t.kind() == "type" {
            push_impl(cx, t, impls);
        }
    }
}

fn super_impls<'s>(cx: &Cx<'_, 's>, c: Node<'_>, impls: &mut Vec<ImplRef<'s>>) {
    for t in named_children(c) {
        if t.kind() == "type" {
            push_impl(cx, t, impls);
        } else if t.kind() == "mixins" {
            iface_impls(cx, t, impls);
        }
    }
}

pub(crate) fn typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let kind = n.kind();
    let is_type = kind == "class_declaration"
        || kind == "mixin_declaration"
        || kind == "enum_declaration"
        || kind == "extension_type_declaration";
    if !is_type {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    let abstraction = kind == "class_declaration"
        && (has_child_kind(n, "abstract") || has_child_kind(n, "interface"));
    let mut impls = Vec::new();
    for c in named_children(n) {
        if c.kind() == "interfaces" {
            iface_impls(cx, c, &mut impls);
        } else if c.kind() == "superclass" {
            super_impls(cx, c, &mut impls);
        }
    }
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility: visibility_of(cx, name),
            abstraction,
        }),
        impls,
        container: Some(Container::Class {
            exported: !name.starts_with('_'),
            implements: has_child_kind(n, "interfaces"),
        }),
    })
}

fn arm_pattern_member(n: Node<'_>) -> bool {
    fn memberish(m: Node<'_>) -> bool {
        m.kind() == "member_expression"
            || (m.kind() == "constant_pattern"
                && m.named_child_count() == 2
                && (0..m.child_count())
                    .filter_map(|i| m.child(i))
                    .any(|c| c.kind() == "."))
    }
    any_named_child(n, |c| {
        memberish(c)
            || (!c.kind().contains("statement")
                && c.kind() != "block"
                && any_named_child(c, memberish))
    })
}

fn sibling_member(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    any_named_child(parent, |s| s.id() != n.id() && arm_pattern_member(s))
}

fn switch_scrutinee(cx: &Cx<'_, '_>, _n: Node<'_>) -> Option<ScrutineeKind> {
    for current in cx.ancestors() {
        let kind = current.kind();
        if kind == "switch_expression" || kind == "switch_statement" {
            let cond = child_by_field(current, "condition")?;
            let inner = if cond.kind() == "parenthesized_expression" {
                first_named_child(cond)?
            } else {
                cond
            };
            return Some(match inner.kind() {
                "identifier" => ScrutineeKind::Ident,
                "member_expression" => ScrutineeKind::Field,
                _ => ScrutineeKind::Other,
            });
        }
    }
    None
}

fn sibling_variants(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let Some(parent) = cx.parent() else {
        return 0;
    };
    narrow(
        named_children(parent)
            .into_iter()
            .filter(|s| s.id() != n.id() && arm_pattern_member(*s))
            .count(),
    )
}

pub(crate) fn match_arm(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    let kind = n.kind();
    let is_arm = kind == "switch_statement_case"
        || kind == "switch_statement_default"
        || kind == "switch_expression_case";
    if !is_arm {
        return None;
    }
    let pos = pos_of(n);
    let scrutinee = switch_scrutinee(cx, n).unwrap_or(ScrutineeKind::Other);
    let variant_count = sibling_variants(cx, n);
    if kind == "switch_statement_default" {
        return Some(WildcardArm {
            pos,
            enum_like: sibling_member(cx, n),
            scrutinee,
            variant_count,
            has_nullable_ctor: false,
        });
    }
    let first = first_named_child(n)?;
    if trimmed(cx, first) != "_" {
        return None;
    }
    Some(WildcardArm {
        pos,
        enum_like: sibling_member(cx, n),
        scrutinee,
        variant_count,
        has_nullable_ctor: false,
    })
}

fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "rethrow_statement" {
        return Stmt::Rethrow;
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    find_map_named_child(s, |c| stmt_call(cx, c, binding)).unwrap_or(Stmt::Other)
}

fn stmt_call<'s>(cx: &Cx<'_, 's>, c: Node<'_>, binding: Option<&'s str>) -> Option<Stmt<'s>> {
    if c.kind() == "call_expression" {
        return child_by_field(c, "function").map(|f| Stmt::Call {
            callee: cut_generics(text(cx, f)),
            kind: CallKind::Plain,
        });
    }
    if c.kind() == "throw_expression" {
        let hit = any_named_child(c, |v| {
            v.kind() == "identifier" && Some(text(cx, v)) == binding
        });
        if hit {
            return Some(Stmt::Rethrow);
        }
    }
    None
}

pub(crate) fn handler<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let binding = child_by_field(n, "exception").map(|e| text(cx, e));
    let mut sib = n.next_sibling();
    let body = loop {
        match sib {
            None => return None,
            Some(s) if s.is_named() => break s,
            Some(s) => sib = s.next_sibling(),
        }
    };
    if body.kind() != "block" {
        return None;
    }
    let mut stmts = Vec::new();
    for s in named_children(body) {
        stmts.push(stmt_of(cx, s, binding));
    }
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn struct_arg<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> Option<&'s str> {
    if a.kind() != "named_argument" {
        return None;
    }
    if a.named_child_count() != 2 {
        return None;
    }
    let (Some(first), Some(val)) = (a.named_child(0), a.named_child(1)) else {
        return None;
    };
    if first.kind() != "label" {
        return None;
    }
    let label = label_text(cx, first);
    if val.kind() != "member_expression" {
        return None;
    }
    let (Some(o), Some(p)) = (
        child_by_field(val, "object"),
        child_by_field(val, "property"),
    ) else {
        return None;
    };
    if o.kind() != "identifier" || p.kind() != "identifier" || text(cx, p) != label {
        return None;
    }
    Some(text(cx, o))
}

pub(crate) fn struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let args_node = child_by_field(n, "arguments")?;
    let kids = named_children(args_node);
    if kids.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    for a in &kids {
        let s = struct_arg(cx, *a)?;
        match source {
            None => source = Some(s),
            Some(prev) if prev == s => {}
            Some(_) => return None,
        }
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(kids.len()),
        source,
    })
}

fn list_declares(cx: &Cx<'_, '_>, list: Node<'_>, name: &str) -> bool {
    let is_list = list.kind() == "initialized_identifier_list"
        || list.kind() == "static_final_declaration_list";
    if !is_list {
        return false;
    }
    for e in named_children(list) {
        if child_by_field(e, "name").is_some_and(|nm| text(cx, nm) == name) {
            return true;
        }
    }
    false
}

fn decl_declares(cx: &Cx<'_, '_>, c: Node<'_>, name: &str) -> bool {
    if c.kind() != "top_level_variable_declaration" {
        return false;
    }
    for list in named_children(c) {
        if list_declares(cx, list, name) {
            return true;
        }
    }
    false
}

fn top_level_declares(cx: &Cx<'_, '_>, _n: Node<'_>, name: &str) -> bool {
    let Some(root) = cx.root() else {
        return false;
    };
    for c in named_children(root) {
        if decl_declares(cx, c, name) {
            return true;
        }
    }
    false
}

fn report_global_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let mut roots = Vec::new();
    write_roots(cx, n, &mut roots);
    for (name, pos) in roots {
        if name != "_" && top_level_declares(cx, n, name) {
            out.push((AmbientKind::GlobalWrite, name, pos));
        }
    }
}

pub(crate) fn ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "top_level_variable_declaration" {
        if has_anon(n, "final") || has_anon(n, "const") {
            return;
        }
        if child_by_field(n, "modifier").is_some_and(|m| {
            let t = text(cx, m);
            t == "final" || t == "const"
        }) {
            return;
        }
        if let Some((name, pos)) = top_name(cx, n) {
            out.push((AmbientKind::GlobalMutable, name, pos));
        }
        return;
    }
    if n.kind() == "member_expression" {
        let (Some(o), Some(p)) = (child_by_field(n, "object"), child_by_field(n, "property"))
        else {
            return;
        };
        if o.kind() == "identifier"
            && text(cx, o) == "Platform"
            && p.kind() == "identifier"
            && text(cx, p) == "environment"
        {
            out.push((AmbientKind::EnvRead, "environment", pos_of(n)));
        }
        return;
    }
    if in_function
        && matches!(
            n.kind(),
            "assignment_expression" | "postfix_expression" | "unary_expression"
        )
    {
        report_global_write(cx, n, out);
    }
}
