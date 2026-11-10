//! ObjC imports, type definitions, handlers, and ambient facts.

use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, CallKind, Import, ImportKind, ImportScope, Pos, Stmt, TypeDef, Visibility,
};
use tree_sitter::Node;

use crate::hooks::{
    Cx, TypeHead, child_by_field, find_named_child, first_named_child, first_named_child_matching,
    named_children, pos_of, strip_quotes, text,
};
use crate::lang::c_family::{self, C_DECLARATORS, angles, flat, in_function, inner, word};

use super::calls::first_selector;

pub(crate) fn push_include<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(path) = child_by_field(n, "path") else {
        return;
    };
    let module = match path.kind() {
        "string_literal" => strip_quotes(text(cx, path)),
        "system_lib_string" => angles(text(cx, path)),
        _ => text(cx, path),
    };
    out.push(side_effect(n, Cow::Borrowed(module)));
}
pub(crate) fn push_module<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let raw = flat(text(cx, n));
    let inner_text = raw.strip_prefix("@import").unwrap_or(&raw);
    let module = inner_text.strip_suffix(';').unwrap_or(inner_text);
    out.push(side_effect(n, Cow::Owned(module.to_string())));
}
fn side_effect<'s>(n: Node<'_>, module: Cow<'s, str>) -> Import<'s> {
    Import {
        pos: pos_of(n),
        module,
        name: "",
        alias: None,
        kind: ImportKind::SideEffect,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    }
}

pub(crate) fn tag_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: if !in_function(cx) {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            abstraction: false,
        }),
        impls: Vec::new(),
        container: None,
    })
}
pub(crate) fn alias_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let mut current = child_by_field(n, "declarator")?;
    loop {
        if current.kind() == "type_identifier" || current.kind() == "identifier" {
            return Some(TypeHead {
                def: Some(TypeDef {
                    name: text(cx, current),
                    pos: pos_of(current),
                    visibility: if !in_function(cx) {
                        Visibility::Exported
                    } else {
                        Visibility::Private
                    },
                    abstraction: false,
                }),
                impls: Vec::new(),
                container: None,
            });
        }
        {
            let next = inner(current)?;
            current = next
        }
    }
}
pub(crate) fn class_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if child_by_field(n, "category").is_some() {
        return None;
    }
    let superclass = child_by_field(n, "superclass");
    let name_node = find_named_child(n, |c| c.kind() == "identifier" && Some(c) != superclass)?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: Visibility::Exported,
            abstraction: false,
        }),
        impls: Vec::new(),
        container: None,
    })
}
pub(crate) fn protocol_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let name_node = first_named_child_matching(n, &["identifier"])?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: Visibility::Exported,
            abstraction: true,
        }),
        impls: Vec::new(),
        container: None,
    })
}
pub(crate) fn catch_binding<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let type_name = first_named_child_matching(n, &["type_name"])?;
    let id = first_named_child_matching(type_name, &["identifier"]);
    if let Some(found) = id {
        return Some(text(cx, found));
    }
    let declarator = first_named_child_matching(type_name, &["pointer_declarator"])?;
    first_ident(cx, declarator)
}
fn first_ident<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut stack = named_children(n);
    while let Some(next) = stack.pop() {
        if next.kind() == "identifier" {
            return Some(text(cx, next));
        }
        stack.extend(named_children(next));
    }
    None
}
pub(crate) fn catch_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, bound: Option<&'s str>) -> Stmt<'s> {
    if stmt.kind() == "expression_statement" {
        let expr = child_by_field(stmt, "expression").or_else(|| first_named_child(stmt));
        if expr.is_some_and(|e| e.kind() == "call_expression") {
            return call_stmt(cx, expr);
        }
        if expr.is_some_and(|e| e.kind() == "message_expression") {
            let callee = expr.and_then(|e| first_selector(cx, e)).unwrap_or("");
            return Stmt::Call {
                callee,
                kind: CallKind::Plain,
            };
        }
        return Stmt::Other;
    }
    if stmt.kind() == "message_expression" {
        return Stmt::Call {
            callee: first_selector(cx, stmt).unwrap_or(""),
            kind: CallKind::Plain,
        };
    }
    if stmt.kind() == "throw_statement" {
        let thrown = first_named_child(stmt);
        match thrown {
            None => return Stmt::Rethrow,
            Some(found) if found.kind() == "identifier" && Some(text(cx, found)) == bound => {
                return Stmt::Rethrow;
            }
            _ => return Stmt::Other,
        }
    }
    Stmt::Other
}
fn call_stmt<'s>(cx: &Cx<'_, 's>, expr: Option<Node<'_>>) -> Stmt<'s> {
    let callee = expr
        .and_then(|e| child_by_field(e, "function"))
        .map(|f| text(cx, f))
        .unwrap_or("");
    Stmt::Call {
        callee,
        kind: CallKind::Plain,
    }
}
pub(crate) fn global_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if word(cx, n, &["type_qualifier"], &["const"]) {
        return;
    }
    if word(cx, n, &["storage_class_specifier"], &["extern"]) {
        return;
    }
    let (names, func) = c_family::decl_info(cx, n, C_DECLARATORS.wrap);
    if func {
        return;
    }
    for (name, pos) in names {
        out.push((AmbientKind::GlobalMutable, name, pos));
    }
}
pub(crate) fn dot_env<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let field = child_by_field(n, "field")?;
    if text(cx, field) != "environment" {
        return None;
    }
    let arg = child_by_field(n, "argument")?;
    let raw = flat(text(cx, arg));
    (raw == "NSProcessInfo.processInfo").then_some((text(cx, n), pos_of(n)))
}
pub(crate) fn msg_env<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cursor = n.walk();
    let mut methods = n.children_by_field_name("method", &mut cursor);
    let only = methods.next()?;
    if methods.next().is_some() {
        return None;
    }
    if only.kind() != "identifier" || text(cx, only) != "environment" {
        return None;
    }
    let receiver = child_by_field(n, "receiver")?;
    let raw = flat(text(cx, receiver));
    (raw == "[NSProcessInfoprocessInfo]").then_some((text(cx, n), pos_of(n)))
}
