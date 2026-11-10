use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, FunctionHead, any_named_child, child_by_field, first_named_child,
    named_children, only_named_child_matching, pos_of, preceding_comments, region_of, text,
    trimmed,
};
use sloplint_core::role::CodeRole;

pub(crate) fn is_test_path(path: &str) -> bool {
    path.split('/').any(|seg| seg == "test" || seg == "tests")
        || path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .ends_with("Test.php")
}

pub(crate) fn in_test_class<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> bool {
    cx.ancestors()
        .find(|parent| parent.kind() == "class_declaration")
        .is_some_and(|parent| {
            any_named_child(parent, |c| {
                c.kind() == "base_clause"
                    && any_named_child(c, |base| {
                        crate::hooks::last_segment(text(cx, base), &["\\"]) == "TestCase"
                    })
            })
        })
}

pub(crate) fn enclosing_fn_name<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Option<&'s str> {
    cx.ancestors()
        .find(|parent| {
            parent.kind() == "function_definition" || parent.kind() == "method_declaration"
        })
        .and_then(|parent| child_by_field(parent, "name"))
        .map(|v| text(cx, v))
}

pub(crate) fn inside_fn(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    cx.ancestors().any(|parent| {
        matches!(
            parent.kind(),
            "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
        )
    })
}

fn method_vis<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Visibility {
    let hidden = any_named_child(n, |c| {
        c.kind() == "visibility_modifier" && text(cx, c) == "private"
    });
    if hidden {
        Visibility::Private
    } else {
        Visibility::Exported
    }
}

fn primitive_of(ty: Option<&str>) -> PrimitiveKind {
    let Some(raw) = ty else {
        return PrimitiveKind::Other;
    };
    let seg = raw
        .trim()
        .trim_start_matches('?')
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    match seg.as_str() {
        "string" => PrimitiveKind::Str,
        "bool" => PrimitiveKind::Bool,
        "int" | "integer" => PrimitiveKind::Int,
        "float" | "double" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn push_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, missing: &mut Vec<&'s str>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let Some(list) = child_by_field(n, "parameters") else {
        return out;
    };
    for p in named_children(list) {
        let ok = matches!(
            p.kind(),
            "simple_parameter" | "property_promotion_parameter" | "variadic_parameter"
        );
        if !ok {
            continue;
        }
        let raw = child_by_field(p, "name").map(|v| text(cx, v)).unwrap_or("");
        let name = raw.strip_prefix('$').unwrap_or(raw);
        let ty = child_by_field(p, "type").map(|t| text(cx, t));
        let has_default = child_by_field(p, "default_value").is_some();
        let bool_default = child_by_field(p, "default_value")
            .map(|d| trimmed(cx, d))
            .is_some_and(|d| d == "true" || d == "false");
        if ty.is_none() {
            missing.push(name);
        }
        out.push(Param {
            name,
            ty,
            primitive_kind: primitive_of(ty),
            has_default,
            bool_default,
            keyword_only: false,
            is_variadic: p.kind() == "variadic_parameter",
        });
    }
    out
}

pub(crate) fn sole_call(n: Node<'_>) -> Option<usize> {
    let body = child_by_field(n, "body")?;
    if body.kind() != "compound_statement" {
        return None;
    }
    let stmt = only_named_child_matching(body, |c| c.kind() != "comment")?;
    let inner = if stmt.kind() == "expression_statement" || stmt.kind() == "return_statement" {
        first_named_child(stmt)?
    } else {
        stmt
    };
    let kind = inner.kind();
    (kind == "function_call_expression"
        || kind == "member_call_expression"
        || kind == "scoped_call_expression"
        || kind == "nullsafe_member_call_expression"
        || kind == "object_creation_expression")
        .then(|| inner.id())
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let kind = n.kind();
    if kind != "function_definition" && kind != "method_declaration" {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    let mut missing = Vec::new();
    let params = push_params(cx, n, &mut missing);
    if child_by_field(n, "return_type").is_none() && name != "__construct" {
        missing.push("return");
    }
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &["attribute_list"]);
    let tested = in_test_class(cx, n) || is_test_path(cx.path);
    Some(FunctionHead {
        name,
        name_pos: pos_of(name_node),
        visibility: if kind == "method_declaration" {
            method_vis(cx, n)
        } else {
            Visibility::Exported
        },
        signature_forced: false,
        entry_point: false,
        in_test: tested,
        is_overload: false,
        is_deprecated: false,
        is_decorated: any_named_child(n, |c| c.kind() == "attribute_list"),
        is_trait_item: matches!(cx.innermost_container(), Some(Container::Trait { .. })),
        region: tested.then(|| region_of(n, CodeRole::Test)),
        params,
        return_type: child_by_field(n, "return_type").map(|t| text(cx, t)),
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: sole_call(n),
    })
}
