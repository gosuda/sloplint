use super::calls::{
    call_callee_last, call_stmt_callee, creation_args, first_arg_text, is_call_kind,
};
use super::funcs::enclosing_fn_name;
use crate::hooks::{
    Container, Cx, TypeHead, any_named_child, child_by_field, first_named_child, last_segment,
    named_children, narrow, pos_of, rfind_named_child, strip_quotes, text,
};
use sloplint_core::facts::{
    AmbientKind, FieldCopy, Handler, ImplRef, Pos, ScrutineeKind, Stmt, TypeDef, Visibility,
    WildcardArm,
};
use tree_sitter::Node;

fn var_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "variable_name" {
            let t = text(cx, current);
            out.push((t.strip_prefix('$').unwrap_or(t), pos_of(current)));
        } else {
            stack.extend(named_children(current));
        }
    }
}

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "foreach_statement" || n.kind() == "catch_clause" {
        let mut targets = Vec::new();
        super::binds::flow_targets(cx, n, &mut targets);
        for (name, pos) in targets {
            if !super::binds::captured_by_outer_ref(cx, name) {
                out.push((name, pos));
            }
        }
        return;
    }
    if !matches!(
        n.kind(),
        "assignment_expression"
            | "augmented_assignment_expression"
            | "reference_assignment_expression"
    ) {
        return;
    }
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    if left.kind() == "variable_name" {
        let t = text(cx, left);
        let name = t.strip_prefix('$').unwrap_or(t);
        if !super::binds::captured_by_outer_ref(cx, name) {
            out.push((name, pos_of(left)));
        }
    } else if left.kind() == "list_literal" {
        var_names(cx, left, out);
    }
}

fn descend<'s, 'c>(
    cx: &Cx<'_, 's>,
    current: Node<'c>,
    out: &mut Vec<(&'s str, Pos)>,
) -> Option<Node<'c>> {
    if current.kind() == "variable_name" {
        let t = text(cx, current);
        out.push((t.strip_prefix('$').unwrap_or(t), pos_of(current)));
        return None;
    }
    if current.kind() == "subscript_expression" {
        return first_named_child(current);
    }
    child_by_field(current, "object")
}

pub(crate) fn write_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "foreach_statement" || n.kind() == "catch_clause" {
        let mut targets = Vec::new();
        super::binds::flow_targets(cx, n, &mut targets);
        for (name, pos) in targets {
            if super::binds::captured_by_outer_ref(cx, name) {
                out.push((name, pos));
            }
        }
        return;
    }
    if matches!(
        n.kind(),
        "assignment_expression"
            | "augmented_assignment_expression"
            | "reference_assignment_expression"
    ) {
        if let Some(left) = child_by_field(n, "left")
            && left.kind() == "variable_name"
        {
            let t = text(cx, left);
            let name = t.strip_prefix('$').unwrap_or(t);
            if super::binds::captured_by_outer_ref(cx, name) {
                out.push((name, pos_of(left)));
            }
        }
        return;
    }
    if n.kind() != "update_expression" {
        return;
    }
    if super::binds::inside_arrow(cx) {
        return;
    }
    let Some(mut current) = child_by_field(n, "argument") else {
        return;
    };
    while let Some(next) = descend(cx, current, out) {
        current = next;
    }
}

fn clause_refs<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    child_is_abstract: bool,
    out: &mut Vec<ImplRef<'s>>,
) {
    for item in named_children(clause) {
        out.push(ImplRef {
            name: last_segment(text(cx, item), &["\\"]),
            pos: pos_of(item),
            is_generic: false,
            child_is_abstract,
        });
    }
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let kind = n.kind();
    if !matches!(
        kind,
        "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration"
    ) {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let child_abstract = is_abstract_type(kind, n);
    let (impls, implements) = collect_clauses(cx, n, child_abstract);
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: Visibility::Exported,
            abstraction: child_abstract,
        }),
        impls,
        container: container_of(kind, implements),
    })
}

fn is_abstract_type(kind: &str, n: Node<'_>) -> bool {
    kind == "interface_declaration"
        || kind == "trait_declaration"
        || any_named_child(n, |c| c.kind() == "abstract_modifier")
}

fn collect_clauses<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    child_is_abstract: bool,
) -> (Vec<ImplRef<'s>>, bool) {
    let mut impls = Vec::new();
    let mut implements = false;
    for child in named_children(n) {
        if child.kind() == "base_clause" {
            clause_refs(cx, child, child_is_abstract, &mut impls);
        } else if child.kind() == "class_interface_clause" {
            implements = true;
            clause_refs(cx, child, child_is_abstract, &mut impls);
        }
    }
    (impls, implements)
}

fn container_of(kind: &str, implements: bool) -> Option<Container> {
    if kind == "class_declaration" || kind == "enum_declaration" {
        return Some(Container::Class {
            exported: true,
            implements,
        });
    }
    Some(Container::Trait { public: true })
}

fn cond_kind(cond: Node<'_>) -> ScrutineeKind {
    let inner = if cond.kind() == "parenthesized_expression" {
        first_named_child(cond).unwrap_or(cond)
    } else {
        cond
    };
    match inner.kind() {
        "variable_name" => ScrutineeKind::Ident,
        "member_access_expression" | "nullsafe_member_access_expression" => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}

fn scrutinee_of(cx: &Cx<'_, '_>, _n: Node<'_>) -> ScrutineeKind {
    for p in cx.ancestors().take(3) {
        if let Some(cond) = child_by_field(p, "condition") {
            return cond_kind(cond);
        }
    }
    ScrutineeKind::Other
}

fn arm_variant_names<'s>(cx: &Cx<'_, 's>, arm: Node<'_>, out: &mut Vec<&'s str>) {
    if arm.kind() != "case_statement" && arm.kind() != "match_conditional_expression" {
        return;
    }
    for child in named_children(arm) {
        if child.kind() == "class_constant_access_expression" {
            out.push(text(cx, child));
        } else if child.kind() == "match_condition_list" {
            for cond in named_children(child) {
                if cond.kind() == "class_constant_access_expression" {
                    out.push(text(cx, cond));
                }
            }
        }
    }
}

fn sibling_variants(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let Some(parent) = cx.parent() else {
        return 0;
    };
    let mut names: Vec<&str> = Vec::new();
    for sib in named_children(parent) {
        if sib.id() != n.id() {
            arm_variant_names(cx, sib, &mut names);
        }
    }
    names.sort_unstable();
    names.dedup();
    narrow(names.len())
}

pub(crate) fn match_arm(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    let kind = n.kind();
    if kind != "default_statement" && kind != "match_default_expression" {
        return None;
    }
    let variants = sibling_variants(cx, n);
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variants >= 2,
        scrutinee: scrutinee_of(cx, n),
        variant_count: variants,
        has_nullable_ctor: false,
    })
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: &str) -> Stmt<'s> {
    let inner = if stmt.kind() == "expression_statement" {
        first_named_child(stmt).unwrap_or(stmt)
    } else {
        stmt
    };
    if is_call_kind(inner.kind()) || inner.kind() == "object_creation_expression" {
        let (callee, kind) = call_stmt_callee(cx, inner);
        return Stmt::Call { callee, kind };
    }
    if inner.kind() == "throw_expression" {
        let thrown = first_named_child(inner);
        let rethrown = thrown.is_some_and(|t| {
            t.kind() == "variable_name"
                && text(cx, t).strip_prefix('$').unwrap_or(text(cx, t)) == binding
        });
        if rethrown {
            return Stmt::Rethrow;
        }
    }
    Stmt::Other
}

fn try_imports_only(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    let Some(parent) = cx.parent().filter(|p| p.kind() == "try_statement") else {
        return false;
    };
    let Some(body) = child_by_field(parent, "body") else {
        return false;
    };
    let kids: Vec<Node<'_>> = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment")
        .collect();
    if kids.is_empty() {
        return false;
    }
    kids.iter().all(|s| {
        let inner = if s.kind() == "expression_statement" {
            first_named_child(*s).unwrap_or(*s)
        } else {
            *s
        };
        matches!(
            inner.kind(),
            "include_expression"
                | "include_once_expression"
                | "require_expression"
                | "require_once_expression"
        )
    })
}

pub(crate) fn handler_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let binding = child_by_field(n, "name")
        .map(|v| text(cx, v).strip_prefix('$').unwrap_or(text(cx, v)))
        .unwrap_or("");
    let body = child_by_field(n, "body")?;
    let mut stmts = Vec::new();
    for stmt in named_children(body) {
        if stmt.kind() == "comment" {
            continue;
        }
        stmts.push(handler_stmt(cx, stmt, binding));
    }
    let cleanup = enclosing_fn_name(cx, n).is_some_and(|v| v == "__destruct" || v == "close");
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: try_imports_only(cx, n),
        in_cleanup: cleanup,
    })
}

fn member_source<'s>(cx: &Cx<'_, 's>, expr: Node<'_>, want: &str) -> Option<&'s str> {
    if expr.kind() != "member_access_expression" {
        return None;
    }
    if expr.named_child_count() != 2 {
        return None;
    }
    let (Some(obj_node), Some(mem_node)) = (expr.named_child(0), expr.named_child(1)) else {
        return None;
    };
    (obj_node.kind() == "variable_name" && text(cx, mem_node) == want).then(|| {
        let t = text(cx, obj_node);
        t.strip_prefix('$').unwrap_or(t)
    })
}

fn array_field_source<'s>(cx: &Cx<'_, 's>, elem: Node<'_>) -> Option<&'s str> {
    if elem.named_child_count() != 2 {
        return None;
    }
    let (Some(key_node), Some(val_node)) = (elem.named_child(0), elem.named_child(1)) else {
        return None;
    };
    if key_node.kind() != "string" {
        return None;
    }
    member_source(cx, val_node, strip_quotes(text(cx, key_node)))
}

fn array_copy<'s>(cx: &Cx<'_, 's>, n: Node<'_>, elems: &[Node<'_>]) -> Option<FieldCopy<'s>> {
    if elems.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    for elem in elems {
        let found = array_field_source(cx, *elem)?;
        if source.is_some_and(|s| s != found) {
            return None;
        }
        source = Some(found);
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(elems.len()),
        source,
    })
}

fn named_arg<'s, 'c>(cx: &Cx<'_, 's>, arg: Node<'c>) -> Option<(&'s str, Node<'c>)> {
    let want = child_by_field(arg, "name").map(|v| text(cx, v))?;
    let expr = {
        let named = child_by_field(arg, "name").map(|v| v.id());
        rfind_named_child(arg, |c| {
            Some(c.id()) != named && c.kind() != "reference_modifier"
        })
        .unwrap_or(arg)
    };
    Some((want, expr))
}

pub(crate) fn struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() == "array_creation_expression" {
        let elems: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() == "array_element_initializer")
            .collect();
        return array_copy(cx, n, &elems);
    }
    if n.kind() != "object_creation_expression" {
        return None;
    }
    let args = creation_args(n);
    if args.is_empty() {
        return None;
    }
    let fields = narrow(args.len());
    let mut source: Option<&'s str> = None;
    for arg in args {
        let (want, expr) = named_arg(cx, arg)?;
        let found = member_source(cx, expr, want)?;
        if source.is_some_and(|s| s != found) {
            return None;
        }
        source = Some(found);
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields,
        source,
    })
}

fn is_env_superglobal(t: &str) -> bool {
    matches!(
        t,
        "$_ENV" | "$_SERVER" | "$_GET" | "$_POST" | "$_COOKIE" | "$_REQUEST" | "$_FILES"
    )
}

fn first_var_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        if current.kind() == "variable_name" {
            let t = text(cx, current);
            return Some(t.strip_prefix('$').unwrap_or(t));
        }
        stack.extend(named_children(current));
    }
    None
}

fn is_write_target(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() == "update_expression" {
        return child_by_field(parent, "argument").is_some_and(|a| a.id() == n.id());
    }
    if parent.kind() == "assignment_expression"
        || parent.kind() == "augmented_assignment_expression"
        || parent.kind() == "reference_assignment_expression"
    {
        return child_by_field(parent, "left").is_some_and(|l| l.id() == n.id());
    }
    false
}

fn ambient_subscript<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let base = first_named_child(n).map(|b| text(cx, b)).unwrap_or("");
    if is_env_superglobal(base) {
        out.push((AmbientKind::EnvRead, base, pos_of(n)));
    } else if base == "$_SESSION" || base == "$GLOBALS" {
        out.push((AmbientKind::GlobalMutable, base, pos_of(n)));
        if in_function && is_write_target(cx, n) {
            out.push((AmbientKind::GlobalWrite, base, pos_of(n)));
        }
    }
}

fn ambient_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let (callee, _) = call_stmt_callee(cx, n);
    let last = call_callee_last(cx, n, callee);
    if last == "getenv" || last == "apache_getenv" {
        out.push((
            AmbientKind::EnvRead,
            first_arg_text(cx, n).unwrap_or(callee),
            pos_of(n),
        ));
    } else if last == "putenv" {
        out.push((
            AmbientKind::GlobalMutable,
            first_arg_text(cx, n).unwrap_or(callee),
            pos_of(n),
        ));
    }
}

pub(crate) fn ambient_head<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "subscript_expression" => ambient_subscript(cx, n, in_function, out),
        "global_declaration" => {
            if let Some(name) = first_var_name(cx, n) {
                out.push((AmbientKind::GlobalWrite, name, pos_of(n)));
            }
        }
        "function_static_declaration" => {
            if let Some(name) = first_var_name(cx, n) {
                out.push((AmbientKind::GlobalMutable, name, pos_of(n)));
            }
        }
        "property_declaration" => {
            if any_named_child(n, |c| c.kind() == "static_modifier")
                && let Some(name) = first_var_name(cx, n)
            {
                out.push((AmbientKind::GlobalMutable, name, pos_of(n)));
            }
        }
        kind if is_call_kind(kind) => ambient_call(cx, n, out),
        _ => {}
    }
}
