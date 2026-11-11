use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, CallKind, FieldCopy, Handler, ImplRef, Import, ImportKind, ImportScope, Pos,
    ScrutineeKind, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, TypeHead, any_named_child, child_by_field, find_named_child,
    first_named_child_matching, first_of_kind, has_child_kind, last_named_child, last_segment,
    named_children, narrow, pos_of, slice, text, trimmed,
};

use super::funcs::visibility_of;
use super::syntax::COMMENT_KINDS;

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "value_binding_pattern" {
        super::binds::decl_names_for(cx, n, out);
        return;
    }
    if is_var(cx, n) {
        out.extend(bound_names(cx, n));
    }
}

fn is_var(cx: &Cx<'_, '_>, decl: Node<'_>) -> bool {
    first_named_child_matching(decl, &["value_binding_pattern"])
        .and_then(|pattern| child_by_field(pattern, "mutability"))
        .is_some_and(|mutability| trimmed(cx, mutability) == "var")
}

fn bound_names<'s>(cx: &Cx<'_, 's>, decl: Node<'_>) -> Vec<(&'s str, Pos)> {
    (0..decl.child_count())
        .filter(|&index| decl.field_name_for_child(index) == Some("name"))
        .filter_map(|index| decl.child(index))
        .flat_map(named_children)
        .filter(|c| c.kind() == "simple_identifier")
        .map(|c| (text(cx, c), pos_of(c)))
        .collect()
}

pub(crate) fn write_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "assignment" {
        out.extend(child_by_field(n, "target").and_then(|t| root_of(cx, t)));
    }
}

fn root_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "simple_identifier" => return Some((text(cx, current), pos_of(current))),
            "self_expression" => return Some(("self", pos_of(current))),
            "navigation_expression" | "postfix_expression" => child_by_field(current, "target")?,
            "directly_assignable_expression" => current.named_child(0)?,
            "call_expression" if super::calls::is_subscript(current) => current.named_child(0)?,
            _ => return None,
        };
    }
}

pub(crate) fn imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let target = first_named_child_matching(n, &["identifier"]);
    let (Some(target), "import_declaration") = (target, n.kind()) else {
        return;
    };
    let module = trimmed(cx, target);
    let named = first_of_kind(n, &["import"]).is_some_and(|keyword| {
        !slice(cx.src, keyword.end_byte(), target.start_byte())
            .trim()
            .is_empty()
    });
    let is_reexport = any_named_child(n, |c| {
        c.kind() == "modifiers"
            && any_named_child(c, |a| {
                a.kind() == "attribute"
                    && any_named_child(a, |u| {
                        u.kind() == "user_type" && trimmed(cx, u) == "_exported"
                    })
            })
    });
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name: last_segment(module, &["."]),
        alias: None,
        kind: if named {
            ImportKind::Named
        } else {
            ImportKind::Namespace
        },
        scope: ImportScope::Module,
        in_test: false,
        is_reexport,
        external_root: Some(module.split('.').next().unwrap_or(module)),
    })
}

pub(crate) fn typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let name_node = child_by_field(n, "name")?;
    let base = TypeDef {
        name: trimmed(cx, name_node),
        pos: pos_of(name_node),
        visibility: visibility_of(cx, n),
        abstraction: false,
    };
    let extension =
        child_by_field(n, "declaration_kind").is_some_and(|kind| trimmed(cx, kind) == "extension");
    let (def, impls) = match n.kind() {
        "class_declaration" if !extension => (base, inheritance_refs(cx, n)),
        "protocol_declaration" => (
            TypeDef {
                name: trimmed(cx, name_node),
                pos: pos_of(name_node),
                visibility: visibility_of(cx, n),
                abstraction: true,
            },
            Vec::new(),
        ),
        "typealias_declaration" => (base, Vec::new()),
        _ => return None,
    };
    Some(TypeHead {
        def: Some(def),
        impls,
        container: None,
    })
}

fn inheritance_refs<'s>(cx: &Cx<'_, 's>, class: Node<'_>) -> Vec<ImplRef<'s>> {
    named_children(class)
        .into_iter()
        .filter(|c| c.kind() == "inheritance_specifier")
        .filter_map(|specifier| {
            let user = first_named_child_matching(specifier, &["user_type"])?;
            let base = trimmed(cx, user);
            let cut = base.find('<').unwrap_or(base.len());
            Some(ImplRef {
                name: last_segment(&base[..cut], &["."]),
                pos: pos_of(specifier),
                is_generic: false,
                child_is_abstract: false,
            })
        })
        .collect()
}

fn pattern_of(entry: Node<'_>) -> Option<Node<'_>> {
    find_named_child(entry, |c| matches!(c.kind(), "switch_pattern" | "pattern"))
}

fn is_wildcard_entry(cx: &Cx<'_, '_>, entry: Node<'_>) -> bool {
    has_child_kind(entry, "default_keyword")
        || (!has_child_kind(entry, "where_keyword")
            && pattern_of(entry).is_some_and(|pattern| trimmed(cx, pattern) == "_"))
}
fn enum_pattern(pattern: &str) -> bool {
    pattern.starts_with('.')
        || pattern.split_once('.').is_some_and(|(head, tail)| {
            head.starts_with(|c: char| c.is_ascii_uppercase())
                && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && tail.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        })
}

fn is_nullable_pattern(pattern: &str) -> bool {
    if pattern.starts_with("Optional.") || pattern.starts_with("Result.") {
        return true;
    }
    let case = pattern.strip_prefix('.').unwrap_or(pattern);
    let ident = case.split(['(', ' ', ':']).next().unwrap_or(case);
    matches!(ident, "some" | "none" | "ok" | "err")
}
fn enum_root(pattern: &str) -> &str {
    let text = pattern.trim();
    if text.starts_with('.') {
        "."
    } else {
        text.split(['.', '(']).next().unwrap_or(text)
    }
}
fn switch_scrutinee(cx: &Cx<'_, '_>, _n: Node<'_>) -> ScrutineeKind {
    cx.ancestors()
        .find_map(|node| {
            (node.kind() == "switch_statement").then(|| {
                match child_by_field(node, "expr").map(|s| s.kind()) {
                    Some("simple_identifier") => ScrutineeKind::Ident,
                    Some("navigation_expression") => ScrutineeKind::Field,
                    _ => ScrutineeKind::Other,
                }
            })
        })
        .unwrap_or(ScrutineeKind::Other)
}

fn sibling_info(cx: &Cx<'_, '_>, n: Node<'_>) -> (u32, bool, bool) {
    let Some(parent) = cx.parent() else {
        return (0, false, false);
    };
    let mut count = 0;
    let mut nullable = false;
    let mut root: Option<&str> = None;
    let mut same = true;
    for sibling in named_children(parent) {
        if sibling.id() == n.id() || sibling.kind() != "switch_entry" {
            continue;
        }
        for child in named_children(sibling) {
            if !matches!(child.kind(), "switch_pattern" | "pattern") {
                continue;
            }
            let text = trimmed(cx, child);
            if !enum_pattern(text) {
                continue;
            }
            count += 1;
            nullable = nullable || is_nullable_pattern(text);
            let next = enum_root(text);
            match root {
                None => root = Some(next),
                Some(current) if current != next => same = false,
                _ => {}
            }
        }
    }
    (narrow(count), nullable, count >= 2 && same)
}

pub(crate) fn match_arm(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    (n.kind() == "switch_entry" && is_wildcard_entry(cx, n)).then(|| {
        let (variant_count, has_nullable_ctor, enum_like) = sibling_info(cx, n);
        WildcardArm {
            pos: pos_of(n),
            enum_like,
            scrutinee: switch_scrutinee(cx, n),
            variant_count,
            has_nullable_ctor,
        }
    })
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, statement: Node<'_>, binding: &str) -> Stmt<'s> {
    match statement.kind() {
        "call_expression" if !super::calls::is_subscript(statement) => statement
            .named_child(0)
            .map_or(Stmt::Other, |callee| Stmt::Call {
                callee: trimmed(cx, callee),
                kind: CallKind::Plain,
            }),
        "control_transfer_statement"
            if has_child_kind(statement, "throw_keyword")
                && last_named_child(statement).is_some_and(|last| {
                    last.kind() == "simple_identifier" && trimmed(cx, last) == binding
                }) =>
        {
            Stmt::Rethrow
        }
        _ => Stmt::Other,
    }
}

pub(crate) fn handler<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_block" {
        return None;
    }
    let binding = match child_by_field(n, "error") {
        Some(pattern) => child_by_field(pattern, "bound_identifier")
            .map_or_else(|| trimmed(cx, pattern), |id| trimmed(cx, id)),
        None => "error",
    };
    let stmts = first_named_child_matching(n, &["statements"]).map_or_else(Vec::new, |body| {
        named_children(body)
            .into_iter()
            .filter(|c| !COMMENT_KINDS.contains(&c.kind()))
            .map(|s| handler_stmt(cx, s, binding))
            .collect()
    });
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn copy_source<'s>(cx: &Cx<'_, 's>, arguments: &[Node<'_>]) -> Option<&'s str> {
    let mut source: Option<&'s str> = None;
    for argument in arguments {
        let label = trimmed(cx, child_by_field(*argument, "name")?).trim_end_matches(':');
        let value =
            child_by_field(*argument, "value").filter(|v| v.kind() == "navigation_expression")?;
        let target = child_by_field(value, "target").filter(|t| t.kind() == "simple_identifier")?;
        let member = child_by_field(child_by_field(value, "suffix")?, "suffix")?;
        if trimmed(cx, member) != label || source.is_some_and(|s| s != text(cx, target)) {
            return None;
        }
        source = Some(text(cx, target));
    }
    source
}

pub(crate) fn struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call_expression" || super::calls::is_subscript(n) {
        return None;
    }
    let callee = trimmed(cx, n.named_child(0)?);
    let arguments = super::calls::value_arguments(n);
    let labeled = arguments
        .iter()
        .all(|a| child_by_field(*a, "name").is_some());
    let copies = !arguments.is_empty() && labeled;
    if !copies || !callee.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(arguments.len()),
        source: copy_source(cx, &arguments),
    })
}

fn global_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for c in named_children(root) {
        if c.kind() != "property_declaration" {
            continue;
        }
        if !is_var(cx, c) {
            continue;
        }
        names.extend(bound_names(cx, c).into_iter().map(|(name, _)| name));
    }
    names
}

fn global_declares(cx: &Cx<'_, '_>, _n: Node<'_>, name: &str) -> bool {
    cx.file_names(FileSetKind::Scope, || global_names(cx))
        .contains(&name)
}

fn report_global_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let target = child_by_field(n, "target").and_then(|t| root_of(cx, t));
    if let Some((name, pos)) = target
        && name != "_"
        && global_declares(cx, n, name)
    {
        out.push((AmbientKind::GlobalWrite, name, pos));
    }
}

fn global_scope(cx: &Cx<'_, '_>, decl: Node<'_>) -> bool {
    let nested = cx
        .ancestors()
        .any(|a| matches!(a.kind(), "class_declaration" | "protocol_declaration"));
    !nested
        || any_named_child(decl, |c| {
            c.kind() == "modifiers"
                && any_named_child(c, |m| {
                    m.kind() == "property_modifier" && matches!(trimmed(cx, m), "static" | "class")
                })
        })
}

fn eq_ignoring_whitespace(cx: &Cx<'_, '_>, n: Node<'_>, want: &str) -> bool {
    text(cx, n)
        .chars()
        .filter(|c| !c.is_whitespace())
        .eq(want.chars())
}

pub(crate) fn ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "property_declaration" if !in_function && is_var(cx, n) && global_scope(cx, n) => {
            let names = bound_names(cx, n);
            out.extend(
                names
                    .into_iter()
                    .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
            );
        }
        "navigation_expression"
            if eq_ignoring_whitespace(cx, n, "ProcessInfo.processInfo.environment") =>
        {
            out.push((AmbientKind::EnvRead, trimmed(cx, n), pos_of(n)));
        }
        "call_expression" => out.extend(
            n.named_child(0)
                .filter(|c| eq_ignoring_whitespace(cx, *c, "getenv"))
                .map(|c| (AmbientKind::EnvRead, trimmed(cx, c), pos_of(n))),
        ),
        "assignment" if in_function => {
            report_global_write(cx, n, out);
        }
        _ => {}
    }
}
