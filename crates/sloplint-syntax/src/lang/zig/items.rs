use sloplint_core::facts::{
    AmbientKind, AssertShape, Assertion, CallKind, FieldCopy, Handler, Pos, ScrutineeKind, Stmt,
    TypeDef, Visibility, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, TypeHead, child_by_field, first_named_child, first_named_child_matching,
    first_of_kind, has_child_kind, last_named_child, last_segment, named_children, narrow, pos_of,
    text, trimmed,
};

use super::calls::builtin_name;
use super::funcs::has_pub;
use super::write_root;

fn case_has_enum(cx: &Cx<'_, '_>, s: Node<'_>) -> bool {
    for i in 0..s.child_count() {
        let Some(child) = s.child(i) else { continue };
        if child.is_named() {
            if child.kind() == "error_type" || child.kind() == "field_expression" {
                return true;
            }
        } else if text(cx, child) == "=>" {
            break;
        }
    }
    false
}

fn case_pattern_text<'s>(cx: &Cx<'_, 's>, s: Node<'_>) -> Option<&'s str> {
    for i in 0..s.child_count() {
        let Some(child) = s.child(i) else { continue };
        if !child.is_named() && text(cx, child) == "=>" {
            break;
        }
        if child.is_named() {
            return Some(text(cx, child));
        }
    }
    None
}

fn sibling_cases<'a>(parent: Node<'a>, skip: Node<'a>) -> Vec<Node<'a>> {
    named_children(parent)
        .into_iter()
        .filter(|s| s.kind() == "switch_case" && s.id() != skip.id())
        .collect()
}

fn sibling_variant_count(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let Some(parent) = cx.parent() else {
        return 0;
    };
    let mut count = 0u32;
    for s in sibling_cases(parent, n) {
        if case_has_enum(cx, s) {
            count = count.saturating_add(1);
        }
    }
    count
}

fn sibling_nullable_ctor(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    for s in sibling_cases(parent, n) {
        let Some(pat) = case_pattern_text(cx, s) else {
            continue;
        };
        let seg = last_segment(pat, &[".", ":"]);
        if matches!(seg, "Some" | "None" | "Ok" | "Err") {
            return true;
        }
    }
    false
}

fn scrutinee_kind(_cx: &Cx<'_, '_>, parent: Node<'_>) -> ScrutineeKind {
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "switch_case" {
            break;
        }
        return match child.kind() {
            "identifier" => ScrutineeKind::Ident,
            "field_expression" => ScrutineeKind::Field,
            _ => ScrutineeKind::Other,
        };
    }
    ScrutineeKind::Other
}

fn field_value_source<'s>(cx: &Cx<'_, 's>, name: Node<'_>, value: Node<'_>) -> Option<&'s str> {
    if value.kind() != "field_expression" {
        return None;
    }
    let obj = child_by_field(value, "object")?;
    if obj.kind() != "identifier" {
        return None;
    }
    let member = child_by_field(value, "member")
        .map(|m| text(cx, m))
        .unwrap_or("");
    (member == text(cx, name)).then_some(text(cx, obj))
}

fn is_eq_compare(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "binary_expression" {
        return false;
    }
    let equal = |c: Node<'_>| text(cx, c) == "==" || text(cx, c) == "!=";
    (0..n.child_count()).any(|i| n.child(i).is_some_and(equal))
}

fn catch_bound<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let payload = first_of_kind(n, &["payload"]);
    payload
        .and_then(|p| first_named_child_matching(p, &["identifier"]))
        .map(|id| text(cx, id))
        .unwrap_or("")
}

fn stmt_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, bound: &str) -> Stmt<'s> {
    let mut current = n;
    if current.kind() == "statement" {
        match first_named_child(current) {
            Some(next) => current = next,
            None => return Stmt::Other,
        }
    }
    if current.kind() == "expression_statement" {
        match first_named_child(current) {
            Some(next) => current = next,
            None => return Stmt::Other,
        }
    }
    if current.kind() == "call_expression" {
        let func = child_by_field(current, "function");
        let callee = func.map(|f| trimmed(cx, f)).unwrap_or("");
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    if current.kind() == "builtin_function" {
        let callee = builtin_name(cx, current);
        return Stmt::Call {
            callee,
            kind: CallKind::Macro,
        };
    }
    if current.kind() == "return_expression" {
        let first = first_named_child(current);
        if first.is_some_and(|a| a.kind() == "identifier" && text(cx, a) == bound) {
            return Stmt::Rethrow;
        }
    }
    Stmt::Other
}

fn is_type_kind(kind: &str) -> bool {
    kind == "struct_declaration"
        || kind == "enum_declaration"
        || kind == "union_declaration"
        || kind == "opaque_declaration"
}

fn is_assert_equal(callee: &str) -> bool {
    callee.ends_with("testing.expectEqual")
        || callee.ends_with("testing.expectEqualStrings")
        || callee.ends_with("testing.expectEqualSlices")
        || callee.ends_with("testing.expectError")
}

fn is_env_callee(callee: &str) -> bool {
    callee.ends_with("process.getEnvVarOwned")
        || callee.ends_with("process.getEnvMap")
        || callee.ends_with("process.hasEnvVar")
        || callee.ends_with("posix.getenv")
        || callee.ends_with("os.getenv")
}

fn is_field_call(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    first_of_kind(n, &["builtin_identifier"]).is_some_and(|id| text(cx, id) == "@field")
}

pub(crate) fn zig_typedef<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
    if !is_type_kind(n.kind()) {
        return None;
    }
    let holder = cx.parent()?;
    if holder.kind() != "variable_declaration" {
        return None;
    }
    let name_node = first_named_child_matching(holder, &["identifier"])?;
    let visibility = if has_pub(holder) {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    let abstraction = n.kind() == "opaque_declaration";
    let name = text(cx, name_node);
    let exported = visibility == Visibility::Exported;
    let class = Container::Class {
        exported,
        implements: false,
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility,
            abstraction,
        }),
        impls: Vec::new(),
        container: Some(class),
    })
}

pub(crate) fn zig_match_arm(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "switch_case" || first_of_kind(n, &["else"]).is_none() {
        return None;
    }
    let variant_count = sibling_variant_count(cx, n);
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 1,
        scrutinee: cx
            .parent()
            .map_or(ScrutineeKind::Other, |p| scrutinee_kind(cx, p)),
        variant_count,
        has_nullable_ctor: sibling_nullable_ctor(cx, n),
    })
}

pub(crate) fn zig_handler<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
    if n.kind() != "catch_expression" {
        return None;
    }
    let body = last_named_child(n)?;
    let bound = catch_bound(cx, n);
    let stmts = if body.kind() == "block" {
        named_children(body)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .map(|s| stmt_for(cx, s, bound))
            .collect()
    } else {
        vec![stmt_for(cx, body, bound)]
    };
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

pub(crate) fn zig_struct_lit<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
    if n.kind() != "struct_initializer" && n.kind() != "anonymous_struct_initializer" {
        return None;
    }
    let list = first_named_child_matching(n, &["initializer_list"])?;
    let fields: Vec<Node<'_>> = named_children(list)
        .into_iter()
        .filter(|c| c.kind() == "field_initializer")
        .collect();
    if fields.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    let mut complete = true;
    for field in &fields {
        let found = match (
            field.named_child(0),
            field.named_child(1),
            field.named_child(2),
        ) {
            (Some(name), Some(value), None) if name.kind() == "identifier" => {
                field_value_source(cx, name, value)
            }
            _ => None,
        };
        match found {
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

fn report_assign_global_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    let Some((root, pos)) = write_root(cx, left) else {
        return;
    };
    if root != "_" && zig_file_vars(cx, n).contains(&root) {
        out.push((AmbientKind::GlobalWrite, root, pos));
    }
}

fn zig_file_vars<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for decl in named_children(root) {
        if decl.kind() != "variable_declaration" || !has_child_kind(decl, "var") {
            continue;
        }
        if let Some(id) = first_named_child_matching(decl, &["identifier"]) {
            let name = text(cx, id);
            if name != "_" {
                names.push(name);
            }
        }
    }
    names
}

pub(crate) fn zig_ambient<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "variable_declaration" {
        if in_function {
            if !has_child_kind(n, "var")
                && !has_child_kind(n, "const")
                && let Some(target) = first_named_child(n)
                && let Some((root, pos)) = write_root(cx, target)
                && root != "_"
                && zig_file_vars(cx, n).contains(&root)
            {
                out.push((AmbientKind::GlobalWrite, root, pos));
            }
            return;
        }
        if !has_child_kind(n, "var") {
            return;
        }
        if let Some(id) = first_named_child_matching(n, &["identifier"]) {
            let name = text(cx, id);
            if name == "_" {
                return;
            }
            let global = (AmbientKind::GlobalMutable, name, pos_of(id));
            out.push(global);
        }
        return;
    }
    if n.kind() == "assignment_expression" {
        if in_function {
            report_assign_global_write(cx, n, out);
        }
        return;
    }
    if n.kind() != "call_expression" {
        return;
    }
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let callee = trimmed(cx, func);
    if is_env_callee(callee) {
        let read = (AmbientKind::EnvRead, callee, pos_of(n));
        out.push(read);
    }
}

pub(crate) fn zig_dynamic(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    if n.kind() != "builtin_function" {
        return None;
    }
    if !is_field_call(cx, n) {
        return None;
    }
    let args = first_named_child_matching(n, &["arguments"])?;
    let second = args.named_child(1)?;
    (second.kind() != "string").then_some("computed field name (@field)")
}

pub(crate) fn zig_assertion(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    let callee = trimmed(cx, func);
    if is_assert_equal(callee) {
        return Some(Assertion {
            pos: pos_of(n),
            has_message: false,
            comparison: false,
            shape: AssertShape::Other,
            in_typecheck: false,
            value_is_bool_literal: false,
        });
    }
    if !callee.ends_with("debug.assert") && !callee.ends_with("testing.expect") {
        return None;
    }
    let first = super::calls::positional_nodes(n, func).first().copied();
    let comparison = first.is_some_and(|a| is_eq_compare(cx, a));
    let shape = if comparison {
        AssertShape::TopEqNe
    } else if first.is_some_and(|a| a.kind() == "identifier") {
        AssertShape::BareName
    } else {
        AssertShape::Other
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message: false,
        comparison,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
