//! Cpp function heads, visibility, and return types.

use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, has_child_kind, named_children, text, trimmed};
use crate::lang::c_family::{self, inner, span};

pub(crate) fn member_list<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Option<Node<'t>> {
    let parent = cx.parent_of(n)?;
    if parent.kind() == "field_declaration_list" {
        return Some(parent);
    }
    if parent.kind() == "template_declaration" {
        return cx
            .parent_of(parent)
            .filter(|g| g.kind() == "field_declaration_list");
    }
    None
}

pub(crate) fn is_public<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    list: Option<Node<'_>>,
    slot: Option<ScopeMember>,
) -> bool {
    if in_anonymous(cx) {
        return false;
    }
    if let Some(found) = slot {
        return found.public;
    }
    let Some(items) = list else {
        return !c_family::has_static(cx, n);
    };
    let anchor = cx
        .parent()
        .filter(|p| p.kind() == "template_declaration")
        .unwrap_or(n);
    cx.members
        .state_before(items, anchor, |sib| access_marker(cx, sib))
        .unwrap_or_else(|| {
            cx.parent_of(items)
                .is_some_and(|g| g.kind() == "struct_specifier")
        })
}

pub(crate) fn access_marker(cx: &Cx<'_, '_>, sib: Node<'_>) -> Option<bool> {
    if sib.kind() == "access_specifier" {
        let label = trimmed(cx, sib);
        return Some(label.starts_with("public") || label.starts_with("protected"));
    }
    // Generator macros inject declarations that end in an access specifier:
    // Qt's Q_OBJECT/Q_GADGET leave `private:` in force and Unreal's legacy
    // GENERATED_*_BODY macros leave `public:`. UE5's GENERATED_BODY has no
    // trailing specifier and keeps the incoming access. Without those
    // transitions a member after Q_OBJECT in a `struct` reads public.
    if sib.kind() == "macro_declaration" {
        return match sib.child(0).map(|name| name.kind()) {
            Some("Q_OBJECT") | Some("Q_GADGET") => Some(false),
            Some("GENERATED_UCLASS_BODY")
            | Some("GENERATED_USTRUCT_BODY")
            | Some("GENERATED_UINTERFACE_BODY")
            | Some("GENERATED_IINTERFACE_BODY") => Some(true),
            _ => None,
        };
    }
    None
}

pub(crate) fn in_anonymous(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors().any(|found| {
        found.kind() == "namespace_definition" && child_by_field(found, "name").is_none()
    })
}

#[derive(Clone, Copy)]
pub(crate) struct ScopeMember {
    pub(crate) forced: bool,
    pub(crate) public: bool,
}

pub(crate) fn scope_member(cx: &Cx<'_, '_>, scope: &str, name: &str) -> Option<ScopeMember> {
    let mut stack = Vec::new();
    if let Some(root) = cx.root() {
        stack.push(root);
    }
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "class_specifier" | "struct_specifier")
            && child_by_field(node, "name").is_some_and(|id| text(cx, id) == scope)
        {
            return class_slot(cx, node, name);
        }
        stack.extend(named_children(node));
    }
    None
}

fn class_slot(cx: &Cx<'_, '_>, class: Node<'_>, name: &str) -> Option<ScopeMember> {
    let body = child_by_field(class, "body")?;
    let fallback = class.kind() == "struct_specifier";
    for cand in named_children(body) {
        if cand.kind() != "field_declaration" || !declares_name(cx, cand, name) {
            continue;
        }
        let forced = has_child_kind(cand, "virtual") || has_virtual_spec(cand);
        let public = cx
            .members
            .state_before(body, cand, |sib| access_marker(cx, sib))
            .unwrap_or(fallback);
        return Some(ScopeMember { forced, public });
    }
    None
}

fn declares_name(cx: &Cx<'_, '_>, cand: Node<'_>, name: &str) -> bool {
    let mut cursor = cand.walk();
    cand.children_by_field_name("declarator", &mut cursor)
        .any(|d| decl_name(cx, d).is_some_and(|found| found == name))
}

fn decl_name<'s>(cx: &Cx<'_, 's>, mut node: Node<'_>) -> Option<&'s str> {
    loop {
        match node.kind() {
            "identifier" | "field_identifier" | "destructor_name" | "operator_name" => {
                return Some(text(cx, node));
            }
            "qualified_identifier" | "template_function" | "template_method" => {
                return child_by_field(node, "name").map(|id| text(cx, id));
            }
            _ => node = inner(node)?,
        }
    }
}

fn has_virtual_spec(node: Node<'_>) -> bool {
    let mut stack = vec![node];
    while let Some(next) = stack.pop() {
        if next.kind() == "virtual_specifier" {
            return true;
        }
        stack.extend(named_children(next));
    }
    false
}

pub(crate) fn is_bound(n: Node<'_>, fd: Node<'_>) -> bool {
    has_child_kind(fd, "virtual_specifier") || has_child_kind(n, "virtual")
}
pub(crate) fn scope_name<'s>(cx: &Cx<'_, 's>, fd: Node<'_>) -> Option<&'s str> {
    let found = child_by_field(fd, "declarator")?;
    if found.kind() != "qualified_identifier" {
        return None;
    }
    let scope = child_by_field(found, "scope")?;
    if scope.kind() == "template_type" {
        return child_by_field(scope, "name").map(|name| text(cx, name));
    }
    Some(text(cx, scope))
}

pub(crate) fn return_type<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    if let Some(trailing) = child_by_field(n, "trailing_return_type") {
        return child_by_field(trailing, "type_descriptor").map(|t| text(cx, t));
    }
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
}
