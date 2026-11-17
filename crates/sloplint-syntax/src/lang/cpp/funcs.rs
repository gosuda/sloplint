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
        .unwrap_or_else(|| cx.parent_of(items).is_some_and(head_is_struct))
}

pub(crate) fn access_marker(cx: &Cx<'_, '_>, sib: Node<'_>) -> Option<bool> {
    if sib.kind() == "access_specifier" {
        let label = trimmed(cx, sib);
        return Some(label.starts_with("public") || label.starts_with("protected"));
    }
    // Qt resets access to private; legacy Unreal class/interface macros reset
    // it to public. GENERATED_BODY and its USTRUCT alias preserve access.
    if sib.kind() == "macro_declaration" {
        return match sib.child(0).map(|name| name.kind()) {
            Some("Q_OBJECT") | Some("Q_GADGET") => Some(false),
            Some("GENERATED_UCLASS_BODY")
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

/// First `preproc_*_in_guarded_class` arm of a `guarded_class` node. `#else`
/// and `#elif` arms nest inside it as `alternative` children.
fn guarded_arm(n: Node<'_>) -> Option<Node<'_>> {
    named_children(n)
        .into_iter()
        .find(|c| c.kind().ends_with("_in_guarded_class"))
}

/// Class name for both specifier kinds and `guarded_class`: the arm carries the
/// directive's own `name` field (the `#ifdef` macro) before the class name, so
/// the last `name` field of the arm is the class.
pub(crate) fn class_name_node<'t>(class: Node<'t>) -> Option<Node<'t>> {
    if class.kind() != "guarded_class" {
        return child_by_field(class, "name");
    }
    let arm = guarded_arm(class)?;
    let mut cursor = arm.walk();
    arm.children_by_field_name("name", &mut cursor).last()
}

/// Class body: the `body` field on specifiers, the shared
/// `field_declaration_list` child on `guarded_class`.
pub(crate) fn class_body<'t>(class: Node<'t>) -> Option<Node<'t>> {
    child_by_field(class, "body").or_else(|| {
        (class.kind() == "guarded_class")
            .then(|| {
                named_children(class)
                    .into_iter()
                    .find(|c| c.kind() == "field_declaration_list")
            })
            .flatten()
    })
}

/// Members default public under `struct`/`union` — for `guarded_class` the
/// keyword is an anonymous token inside the head arm.
pub(crate) fn head_is_struct(class: Node<'_>) -> bool {
    match class.kind() {
        "struct_specifier" | "union_specifier" => true,
        "guarded_class" => guarded_arm(class).is_some_and(|arm| {
            let mut cursor = arm.walk();
            arm.children(&mut cursor)
                .any(|c| c.kind() == "struct" || c.kind() == "union")
        }),
        _ => false,
    }
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
        if matches!(
            node.kind(),
            "class_specifier" | "struct_specifier" | "guarded_class"
        ) && class_name_node(node).is_some_and(|id| text(cx, id) == scope)
        {
            return class_slot(cx, node, name);
        }
        stack.extend(named_children(node));
    }
    None
}

fn class_slot(cx: &Cx<'_, '_>, class: Node<'_>, name: &str) -> Option<ScopeMember> {
    let body = class_body(class)?;
    let fallback = head_is_struct(class);
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
