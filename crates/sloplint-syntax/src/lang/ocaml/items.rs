use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Barrel, FieldCopy, Handler, Import, ImportKind, ImportScope, Pos, ScrutineeKind,
    TypeDef, Visibility, WildcardArm,
};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use super::OcamlHooks;
use super::calls::{case_stmt, infix_write_root};
use super::funcs::pattern_ident;
use super::params::{is_let_binding, is_parameter};
use crate::hooks::{
    Container, Cx, TypeHead, any_named_child, child_by_field, has_child_kind, last_segment,
    named_children, narrow, pos_of, text,
};

fn is_match_case(c: &Node<'_>) -> bool {
    c.kind() == "match_case"
}

fn is_ref_callee(cx: &Cx<'_, '_>, f: Node<'_>) -> bool {
    if f.kind() != "value_path" {
        return false;
    }
    text(cx, f) == "ref" || text(cx, f) == "Stdlib.ref"
}

fn is_ref_binding(cx: &Cx<'_, '_>, lb: Node<'_>) -> bool {
    if any_named_child(lb, |c| is_parameter(&c)) {
        return false;
    }
    let body = child_by_field(lb, "body");
    if !body.is_some_and(|b| b.kind() == "application_expression") {
        return false;
    }
    body.and_then(|b| child_by_field(b, "function"))
        .is_some_and(|f| is_ref_callee(cx, f))
}

pub(crate) fn decl_names_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "value_definition" {
        return;
    }
    let mutable = has_child_kind(n, "mutable");
    let bindings: Vec<Node<'_>> = named_children(n)
        .into_iter()
        .filter(is_let_binding)
        .collect();
    for lb in bindings {
        if !mutable && !is_ref_binding(cx, lb) {
            continue;
        }
        if let Some(pat) = child_by_field(lb, "pattern")
            && let Some(found) = pattern_ident(cx, pat)
        {
            out.push(found);
        }
    }
}

fn push_first_binding<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    for lb in named_children(n) {
        if lb.kind() != "let_binding" {
            continue;
        }
        if let Some(pat) = child_by_field(lb, "pattern")
            && let Some((name, pos)) = pattern_ident(cx, pat)
        {
            let global = (AmbientKind::GlobalMutable, name, pos);
            out.push(global);
            return;
        }
    }
}

fn is_env_callee(callee: &str) -> bool {
    callee == "Sys.getenv"
        || callee == "Sys.getenv_opt"
        || callee == "Unix.getenv"
        || callee == "Unix.environment"
}

fn file_global_names<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for value in named_children(root) {
        if value.kind() != "value_definition" {
            continue;
        }
        let mutable = has_child_kind(value, "mutable");
        for lb in named_children(value) {
            if !is_let_binding(&lb) {
                continue;
            }
            if !mutable && !is_ref_binding(cx, lb) {
                continue;
            }
            if let Some(pattern) = child_by_field(lb, "pattern")
                && let Some((name, _)) = pattern_ident(cx, pattern)
            {
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
    if n.kind() == "value_definition" {
        if in_function {
            return;
        }
        let mutable = has_child_kind(n, "mutable");
        let stored = any_named_child(n, |lb| is_let_binding(&lb) && is_ref_binding(cx, lb));
        if mutable || stored {
            push_first_binding(cx, n, out);
        }
        return;
    }
    if n.kind() == "infix_expression" {
        if !in_function {
            return;
        }
        if let Some((root, pos)) = infix_write_root(cx, n)
            && root != "_"
            && file_global_names(cx, n).contains(&root)
        {
            out.push((AmbientKind::GlobalWrite, root, pos));
        }
        return;
    }
    if n.kind() != "application_expression" {
        return;
    }
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let callee = text(cx, func);
    if is_env_callee(callee) {
        let read = (AmbientKind::EnvRead, callee, pos_of(n));
        out.push(read);
    }
}

pub(crate) fn imports_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
    let pos = pos_of(n);
    if n.kind() == "open_module" || n.kind() == "include_module" {
        let Some(module) = child_by_field(n, "module") else {
            return;
        };
        let module = Cow::Borrowed(text(cx, module));
        let opened = Import {
            pos,
            module,
            name: "",
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        };
        out.push(opened);
        return;
    }
    if n.kind() != "module_binding" {
        return;
    }
    let Some(body) = child_by_field(n, "body") else {
        return;
    };
    if body.kind() != "extended_module_path" {
        return;
    }
    let path = text(cx, body);
    let name = last_segment(path, &["."]);
    let found = child_by_field(n, "module_name").map(|m| text(cx, m));
    let alias = match found {
        Some(candidate) if candidate != name => Some(candidate),
        _ => None,
    };
    let module = Cow::Borrowed(path);
    let aliased = Import {
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
    out.push(aliased);
}

fn typedef_at(name: &str, pos: Pos, visibility: Visibility, abstraction: bool) -> TypeDef<'_> {
    TypeDef {
        name,
        pos,
        visibility,
        abstraction,
    }
}

pub(crate) fn typedef_head<'s>(
    hooks: &OcamlHooks,
    n: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<TypeHead<'s>> {
    if n.kind() == "module_type_definition" {
        let name_node = child_by_field(n, "module_type_name")?;
        let visibility = if cx.lang == Lang::OcamlInterface {
            Visibility::Exported
        } else {
            hooks.visibility_at(cx, n)
        };
        let def = typedef_at(text(cx, name_node), pos_of(name_node), visibility, true);
        return Some(TypeHead {
            def: Some(def),
            impls: Vec::new(),
            container: None,
        });
    }
    if n.kind() != "type_binding" {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let no_body = child_by_field(n, "body").is_none();
    let no_synonym = child_by_field(n, "synonym").is_none();
    let visibility = if cx.lang == Lang::OcamlInterface {
        Visibility::Exported
    } else {
        hooks.visibility_at(cx, n)
    };
    let def = typedef_at(
        text(cx, name_node),
        pos_of(name_node),
        visibility,
        no_body && no_synonym,
    );
    Some(TypeHead {
        def: Some(def),
        impls: Vec::new(),
        container: None,
    })
}

pub(crate) fn container_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
    if n.kind() != "structure" {
        return None;
    }
    let parent = cx.parent()?;
    if parent.kind() != "module_binding" {
        return Some(Container::Impl {
            interface_bound: false,
        });
    }
    let bound = child_by_field(parent, "module_type").is_some();
    Some(Container::Impl {
        interface_bound: bound,
    })
}

fn sibling_enum(parent: Node<'_>, skip: Node<'_>) -> bool {
    any_named_child(parent, |s| {
        if s.kind() != "match_case" || s.id() == skip.id() {
            return false;
        }
        child_by_field(s, "pattern")
            .is_some_and(|q| q.kind() == "constructor_pattern" || q.kind() == "tag_pattern")
    })
}

pub(crate) fn match_arm_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "match_case" {
        return None;
    }
    let pattern = child_by_field(n, "pattern");
    if !pattern.is_some_and(|p| p.kind() == "any_pattern") {
        return None;
    }
    let parent = cx.parent()?;
    let arm = WildcardArm {
        pos: pos_of(n),
        enum_like: sibling_enum(parent, n),
        scrutinee: ScrutineeKind::Other,
        variant_count: 0,
        has_nullable_ctor: false,
    };
    Some(arm)
}

pub(crate) fn handler_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
    if n.kind() != "try_expression" {
        return None;
    }
    let mut stmts = Vec::new();
    for case in named_children(n) {
        if !is_match_case(&case) {
            continue;
        }
        stmts.push(case_stmt(cx, case));
    }
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn field_source<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> Option<&'s str> {
    let path = child_by_field(field, "field_path")?;
    let body = child_by_field(field, "body")?;
    if body.kind() != "field_get_expression" {
        return None;
    }
    let record = child_by_field(body, "record")?;
    if !matches!(record.kind(), "identifier" | "value_path" | "value_name") {
        return None;
    }
    let target = child_by_field(body, "field")
        .map(|f| text(cx, f))
        .unwrap_or("");
    if target != text(cx, path) {
        return None;
    }
    Some(text(cx, record))
}

pub(crate) fn struct_lit_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
    if n.kind() != "record_expression" {
        return None;
    }
    let fields: Vec<Node<'_>> = named_children(n)
        .into_iter()
        .filter(|c| c.kind() == "field_expression")
        .collect();
    if fields.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    let mut complete = true;
    for field in &fields {
        match field_source(cx, *field) {
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

pub(crate) fn barrel_head(root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
    if root.kind() != "compilation_unit" {
        return None;
    }
    let kids: Vec<Node<'_>> = named_children(root)
        .into_iter()
        .filter(|c| c.kind() != "comment")
        .collect();
    if kids.is_empty() || !kids.iter().all(|c| c.kind() == "include_module") {
        return None;
    }
    let dir = match cx.path.rfind('/') {
        Some(at) => &cx.path[..at],
        None => "",
    };
    let mut targets = Vec::new();
    for child in kids {
        let Some(module) = child_by_field(child, "module") else {
            continue;
        };
        let name = text(cx, module);
        if name.contains('.') {
            continue;
        }
        let lower = name.to_lowercase();
        if dir.is_empty() {
            targets.push(vec![format!("{lower}.ml"), format!("{lower}.mli")]);
        } else {
            targets.push(vec![
                format!("{dir}/{lower}.ml"),
                format!("{dir}/{lower}.mli"),
            ]);
        }
    }
    (!targets.is_empty()).then_some(Barrel { targets })
}
