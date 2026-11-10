//! Java type, import, statement, and match-arm heads.

use std::borrow::Cow;

use sloplint_core::facts::{
    CallKind, Handler, ImplRef, Import, ImportKind, ImportScope, Pos, ScrutineeKind, Stmt, TypeDef,
    Visibility, WildcardArm,
};
use tree_sitter::Node;

use super::calls::method_callee;
use super::{code_children, container_exports, first_code, has_keyword};
use crate::hooks::{
    Container, Cx, TypeHead, child_by_field, first_named_child_matching, first_of_kind,
    has_child_kind, named_children, narrow, pos_of, rfind_named_child, text,
};

fn type_name(n: Node<'_>) -> Option<Node<'_>> {
    let mut current = n;
    loop {
        match current.kind() {
            "type_identifier" => return Some(current),
            "generic_type" => {
                current = first_named_child_matching(
                    current,
                    &["type_identifier", "scoped_type_identifier"],
                )?;
            }
            "scoped_type_identifier" => {
                return rfind_named_child(current, |child| child.kind() == "type_identifier");
            }
            _ => return None,
        }
    }
}

fn push_type_ref<'s>(
    cx: &Cx<'_, 's>,
    ty: Node<'_>,
    is_generic: bool,
    child_is_abstract: bool,
    refs: &mut Vec<ImplRef<'s>>,
) {
    if let Some(name) = type_name(ty) {
        refs.push(ImplRef {
            name: text(cx, name),
            pos: pos_of(name),
            is_generic,
            child_is_abstract,
        });
    }
}

fn supertype_refs<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    is_generic: bool,
    child_is_abstract: bool,
) -> Vec<ImplRef<'s>> {
    let mut refs = Vec::new();
    for clause in named_children(n) {
        match clause.kind() {
            "superclass" => {
                for ty in named_children(clause) {
                    push_type_ref(cx, ty, is_generic, child_is_abstract, &mut refs);
                }
            }
            "super_interfaces" | "extends_interfaces" => {
                for list in named_children(clause) {
                    for ty in named_children(list) {
                        push_type_ref(cx, ty, is_generic, child_is_abstract, &mut refs);
                    }
                }
            }
            _ => {}
        }
    }
    refs
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if !matches!(
        n.kind(),
        "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
    ) {
        return None;
    }
    let name = child_by_field(n, "name")?;
    let in_trait = matches!(cx.innermost_container(), Some(Container::Trait { .. }));
    let visibility = if has_keyword(n, "private") {
        Visibility::Private
    } else if has_keyword(n, "public") || has_keyword(n, "protected") || in_trait {
        Visibility::Exported
    } else {
        Visibility::Restricted
    };
    let visibility = if visibility == Visibility::Exported && !container_exports(cx) {
        Visibility::Restricted
    } else {
        visibility
    };
    let exported = visibility == Visibility::Exported;
    let is_interface = n.kind() == "interface_declaration";
    let abstraction =
        is_interface || (n.kind() == "class_declaration" && has_keyword(n, "abstract"));
    let is_generic = child_by_field(n, "type_parameters").is_some();
    let container = if is_interface {
        Container::Trait { public: exported }
    } else {
        Container::Class {
            exported,
            implements: false,
        }
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name),
            pos: pos_of(name),
            visibility,
            abstraction,
        }),
        impls: supertype_refs(cx, n, is_generic, abstraction),
        container: Some(container),
    })
}

pub(crate) fn import_fact<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Import<'s>> {
    let path = first_of_kind(n, &["scoped_identifier", "identifier"])?;
    let pos = pos_of(n);
    if has_child_kind(n, "asterisk") {
        return Some(Import {
            pos,
            module: Cow::Borrowed(text(cx, path)),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        });
    }
    let (module, name) = match path.kind() {
        "scoped_identifier" => (
            child_by_field(path, "scope").map_or("", |scope| text(cx, scope)),
            child_by_field(path, "name").map_or("", |leaf| text(cx, leaf)),
        ),
        _ => ("", text(cx, path)),
    };
    Some(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias: None,
        kind: ImportKind::Named,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    })
}

pub(crate) fn is_else_if(n: Node<'_>, parent: Option<Node<'_>>) -> bool {
    parent
        .filter(|parent| parent.kind() == "if_statement")
        .and_then(|parent| child_by_field(parent, "alternative"))
        .is_some_and(|alternative| alternative.id() == n.id())
}

pub(crate) fn local_declared<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "local_variable_declaration" || has_keyword(n, "final") {
        return;
    }
    for declarator in named_children(n) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name) = child_by_field(declarator, "name") else {
            continue;
        };
        if name.kind() == "identifier" {
            out.push((text(cx, name), pos_of(name)));
        }
    }
}
fn push_final_local_shadows<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if !has_keyword(n, "final") {
        return;
    }
    for declarator in named_children(n) {
        if declarator.kind() == "variable_declarator"
            && let Some(name) = child_by_field(declarator, "name")
            && name.kind() == "identifier"
        {
            out.push(text(cx, name));
        }
    }
}
pub(crate) fn shadow_statement_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if n.kind() == "local_variable_declaration" {
        push_final_local_shadows(cx, n, out);
        return;
    }
    let name = match n.kind() {
        "enhanced_for_statement" | "catch_formal_parameter" => child_by_field(n, "name"),
        _ => None,
    };
    if let Some(name) = name
        && name.kind() == "identifier"
    {
        out.push(text(cx, name));
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some((text(cx, current), pos_of(current))),
            "field_access" => child_by_field(current, "object")?,
            "array_access" => child_by_field(current, "array")?,
            _ => return None,
        };
    }
}

pub(crate) fn write_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let target = match n.kind() {
        "assignment_expression" => child_by_field(n, "left"),
        "update_expression" => first_code(n),
        _ => None,
    };
    out.extend(target.and_then(|target| write_root(cx, target)));
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    let Some(inner) = first_code(stmt) else {
        return Stmt::Other;
    };
    match (stmt.kind(), inner.kind()) {
        ("expression_statement", "method_invocation") => Stmt::Call {
            callee: method_callee(cx, inner).unwrap_or(""),
            kind: CallKind::Plain,
        },
        ("throw_statement", "identifier") if binding == Some(text(cx, inner)) => Stmt::Rethrow,
        _ => Stmt::Other,
    }
}

pub(crate) fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let parameter = first_of_kind(n, &["catch_formal_parameter"])?;
    let binding = child_by_field(parameter, "name").map(|name| text(cx, name));
    let body = child_by_field(n, "body")?;
    let stmts = code_children(body)
        .into_iter()
        .map(|stmt| handler_stmt(cx, stmt, binding))
        .collect();
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn sibling_labels<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Vec<Node<'t>> {
    let Some(group) = cx.parent_of(n) else {
        return Vec::new();
    };
    let Some(block) = cx.parent_of(group) else {
        return Vec::new();
    };
    let mut labels = Vec::new();
    for child in named_children(block) {
        if child.kind() == "switch_label" {
            labels.push(child);
        } else {
            for label in named_children(child) {
                if label.kind() == "switch_label" {
                    labels.push(label);
                }
            }
        }
    }
    labels
}

fn switch_subject<'t>(cx: &Cx<'t, '_>, _n: Node<'_>) -> Option<Node<'t>> {
    cx.ancestors()
        .find(|node| matches!(node.kind(), "switch_expression" | "switch_statement"))
}

fn scrutinee_of(subject: Node<'_>) -> ScrutineeKind {
    let cond = child_by_field(subject, "condition").and_then(|cond| {
        if cond.kind() == "parenthesized_expression" {
            first_code(cond)
        } else {
            Some(cond)
        }
    });
    match cond.map(|selected| selected.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_access") => ScrutineeKind::Field,
        Some("scoped_identifier") => ScrutineeKind::Path,
        _ => ScrutineeKind::Other,
    }
}

fn enum_variant_count(label: Node<'_>) -> usize {
    named_children(label)
        .iter()
        .filter(|child| {
            matches!(
                child.kind(),
                "pattern"
                    | "type_pattern"
                    | "record_pattern"
                    | "identifier"
                    | "field_access"
                    | "scoped_identifier"
            )
        })
        .count()
}

pub(crate) fn switch_label_arm(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "switch_label" || n.named_child_count() != 0 || text(cx, n).trim() != "default" {
        return None;
    }
    let subject = switch_subject(cx, n)?;
    let variant_count = narrow(
        sibling_labels(cx, n)
            .iter()
            .map(|label| enum_variant_count(*label))
            .sum::<usize>(),
    );
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 2,
        scrutinee: scrutinee_of(subject),
        variant_count,
        has_nullable_ctor: false,
    })
}
