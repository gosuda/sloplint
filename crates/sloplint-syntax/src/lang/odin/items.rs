use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, FieldCopy, Import, ImportKind, ImportScope, Pos, ScrutineeKind, TypeDef,
    Visibility, WildcardArm,
};
use tree_sitter::Node;

use super::calls::qualified_callee;
use super::funcs::attr_has;
use super::{has_operator, is_anon_text, is_test_ident};
use crate::hooks::{
    Container, Cx, TypeHead, any_named_child, child_by_field, find_named_child, first_named_child,
    first_named_child_matching, named_children, narrow, pos_of, strip_quotes, text,
};
fn is_identifier(c: &Node<'_>) -> bool {
    c.kind() == "identifier"
}

fn names_before_sep<'a>(cx: &Cx<'_, '_>, parent: Node<'a>, seps: &[&str]) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for i in 0..parent.child_count() {
        let Some(child) = parent.child(i) else {
            continue;
        };
        if child.is_named() {
            if is_identifier(&child) {
                out.push(child);
            }
        } else if seps.iter().any(|sep| is_anon_text(cx, child, sep)) {
            break;
        }
    }
    out
}

pub(crate) fn decl_names_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "var_declaration" && n.kind() != "variable_declaration" {
        return;
    }
    for name_node in names_before_sep(cx, n, &[":", ":="]) {
        out.push((text(cx, name_node), pos_of(name_node)));
    }
}

fn odin_lhs(n: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    for i in 0..n.child_count() {
        let Some(child) = n.child(i) else { continue };
        if child.is_named() {
            out.push(child);
        } else {
            break;
        }
    }
    out
}

fn odin_write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        let kind = current.kind();
        if kind == "identifier" {
            return Some((text(cx, current), pos_of(current)));
        }
        if kind == "member_expression" && current.named_child_count() == 2 {
            current = current.named_child(0)?;
        } else if kind == "index_expression" {
            current = first_named_child(current)?;
        } else if kind == "parenthesized_expression" {
            current = child_by_field(current, "expression")?;
        } else {
            return None;
        }
    }
}

fn push_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    for target in odin_lhs(n) {
        if let Some(root) = odin_write_root(cx, target) {
            out.push(root);
        }
    }
}

pub(crate) fn write_roots_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "update_statement"
        || (n.kind() == "assignment_statement" && has_operator(n, cx, "="))
    {
        push_roots(cx, n, out);
    }
}

fn last_path_segment(path: &str) -> &str {
    let colon = path.rfind(':').map(|at| at + 1).unwrap_or(0);
    let slash = path.rfind('/').map(|at| at + 1).unwrap_or(0);
    &path[colon.max(slash).min(path.len())..]
}

pub(crate) fn imports_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
    if n.kind() != "import_declaration" {
        return;
    }
    let pos = pos_of(n);
    let raw = first_named_child_matching(n, &["string"])
        .map(|s| text(cx, s))
        .unwrap_or("");
    let module = Cow::Borrowed(strip_quotes(raw));
    let alias = child_by_field(n, "alias").map(|a| text(cx, a));
    let foreign =
        (0..n.child_count()).any(|i| n.child(i).is_some_and(|c| is_anon_text(cx, c, "foreign")));
    if foreign {
        let side = Import {
            pos,
            module,
            name: "",
            alias: None,
            kind: ImportKind::SideEffect,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        };
        out.push(side);
        return;
    }
    let inner = strip_quotes(raw);
    let name = alias.unwrap_or_else(|| last_path_segment(inner));
    let namespaced = Import {
        pos,
        module,
        name,
        alias,
        kind: ImportKind::Namespace,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    };
    out.push(namespaced);
}

fn typedef_at(name: &str, pos: Pos, visibility: Visibility) -> TypeDef<'_> {
    TypeDef {
        name,
        pos,
        visibility,
        abstraction: false,
    }
}

pub(crate) fn typedef_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
    match n.kind() {
        "struct_declaration" | "enum_declaration" | "union_declaration" => {}
        _ => return None,
    }
    let name_node = first_named_child_matching(n, &["expression"])?;
    let top = cx.parent().is_some_and(|p| p.kind() == "source_file");
    let visibility = if top && !attr_has(cx, n, "private") {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    let def = typedef_at(text(cx, name_node), pos_of(name_node), visibility);
    Some(TypeHead {
        def: Some(def),
        impls: Vec::new(),
        container: None,
    })
}

pub(crate) fn container_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
    if n.kind() != "attribute" {
        return None;
    }
    let tested = any_named_child(n, |c| is_test_ident(cx, c));
    tested.then_some(Container::TestModule)
}

fn is_implicit_selector(c: Node<'_>) -> bool {
    c.kind() == "member_expression" && c.named_child_count() == 1
}

fn sibling_enum(parent: Node<'_>, skip: Node<'_>) -> bool {
    any_named_child(parent, |s| {
        s.kind() == "switch_case"
            && s.id() != skip.id()
            && child_by_field(s, "condition").is_some_and(is_implicit_selector)
    })
}

pub(crate) fn match_arm_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "switch_case" {
        return None;
    }
    if child_by_field(n, "condition").is_some() {
        return None;
    }
    let parent = cx.parent()?;
    let arm = WildcardArm {
        pos: pos_of(n),
        enum_like: sibling_enum(parent, n),
        scrutinee: odin_scrutinee(parent),
        variant_count: odin_variant_count(parent, n),
        has_nullable_ctor: odin_nullable_sibling(parent, n, cx),
    };
    Some(arm)
}

fn odin_scrutinee(parent: Node<'_>) -> ScrutineeKind {
    let tag = named_children(parent)
        .into_iter()
        .rfind(|c| c.kind() != "switch_case" && c.kind() != "assignment_statement");
    match tag.map(|c| c.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("member_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}

fn odin_variant_count(parent: Node<'_>, skip: Node<'_>) -> u32 {
    let mut count = 0u32;
    for s in named_children(parent) {
        if s.kind() != "switch_case" || s.id() == skip.id() {
            continue;
        }
        if child_by_field(s, "condition").is_some_and(is_implicit_selector) {
            count = count.saturating_add(1);
        }
    }
    count
}

fn odin_nullable_sibling(parent: Node<'_>, skip: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    for s in named_children(parent) {
        if s.kind() != "switch_case" || s.id() == skip.id() {
            continue;
        }
        let Some(condition) = child_by_field(s, "condition") else {
            continue;
        };
        let last = text(cx, condition).rsplit('.').next().unwrap_or("");
        if matches!(last, "Some" | "None" | "Ok" | "Err") {
            return true;
        }
    }
    false
}

fn odin_field_source<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> Option<&'s str> {
    let assigned =
        (0..field.child_count()).any(|i| field.child(i).is_some_and(|c| is_anon_text(cx, c, "=")));
    if !assigned {
        return None;
    }
    if field.named_child_count() != 2 {
        return None;
    }
    let (Some(first), Some(member)) = (field.named_child(0), field.named_child(1)) else {
        return None;
    };
    if member.kind() != "member_expression" || member.named_child_count() != 2 {
        return None;
    }
    let (Some(part), Some(other)) = (member.named_child(0), member.named_child(1)) else {
        return None;
    };
    if !is_identifier(&part) {
        return None;
    }
    if text(cx, other) != text(cx, first) {
        return None;
    }
    Some(text(cx, part))
}

pub(crate) fn struct_lit_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
    if n.kind() != "struct" {
        return None;
    }
    let fields: Vec<Node<'_>> = named_children(n)
        .into_iter()
        .filter(|c| c.kind() == "struct_field")
        .collect();
    if fields.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    let mut complete = true;
    for field in &fields {
        match odin_field_source(cx, *field) {
            Some(current) if source.is_none_or(|prev| prev == current) => {
                source = Some(current);
            }
            _ => {
                complete = false;
                break;
            }
        }
    }
    let source = if complete { source } else { None };
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(fields.len()),
        source,
    })
}

fn is_env_callee(callee: &str) -> bool {
    callee == "os.get_env"
        || callee == "os.lookup_env"
        || callee == "os2.get_env"
        || callee == "os2.lookup_env"
}

fn odin_file_globals<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for decl in named_children(root) {
        if decl.kind() != "var_declaration" && decl.kind() != "variable_declaration" {
            continue;
        }
        if let Some(id) = find_named_child(decl, |c| is_identifier(&c)) {
            let name = text(cx, id);
            if name != "_" {
                names.push(name);
            }
        }
    }
    names
}

pub(crate) fn ambient_head<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "var_declaration" || n.kind() == "variable_declaration" {
        if in_function {
            return;
        }
        let found = find_named_child(n, |c| is_identifier(&c));
        if let Some(id) = found {
            let global = (AmbientKind::GlobalMutable, text(cx, id), pos_of(id));
            out.push(global);
        }
        return;
    }
    if n.kind() == "update_statement"
        || (n.kind() == "assignment_statement" && has_operator(n, cx, "="))
    {
        if in_function {
            for target in odin_lhs(n) {
                if let Some((root, pos)) = odin_write_root(cx, target)
                    && root != "_"
                    && odin_file_globals(cx, n).contains(&root)
                {
                    out.push((AmbientKind::GlobalWrite, root, pos));
                }
            }
        }
        return;
    }
    if n.kind() != "call_expression" {
        return;
    }
    let callee = child_by_field(n, "function")
        .or_else(|| first_named_child(n))
        .map(|f| qualified_callee(cx, n, f))
        .unwrap_or("");
    if is_env_callee(callee) {
        let read = (AmbientKind::EnvRead, callee, pos_of(n));
        out.push(read);
    }
}
