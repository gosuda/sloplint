//! Cpp type heads: class records, named types, and aliases.

use sloplint_core::facts::{ImplRef, TypeDef, Visibility};
use tree_sitter::Node;

use super::funcs::in_anonymous;
use super::imports::scope_of;
use crate::hooks::{
    Container, Cx, FileSetKind, TypeHead, any_named_child, child_by_field, has_child_kind,
    named_children, pos_of, text,
};
use crate::lang::c_family::{in_function, inner};

pub(crate) fn declared_names<'a, 's>(cx: &'a Cx<'_, 's>) -> &'a [&'s str] {
    cx.file_names(FileSetKind::Type, || {
        let mut names = Vec::new();
        let mut stack = Vec::new();
        if let Some(root) = cx.root() {
            stack.push(root);
        }
        while let Some(node) = stack.pop() {
            match node.kind() {
                "class_specifier" | "struct_specifier" | "union_specifier" => {
                    if let Some(name) = child_by_field(node, "name") {
                        names.push(text(cx, name));
                    }
                }
                "enumerator" => {
                    if let Some(name) = child_by_field(node, "name") {
                        names.push(text(cx, name));
                    }
                }
                _ => {}
            }
            stack.extend(named_children(node));
        }
        names
    })
}

pub(crate) fn class_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let body = child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    let anon = in_anonymous(cx);
    let base = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "base_class_clause");
    let child_abstract = is_abstract(cx, body);
    let mut impls = Vec::new();
    if let Some(clause) = base {
        for c in named_children(clause) {
            if let Some(found) = base_ref(cx, c, child_abstract) {
                impls.push(found);
            }
        }
    }
    let public = !anon && !in_function(cx);
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            abstraction: child_abstract,
        }),
        impls,
        container: Some(Container::Class {
            exported: !anon,
            implements: base.is_some(),
        }),
    })
}

fn base_ref<'s>(cx: &Cx<'_, 's>, n: Node<'_>, child_abstract: bool) -> Option<ImplRef<'s>> {
    if n.kind() != "type_identifier"
        && n.kind() != "qualified_identifier"
        && n.kind() != "template_type"
    {
        return None;
    }
    let full = text(cx, n);
    let plain = match full.find('<') {
        Some(at) => &full[..at],
        None => full,
    };
    Some(ImplRef {
        name: scope_of(plain).1,
        pos: pos_of(n),
        is_generic: plain != full,
        child_is_abstract: child_abstract,
    })
}

fn is_abstract<'s>(cx: &Cx<'_, 's>, body: Node<'_>) -> bool {
    any_named_child(body, |c| {
        c.kind() == "field_declaration" && field_pure(cx, c)
            || c.kind() == "function_definition" && has_child_kind(c, "pure_virtual_clause")
            || c.kind() == "inline_method_definition" && has_child_kind(c, "pure_virtual_clause")
    })
}

fn field_pure<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> bool {
    if !child_by_field(field, "default_value").is_some_and(|v| text(cx, v).trim() == "0") {
        return false;
    }
    let mut cursor = field.walk();
    field
        .children_by_field_name("declarator", &mut cursor)
        .any(|d| d.kind() == "function_declarator")
}

pub(crate) fn named_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if n.kind() == "union_specifier" || n.kind() == "enum_specifier" {
        child_by_field(n, "body")?;
    }
    let name_node = child_by_field(n, "name")?;
    let public = !in_anonymous(cx) && !in_function(cx);
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: if public {
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
            let public = !in_anonymous(cx) && !in_function(cx);
            return Some(TypeHead {
                def: Some(TypeDef {
                    name: text(cx, current),
                    pos: pos_of(current),
                    visibility: if public {
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
