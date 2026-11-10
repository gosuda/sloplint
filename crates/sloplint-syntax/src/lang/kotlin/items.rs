use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, CallKind, Handler, ImplRef, Import, ImportKind, ImportScope, Pos, ScrutineeKind,
    Stmt, TypeDef, Visibility, WildcardArm,
};
use tree_sitter::Node;

use super::calls::callee_of;
use super::{code_children, first_code, has_modifier_word, stricter_visibility, type_visibility};
use crate::hooks::{
    Container, Cx, FileSetKind, TypeHead, child_by_field, first_of_kind, has_child_kind,
    last_segment, named_children, pos_of, rfind_named_child, text,
};

pub(crate) fn delegate_ref<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    child_is_abstract: bool,
    is_generic: bool,
) -> Option<ImplRef<'s>> {
    let user = first_of_kind(spec, &["constructor_invocation"])
        .and_then(|invocation| first_of_kind(invocation, &["user_type"]))
        .or_else(|| first_of_kind(spec, &["user_type"]))?;
    let last = rfind_named_child(user, |child| child.kind() == "identifier")?;
    Some(ImplRef {
        name: text(cx, last),
        pos: pos_of(last),
        is_generic,
        child_is_abstract,
    })
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let name = match n.kind() {
        "type_alias" => child_by_field(n, "type")?,
        "class_declaration" | "object_declaration" => child_by_field(n, "name")?,
        _ => return None,
    };
    let mut visibility = type_visibility(cx, n);
    for p in cx.ancestors() {
        match p.kind() {
            "object_literal" => visibility = Visibility::Private,
            "class_declaration" | "object_declaration" => {
                visibility = stricter_visibility(visibility, type_visibility(cx, p));
            }
            _ => {}
        }
        if visibility == Visibility::Private {
            break;
        }
    }
    let is_class = n.kind() == "class_declaration";
    let is_interface = is_class && has_child_kind(n, "interface");
    let abstraction = is_class && (is_interface || has_modifier_word(cx, n, "abstract"));
    let container = match n.kind() {
        "type_alias" => None,
        _ if is_interface => Some(Container::Trait {
            public: visibility == Visibility::Exported,
        }),
        _ => Some(Container::Class {
            exported: visibility == Visibility::Exported,
            implements: false,
        }),
    };
    let is_generic = has_child_kind(n, "type_parameters");
    let impls: Vec<ImplRef<'s>> =
        first_of_kind(n, &["delegation_specifiers"]).map_or_else(Vec::new, |list| {
            named_children(list)
                .into_iter()
                .filter(|child| child.kind() == "delegation_specifier")
                .filter_map(|spec| delegate_ref(cx, spec, abstraction, is_generic))
                .collect()
        });
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name),
            pos: pos_of(name),
            visibility,
            abstraction,
        }),
        impls,
        container,
    })
}

pub(crate) fn import_fact<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Import<'s>> {
    let pos = pos_of(n);
    let qualified = first_of_kind(n, &["qualified_identifier"]);
    let count = u32::try_from(n.named_child_count()).unwrap_or(u32::MAX);
    let mut identifiers = (0..count)
        .filter_map(|i| n.named_child(i))
        .filter(|child| child.kind() == "identifier");
    let (path, alias) = match qualified {
        Some(qualified) => (
            text(cx, qualified),
            identifiers.next().map(|id| text(cx, id)),
        ),
        None => {
            let first = identifiers.next()?;
            (text(cx, first), identifiers.next().map(|id| text(cx, id)))
        }
    };
    if has_child_kind(n, "*") {
        return Some(Import {
            pos,
            module: Cow::Borrowed(path),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        });
    }
    let (module, name) = path.rsplit_once('.').unwrap_or(("", path));
    Some(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias,
        kind: ImportKind::Named,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    })
}

fn push_variable_name<'s>(cx: &Cx<'_, 's>, variable: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if let Some(id) = first_of_kind(variable, &["identifier"]) {
        out.push((text(cx, id), pos_of(id)));
    }
}

fn variable_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    for child in named_children(n) {
        if child.kind() == "variable_declaration" {
            push_variable_name(cx, child, out);
        } else if child.kind() == "multi_variable_declaration" {
            for inner in named_children(child) {
                if inner.kind() == "variable_declaration" {
                    push_variable_name(cx, inner, out);
                }
            }
        }
    }
}

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "property_declaration" && has_child_kind(n, "var") {
        variable_names(cx, n, out);
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this_expression" => return Some((text(cx, current), pos_of(current))),
            "navigation_expression" | "index_expression" | "parenthesized_expression" => {
                first_code(current)?
            }
            _ => return None,
        };
    }
}

pub(crate) fn write_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let target = match n.kind() {
        "assignment" => child_by_field(n, "left"),
        "unary_expression"
            if child_by_field(n, "operator")
                .is_some_and(|op| matches!(text(cx, op), "++" | "--")) =>
        {
            child_by_field(n, "argument")
        }
        _ => None,
    };
    out.extend(target.and_then(|target| write_root(cx, target)));
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    match stmt.kind() {
        "call_expression" => match callee_of(cx, stmt) {
            Some(callee) => Stmt::Call {
                callee,
                kind: CallKind::Plain,
            },
            None => Stmt::Other,
        },
        "throw_expression" => {
            let rethrows = first_code(stmt).is_some_and(|inner| {
                inner.kind() == "identifier" && binding == Some(text(cx, inner))
            });
            if rethrows { Stmt::Rethrow } else { Stmt::Other }
        }
        _ => Stmt::Other,
    }
}

pub(crate) fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_block" {
        return None;
    }
    let binding = first_of_kind(n, &["identifier"]).map(|id| text(cx, id));
    let body = first_of_kind(n, &["block"])?;
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

fn enum_condition(cx: &Cx<'_, '_>, condition: Node<'_>) -> bool {
    match condition.kind() {
        "navigation_expression" | "type_test" => true,
        "identifier" => !matches!(text(cx, condition), "true" | "false"),
        _ => false,
    }
}

fn sibling_entry<'s>(
    cx: &Cx<'_, 's>,
    entry: Node<'_>,
    self_id: usize,
    variants: &mut u32,
    nullable: &mut bool,
) {
    if entry.kind() != "when_entry" || entry.id() == self_id {
        return;
    }
    let mut cursor = entry.walk();
    for cond in entry.children_by_field_name("condition", &mut cursor) {
        if enum_condition(cx, cond) {
            *variants = variants.saturating_add(1);
        }
        if matches!(
            last_segment(text(cx, cond), &["."]),
            "Some" | "None" | "Ok" | "Err"
        ) {
            *nullable = true;
        }
    }
}

fn sibling_variants(cx: &Cx<'_, '_>, parent: Node<'_>, self_id: usize) -> (u32, bool) {
    let mut variant_count = 0u32;
    let mut has_nullable_ctor = false;
    for entry in named_children(parent) {
        sibling_entry(
            cx,
            entry,
            self_id,
            &mut variant_count,
            &mut has_nullable_ctor,
        );
    }
    (variant_count, has_nullable_ctor)
}

pub(crate) fn match_arm<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "when_entry" || child_by_field(n, "condition").is_some() {
        return None;
    }
    let parent = cx.parent()?;
    let subject = first_of_kind(parent, &["when_subject"]).and_then(first_code);
    let scrutinee = match subject.map(|s| s.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("navigation_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    };
    let (variant_count, has_nullable_ctor) = sibling_variants(cx, parent, n.id());
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: subject.is_some() && variant_count >= 2,
        scrutinee,
        variant_count,
        has_nullable_ctor,
    })
}

pub(crate) fn is_else_if(n: Node<'_>, parent: Option<Node<'_>>) -> bool {
    n.kind() == "if_expression"
        && parent.is_some_and(|parent| parent.kind() == "if_expression")
        && super::previous_code_sibling(n).is_some_and(|sibling| sibling.kind() == "else")
}

fn global_scope(cx: &Cx<'_, '_>) -> bool {
    match cx.parent() {
        Some(parent) if parent.kind() == "source_file" => true,
        Some(body) if body.kind() == "class_body" => cx
            .parent_of(body)
            .is_some_and(|owner| matches!(owner.kind(), "object_declaration" | "companion_object")),
        _ => false,
    }
}

fn ambient_write_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "assignment" => {
            let left = child_by_field(n, "left")?;
            (left.kind() == "identifier").then_some(text(cx, left))
        }
        "unary_expression" => {
            if !child_by_field(n, "operator").is_some_and(|op| matches!(text(cx, op), "++" | "--"))
            {
                return None;
            }
            let argument = child_by_field(n, "argument")?;
            (argument.kind() == "identifier").then_some(text(cx, argument))
        }
        _ => None,
    }
}

fn top_level_vars<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    if root.kind() != "source_file" {
        return Vec::new();
    }
    let mut out = Vec::new();
    for child in named_children(root) {
        if child.kind() != "property_declaration" || !has_child_kind(child, "var") {
            continue;
        }
        let mut names = Vec::new();
        variable_names(cx, child, &mut names);
        out.extend(names.into_iter().map(|(id, _)| id));
    }
    out
}

fn is_top_level_var<'s>(cx: &Cx<'_, 's>, name: &str) -> bool {
    cx.file_names(FileSetKind::Scope, || top_level_vars(cx))
        .contains(&name)
}

pub(crate) fn ambient_sites<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "property_declaration" if global_scope(cx) && has_child_kind(n, "var") => {
            let mut names = Vec::new();
            variable_names(cx, n, &mut names);
            out.extend(
                names
                    .into_iter()
                    .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
            );
        }
        "call_expression" => {
            if let Some(callee) = callee_of(cx, n).filter(|callee| *callee == "System.getenv") {
                out.push((AmbientKind::EnvRead, callee, pos_of(n)));
            }
        }
        "assignment" | "unary_expression" => {
            if let Some(name) = ambient_write_name(cx, n)
                && is_top_level_var(cx, name)
            {
                out.push((AmbientKind::GlobalWrite, name, pos_of(n)));
            }
        }
        _ => {}
    }
}
