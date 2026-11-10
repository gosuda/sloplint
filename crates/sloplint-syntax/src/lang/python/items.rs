//! Python item hooks: types, match arms, handlers, struct literals, ambient, assertions.

use sloplint_core::facts::{
    AmbientKind, AssertShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Pos,
    ScrutineeKind, Stmt, TypeDef, Visibility, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, TypeHead, child_by_field, first_named_child, first_named_child_matching,
    last_segment, named_children, pos_of, text,
};

use super::{
    class_is_abstract, enclosing_class, enclosing_function, is_doc_expr, parent_dir, py_targets,
};

fn strip_as(value: &str) -> &str {
    let rest = value.trim_start();
    if rest.len() > 2 && rest.starts_with("as") && rest[2..].starts_with(char::is_whitespace) {
        rest[2..].trim_start()
    } else {
        value
    }
}

fn py_raise<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    let Some(first) = s.named_child(0) else {
        return Stmt::Rethrow;
    };
    if s.named_child(1).is_some() {
        return Stmt::Other;
    }
    if first.kind() == "identifier" && text(cx, first) == binding {
        Stmt::Rethrow
    } else {
        Stmt::Other
    }
}

fn raise_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    if s.kind() == "raise_statement" {
        return py_raise(cx, s, binding);
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    let Some(inner) = first_named_child(s) else {
        return Stmt::Other;
    };
    if inner.kind() == "call" {
        let callee = child_by_field(inner, "function").map_or("", |f| text(cx, f));
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    if inner.kind() == "raise_statement" {
        return py_raise(cx, inner, binding);
    }
    Stmt::Other
}

fn arm_pattern(arm: Node<'_>) -> Option<Node<'_>> {
    let pat = first_named_child(arm)?;
    if pat.kind() == "case_pattern" {
        first_named_child(pat)
    } else {
        Some(pat)
    }
}

fn under_type_checking<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> bool {
    for ancestor in cx.ancestors() {
        if ancestor.kind() == "if_statement" {
            let hit = child_by_field(ancestor, "condition")
                .is_some_and(|c| text(cx, c).contains("TYPE_CHECKING"));
            if hit {
                return true;
            }
        }
    }
    false
}

fn is_os_environ_value<'s>(cx: &Cx<'_, 's>, v: Node<'_>) -> bool {
    if v.kind() != "attribute" {
        return false;
    }
    let obj = child_by_field(v, "object").map_or("", |o| text(cx, o));
    let attr = child_by_field(v, "attribute").map_or("", |a| text(cx, a));
    obj == "os" && attr == "environ"
}

fn is_store_target(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "assignment" && parent.kind() != "augmented_assignment" {
        return false;
    }
    child_by_field(parent, "left").is_some_and(|l| l.id() == n.id())
}

fn is_env_write_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    let Some(func) = child_by_field(n, "function") else {
        return false;
    };
    if func.kind() != "attribute" {
        return false;
    }
    let attr = child_by_field(func, "attribute").map_or("", |a| text(cx, a));
    let Some(obj) = child_by_field(func, "object") else {
        return false;
    };
    if attr == "putenv" && obj.kind() == "identifier" && text(cx, obj) == "os" {
        return true;
    }
    is_os_environ_value(cx, obj) && matches!(attr, "update" | "setdefault" | "pop" | "clear")
}

pub(crate) fn push_typedef<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
    if n.kind() != "class_definition" {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    let visibility = if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Exported
    };
    let abstraction = class_is_abstract(cx, n);
    let mut impls = Vec::new();
    if let Some(sup) = child_by_field(n, "superclasses") {
        for base in named_children(sup) {
            if base.kind() == "keyword_argument" {
                continue;
            }
            let raw = if base.kind() == "subscript" {
                child_by_field(base, "value").map_or("", |v| text(cx, v))
            } else {
                text(cx, base)
            };
            let seg = last_segment(raw, &["."]);
            if seg == "object" || seg == "Generic" {
                continue;
            }
            if seg == "Protocol" || seg == "ABC" {
                continue;
            }
            impls.push(ImplRef {
                name: seg,
                pos: pos_of(base),
                is_generic: base.kind() == "subscript",
                child_is_abstract: abstraction,
            });
        }
    }
    let exported = visibility == Visibility::Exported;
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility,
            abstraction,
        }),
        impls,
        container: Some(Container::Class {
            exported,
            implements: false,
        }),
    })
}

pub(crate) fn push_match_arm(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "case_clause" {
        return None;
    }
    if child_by_field(n, "guard").is_some() {
        return None;
    }
    let pat = first_named_child(n)?;
    if text(cx, pat) != "_" {
        return None;
    }
    let parent = cx.parent()?;
    let scrutinee = match child_by_field(parent, "subject").map(|s| s.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("attribute") => ScrutineeKind::Field,
        Some("dotted_name") => ScrutineeKind::Path,
        _ => ScrutineeKind::Other,
    };
    let mut variant_count = 0u32;
    let mut has_nullable_ctor = false;
    if let Some(body) = child_by_field(parent, "body") {
        for arm in named_children(body) {
            if arm.kind() != "case_clause" || arm.id() == n.id() {
                continue;
            }
            let Some(p) = arm_pattern(arm) else {
                continue;
            };
            if p.kind() == "class_pattern" || p.kind() == "dotted_name" {
                variant_count = variant_count.saturating_add(1);
            }
            let seg = last_segment(text(cx, p), &["."]);
            if matches!(seg, "Some" | "None" | "Ok" | "Err") {
                has_nullable_ctor = true;
            }
        }
    }
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 1,
        scrutinee,
        variant_count,
        has_nullable_ctor,
    })
}

pub(crate) fn push_handler<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
    if n.kind() != "except_clause" {
        return None;
    }
    let binding = child_by_field(n, "alias").map_or("", |a| strip_as(text(cx, a)));
    let block = first_named_child_matching(n, &["block"])?;
    let mut stmts = Vec::new();
    for s in named_children(block) {
        if s.kind() == "pass_statement" {
            continue;
        }
        if s.kind() == "expression_statement" {
            let inner = first_named_child(s);
            if inner.is_some_and(|v| v.kind() == "ellipsis") {
                continue;
            }
        }
        stmts.push(raise_stmt(cx, s, binding));
    }
    let parent_try = cx.parent().filter(|p| p.kind() == "try_statement");
    let has_else =
        parent_try.is_some_and(|t| named_children(t).iter().any(|c| c.kind() == "else_clause"));
    let try_imports_only = parent_try.is_some_and(|t| {
        child_by_field(t, "body").is_some_and(|b| {
            let kids = named_children(b);
            !kids.is_empty()
                && kids.iter().all(|k| {
                    matches!(
                        k.kind(),
                        "import_statement" | "import_from_statement" | "future_import_statement"
                    )
                })
        })
    });
    let in_cleanup = enclosing_function(cx).is_some_and(|f| {
        child_by_field(f, "name")
            .is_some_and(|v| matches!(text(cx, v), "__del__" | "close" | "__exit__"))
    });
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else,
        try_imports_only,
        in_cleanup,
    })
}

pub(crate) fn push_struct_lit<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func.kind() != "identifier" {
        return None;
    }
    let callee = text(cx, func);
    if callee == "dict" || callee == "OrderedDict" {
        return None;
    }
    let list = child_by_field(n, "arguments")?;
    let args = named_children(list);
    if args.is_empty() {
        return None;
    }
    let mut fields = 0u32;
    let mut source: Option<&'s str> = None;
    let mut poisoned = false;
    for a in &args {
        if a.kind() != "keyword_argument" {
            poisoned = true;
            continue;
        }
        fields += 1;
        let want = child_by_field(*a, "name").map_or("", |x| text(cx, x));
        let cand = child_by_field(*a, "value").and_then(|v| {
            if v.kind() != "attribute" {
                return None;
            }
            let obj = child_by_field(v, "object")?;
            if obj.kind() != "identifier" {
                return None;
            }
            if child_by_field(v, "attribute").map_or("", |x| text(cx, x)) != want {
                return None;
            }
            Some(text(cx, obj))
        });
        match (source, cand) {
            (None, Some(s)) => source = Some(s),
            (Some(a), Some(b)) if a == b => {}
            _ => poisoned = true,
        }
    }
    if fields == 0 {
        return None;
    }
    if !poisoned && let Some(src) = source {
        let same_type = (src == "self" || src == "cls")
            && enclosing_class(cx)
                .is_some_and(|c| child_by_field(c, "name").is_some_and(|v| text(cx, v) == callee));
        if same_type {
            poisoned = true;
        }
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields,
        source: if poisoned { None } else { source },
    })
}

fn is_module_assignment(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    let mut ancestors = cx.ancestors();
    match ancestors.next() {
        Some(parent) if parent.kind() == "module" => true,
        Some(parent) if parent.kind() == "expression_statement" => {
            ancestors.next().is_some_and(|g| g.kind() == "module")
        }
        _ => false,
    }
}

fn push_module_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    match n.kind() {
        "identifier" => {
            let name = text(cx, n);
            if name != "_" {
                out.push((AmbientKind::GlobalMutable, name, pos_of(n)));
            }
        }
        "tuple_pattern" | "pattern_list" | "list_pattern" | "tuple" | "list" => {
            for child in named_children(n) {
                push_module_names(cx, child, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn push_ambient<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "assignment" && !in_function && is_module_assignment(cx, n) {
        if let Some(left) = child_by_field(n, "left") {
            push_module_names(cx, left, out);
        }
        return;
    }
    if n.kind() == "global_statement" {
        if !in_function {
            return;
        }
        for c in named_children(n) {
            if c.kind() == "identifier" {
                out.push((AmbientKind::GlobalWrite, text(cx, c), pos_of(c)));
            }
        }
        return;
    }
    if n.kind() == "call" {
        if is_env_write_call(cx, n) {
            out.push((AmbientKind::GlobalWrite, "os.environ", pos_of(n)));
            return;
        }
        let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
        if callee == "os.getenv" || callee == "getenv" {
            out.push((AmbientKind::EnvRead, "os.getenv", pos_of(n)));
        }
        return;
    }
    if n.kind() == "attribute" {
        let obj = child_by_field(n, "object").map_or("", |o| text(cx, o));
        let attr = child_by_field(n, "attribute").map_or("", |a| text(cx, a));
        if obj == "os" && attr == "environ" {
            let mut ancestors = cx.ancestors();
            let stored_above = ancestors.next().is_some_and(|p| {
                p.kind() == "subscript"
                    && child_by_field(p, "value").is_some_and(|v| v.id() == n.id())
                    && ancestors.next().is_some_and(|g| {
                        (g.kind() == "assignment" || g.kind() == "augmented_assignment")
                            && child_by_field(g, "left").is_some_and(|l| l.id() == p.id())
                    })
            });
            if !stored_above {
                out.push((AmbientKind::EnvRead, "os.environ", pos_of(n)));
            }
        }
        return;
    }
    if n.kind() != "subscript" {
        return;
    }
    let Some(v) = child_by_field(n, "value") else {
        return;
    };
    if is_os_environ_value(cx, v) && is_store_target(cx, n) {
        out.push((AmbientKind::GlobalWrite, "os.environ", pos_of(n)));
    }
}

pub(crate) fn push_assertion(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
    if n.kind() != "assert_statement" {
        return None;
    }
    let cond = first_named_child(n)?;
    let shape = if cond.kind() == "identifier" || cond.kind() == "attribute" {
        AssertShape::BareName
    } else if cond.kind() == "comparison_operator" {
        let op = text(cx, cond);
        if op.contains("==") || op.contains("!=") {
            AssertShape::TopEqNe
        } else {
            AssertShape::Other
        }
    } else {
        AssertShape::Other
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message: n.named_child_count() >= 2,
        comparison: cond.kind() == "comparison_operator",
        shape,
        in_typecheck: under_type_checking(cx, n),
        value_is_bool_literal: false,
    })
}

pub(crate) fn push_barrel(root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
    if !cx.path.ends_with("__init__.py") {
        return None;
    }
    if root.kind() != "module" {
        return None;
    }
    let mut found_import = false;
    for child in named_children(root) {
        match child.kind() {
            "comment" => {}
            "import_from_statement" => found_import = true,
            "expression_statement" => {
                if !is_doc_expr(child) {
                    return None;
                }
            }
            "assignment" => {
                if child_by_field(child, "left").map_or("", |l| text(cx, l)) != "__all__" {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if !found_import {
        return None;
    }
    let dir = parent_dir(cx.path);
    let mut targets = Vec::new();
    for child in named_children(root) {
        if child.kind() != "import_from_statement" {
            continue;
        }
        targets.push(py_targets(cx, dir, child));
    }
    Some(Barrel { targets })
}
