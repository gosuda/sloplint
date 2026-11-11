use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, CallKind, FieldCopy, Handler, ImplRef, Import, ImportKind, ImportScope, Pos, Stmt,
    TypeDef, Visibility,
};
use tree_sitter::Node;

use super::calls::{arg_expr, callee_text, first_arg};
use super::{attr_hit, has_mod, seg_last};
use crate::hooks::{
    Container, Cx, FileSetKind, TypeHead, child_by_field, find_map_named_child, first_named_child,
    first_named_child_matching, has_child_kind, named_children, pos_of, rfind_named_child, text,
};

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some((text(cx, current), pos_of(current))),
            "member_access_expression" | "element_access_expression" => {
                child_by_field(current, "expression")?
            }
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
    if n.kind() != "prefix_unary_expression" && n.kind() != "postfix_unary_expression" {
        return;
    }
    let arith = (0..n.child_count())
        .filter_map(|i| n.child(i))
        .any(|c| c.kind() == "++" || c.kind() == "--");
    if arith && let Some(e) = first_named_child(n) {
        out.extend(write_root(cx, e));
    }
}

fn collect_namespace<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if n.kind() != "namespace_declaration" && n.kind() != "file_scoped_namespace_declaration" {
        return;
    }
    let Some(nm) = child_by_field(n, "name") else {
        return;
    };
    let Some(first) = text(cx, nm).split('.').next() else {
        return;
    };
    if !first.is_empty() && !out.contains(&first) {
        out.push(first);
    }
}

fn file_namespace_firsts<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        collect_namespace(cx, n, &mut out);
        stack.extend(named_children(n));
    }
    out
}

fn external_root_for<'s>(cx: &Cx<'_, 's>, full: &'s str) -> Option<&'s str> {
    let first = full.split('.').next().unwrap_or(full);
    if first.is_empty() {
        return None;
    }
    if cx
        .file_names(FileSetKind::Module, || file_namespace_firsts(cx))
        .contains(&first)
    {
        return None;
    }
    Some(first)
}

pub(crate) fn push_imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    if n.kind() != "using_directive" {
        return;
    }
    let alias_node = child_by_field(n, "name");
    let alias_id = alias_node.map(|a| a.id());
    let Some(t) = rfind_named_child(n, |c| Some(c.id()) != alias_id) else {
        return;
    };
    let full = text(cx, t);
    if has_child_kind(n, "static") {
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(full),
            name: seg_last(full),
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: cx.inside_test_module(),
            is_reexport: false,
            external_root: external_root_for(cx, full),
        });
        return;
    }
    let name = seg_last(full);
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(match full.rfind('.') {
            Some(at) => &full[..at],
            None => "",
        }),
        name,
        alias: match alias_node.map(|a| text(cx, a)) {
            Some(a) if a != name => Some(a),
            _ => None,
        },
        kind: if alias_node.is_some() {
            ImportKind::Named
        } else {
            ImportKind::Namespace
        },
        scope: ImportScope::Module,
        in_test: cx.inside_test_module(),
        is_reexport: false,
        external_root: external_root_for(cx, full),
    });
}

fn is_typedef_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "struct_declaration"
            | "record_declaration"
            | "interface_declaration"
            | "enum_declaration"
    )
}

fn head_visibility(cx: &Cx<'_, '_>, n: Node<'_>, enclosed: bool) -> Visibility {
    if !enclosed || has_mod(cx, n, "private") {
        Visibility::Private
    } else if has_mod(cx, n, "public") || has_mod(cx, n, "protected") {
        Visibility::Exported
    } else {
        Visibility::Restricted
    }
}

fn push_base_impl<'s>(
    cx: &Cx<'_, 's>,
    t: Node<'_>,
    child_abstract: bool,
    has_base: &mut bool,
    impls: &mut Vec<ImplRef<'s>>,
) {
    if t.kind() == "argument_list" || t.kind() == "primary_constructor_base_type" {
        return;
    }
    *has_base = true;
    let raw = text(cx, t);
    impls.push(ImplRef {
        name: seg_last(raw),
        pos: pos_of(t),
        is_generic: has_child_kind(t, "type_argument_list") || raw.contains('<'),
        child_is_abstract: child_abstract,
    });
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let kind = n.kind();
    if !is_typedef_kind(kind) {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let inner = cx.innermost_container();
    let enclosed = match inner {
        None => true,
        Some(Container::Class { exported, .. }) => exported,
        Some(Container::Trait { public }) => public,
        Some(_) => true,
    };
    let visibility = head_visibility(cx, n, enclosed);
    let exported = visibility == Visibility::Exported;
    let child_abstract = has_mod(cx, n, "abstract");
    let mut impls = Vec::new();
    let mut has_base = false;
    for c in named_children(n) {
        if c.kind() != "base_list" {
            continue;
        }
        for t in named_children(c) {
            push_base_impl(cx, t, child_abstract, &mut has_base, &mut impls);
        }
    }
    let container = if kind == "interface_declaration" {
        Some(Container::Trait { public: exported })
    } else if kind == "enum_declaration" {
        None
    } else if attr_hit(cx, n, &["TestFixture", "TestClass"]) {
        Some(Container::TestModule)
    } else {
        Some(Container::Class {
            exported,
            implements: has_base,
        })
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility,
            abstraction: kind == "interface_declaration" || child_abstract,
        }),
        impls,
        container,
    })
}

fn throw_site<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    match (first_named_child(s), binding) {
        (None, _) => Stmt::Rethrow,
        (Some(e), Some(b)) if e.kind() == "identifier" && text(cx, e) == b => Stmt::Rethrow,
        _ => Stmt::Other,
    }
}

fn unwrap_stmt_call(n: Node<'_>) -> Node<'_> {
    let mut cur = n;
    while cur.kind() == "await_expression" {
        let Some(next) = first_named_child(cur) else {
            break;
        };
        cur = next;
    }
    cur
}

fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "throw_statement" {
        return throw_site(cx, s, binding);
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    find_map_named_child(s, |c| {
        let u = unwrap_stmt_call(c);
        if u.kind() != "invocation_expression" {
            return None;
        }
        child_by_field(u, "function")
    })
    .map(|f| Stmt::Call {
        callee: callee_text(cx, f),
        kind: CallKind::Plain,
    })
    .unwrap_or(Stmt::Other)
}

pub(crate) fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let body = child_by_field(n, "body")?;
    let binding = first_named_child_matching(n, &["catch_declaration"])
        .and_then(|d| child_by_field(d, "name"))
        .map(|m| text(cx, m));
    let mut stmts = Vec::new();
    for s in named_children(body) {
        if s.kind() != "comment" {
            stmts.push(stmt_of(cx, s, binding));
        }
    }
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn copy_field<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> Option<&'s str> {
    if c.kind() != "assignment_expression" {
        return None;
    }
    if child_by_field(c, "operator")
        .map(|o| text(cx, o))
        .unwrap_or("")
        != "="
    {
        return None;
    }
    let l = child_by_field(c, "left")?;
    let r = child_by_field(c, "right")?;
    if l.kind() != "identifier" {
        return None;
    }
    if r.kind() != "member_access_expression" {
        return None;
    }
    let o = child_by_field(r, "expression")?;
    let m = child_by_field(r, "name")?;
    if o.kind() != "identifier" {
        return None;
    }
    if text(cx, m) != text(cx, l) {
        return None;
    }
    Some(text(cx, o))
}

pub(crate) fn struct_copy<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "initializer_expression" {
        return None;
    }
    if cx
        .parent()
        .is_none_or(|p| p.kind() != "object_creation_expression")
    {
        return None;
    }
    let mut fields = 0u32;
    let mut source = None;
    let mut uniform = true;
    for c in named_children(n) {
        let object = copy_field(cx, c)?;
        fields += 1;
        match source {
            None if uniform => source = Some(object),
            Some(prev) if uniform && prev == object => {}
            _ => {
                uniform = false;
                source = None;
            }
        }
    }
    if fields == 0 {
        return None;
    }
    if source == Some("this") {
        source = None;
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields,
        source,
    })
}
fn env_label<'s>(cx: &Cx<'_, 's>, invocation: Node<'_>, func: Node<'_>) -> &'s str {
    first_arg(invocation)
        .and_then(arg_expr)
        .map(|e| text(cx, e))
        .unwrap_or_else(|| text(cx, func))
}

fn ambient_write_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "assignment_expression" => {
            let left = child_by_field(n, "left")?;
            (left.kind() == "identifier").then_some(text(cx, left))
        }
        "prefix_unary_expression" | "postfix_unary_expression" => {
            let operand = first_named_child(n)?;
            (operand.kind() == "identifier").then_some(text(cx, operand))
        }
        _ => None,
    }
}

fn static_field_named(cx: &Cx<'_, '_>, field: Node<'_>, name: &str) -> bool {
    if field.kind() != "field_declaration"
        || !has_mod(cx, field, "static")
        || has_mod(cx, field, "readonly")
        || has_mod(cx, field, "const")
    {
        return false;
    }
    find_map_named_child(field, |decl| {
        if decl.kind() != "variable_declaration" {
            return None;
        }
        find_map_named_child(decl, |item| {
            if item.kind() != "variable_declarator" {
                return None;
            }
            child_by_field(item, "name").filter(|id| text(cx, *id) == name)
        })
    })
    .is_some()
}

fn in_static_scope<'s>(cx: &Cx<'_, 's>, n: Node<'_>, name: &str) -> bool {
    let mut current = cx.parent_of(n);
    while let Some(node) = current {
        if matches!(
            node.kind(),
            "class_declaration"
                | "struct_declaration"
                | "record_declaration"
                | "interface_declaration"
        ) {
            return named_children(node).into_iter().any(|child| {
                static_field_named(cx, child, name)
                    || named_children(child)
                        .into_iter()
                        .any(|sub| static_field_named(cx, sub, name))
            });
        }
        current = cx.parent_of(node);
    }
    false
}

pub(crate) fn ambient_sites<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "field_declaration" {
        push_static_field(cx, n, out);
        return;
    }
    if matches!(
        n.kind(),
        "assignment_expression" | "prefix_unary_expression" | "postfix_unary_expression"
    ) {
        if let Some(name) = ambient_write_name(cx, n)
            && in_static_scope(cx, n, name)
        {
            out.push((AmbientKind::GlobalWrite, name, pos_of(n)));
        }
        return;
    }
    if n.kind() != "invocation_expression" {
        return;
    }
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    if func.kind() != "member_access_expression" {
        return;
    }
    let expr = child_by_field(func, "expression")
        .map(|e| text(cx, e))
        .unwrap_or("");
    let name = child_by_field(func, "name")
        .map(|m| text(cx, m))
        .unwrap_or("");
    if (expr == "Environment" || expr == "System.Environment")
        && (name == "GetEnvironmentVariable" || name == "GetEnvironmentVariables")
    {
        out.push((AmbientKind::EnvRead, env_label(cx, n, func), pos_of(n)));
    }
}

fn push_static_field<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if !has_mod(cx, n, "static") || has_mod(cx, n, "readonly") || has_mod(cx, n, "const") {
        return;
    }
    if let Some(nm) = find_map_named_child(n, |c| {
        if c.kind() != "variable_declaration" {
            return None;
        }
        find_map_named_child(c, |d| {
            if d.kind() != "variable_declarator" {
                return None;
            }
            child_by_field(d, "name")
        })
    }) {
        out.push((AmbientKind::GlobalMutable, text(cx, nm), pos_of(nm)));
    }
}
