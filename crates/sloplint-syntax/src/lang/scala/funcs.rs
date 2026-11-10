use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, FunctionHead, child_by_field, first_named_child, first_named_child_matching,
    has_child_kind, named_children, pos_of, preceding_comments, text, trimmed,
};
pub(crate) fn cut_at(s: &str, ch: char) -> &str {
    match s.find(ch) {
        Some(i) => &s[..i],
        None => s,
    }
}

pub(crate) fn seg_last(t: &str) -> &str {
    let base = cut_at(cut_at(t, '<'), '[');
    let bytes = base.as_bytes();
    let mut end = base.len();
    while end > 0 && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    &base[start..end]
}

pub(crate) fn is_suite_base(base: &str) -> bool {
    let base = base.strip_prefix("Fixture").unwrap_or(base);
    let base = base.strip_prefix("Async").unwrap_or(base);
    if matches!(
        base,
        "Suite"
            | "Suites"
            | "FunSuite"
            | "CatsEffectSuite"
            | "Specification"
            | "ZIOSpecDefault"
            | "Properties"
    ) {
        return true;
    }
    base.starts_with("Any") && (base.ends_with("Suite") || base.ends_with("Spec"))
}

pub(crate) fn in_test_dir(path: &str) -> bool {
    path.split('/').any(|seg| seg == "test" || seg == "tests")
}

pub(crate) fn primitive_of(ty: Option<&str>) -> PrimitiveKind {
    let Some(raw) = ty else {
        return PrimitiveKind::Other;
    };
    match seg_last(raw.trim()) {
        "String" => PrimitiveKind::Str,
        "Boolean" => PrimitiveKind::Bool,
        "Int" | "Long" | "Short" | "Byte" => PrimitiveKind::Int,
        "Float" | "Double" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn modifiers_vis<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Visibility> {
    let mods = first_named_child_matching(n, &["modifiers"])?;
    let acc = named_children(mods)
        .into_iter()
        .find(|c| c.kind() == "access_modifier")?;
    let t = text(cx, acc);
    if t.contains('[') {
        Some(Visibility::Restricted)
    } else if t.starts_with("protected") {
        Some(Visibility::Exported)
    } else {
        Some(Visibility::Private)
    }
}

fn outer_vis(cx: &Cx<'_, '_>) -> Option<Visibility> {
    match cx.innermost_container() {
        Some(Container::Class { exported, .. }) if !exported => Some(Visibility::Private),
        Some(Container::Trait { public }) if !public => Some(Visibility::Private),
        _ => None,
    }
}

pub(crate) fn is_nested_fn(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    cx.ancestors().any(|parent| {
        matches!(
            parent.kind(),
            "function_definition"
                | "lambda_expression"
                | "block"
                | "indented_block"
                | "case_clause"
        )
    })
}

fn sole_call_of(body: Node<'_>) -> Option<usize> {
    if body.kind() == "call_expression" {
        return Some(body.id());
    }
    if body.kind() != "block" && body.kind() != "indented_block" {
        return None;
    }
    let mut sole = None;
    let mut count = 0u32;
    for c in named_children(body) {
        if c.kind() == "comment" || c.kind() == "block_comment" {
            continue;
        }
        count += 1;
        sole = Some(c);
    }
    let s = sole.filter(|_| count == 1)?;
    let mut target = s;
    if target.kind() == "return_expression" {
        target = first_named_child(target)?;
    }
    (target.kind() == "call_expression").then_some(target.id())
}

pub(crate) fn params_from<'s>(cx: &Cx<'_, 's>, c: Node<'_>, out: &mut Vec<Param<'s>>) {
    for p in named_children(c) {
        if p.kind() != "parameter" {
            continue;
        }
        let Some(nm) = child_by_field(p, "name") else {
            continue;
        };
        let ty = child_by_field(p, "type").map(|t| text(cx, t));
        let has_default = child_by_field(p, "default_value").is_some();
        let bool_default = child_by_field(p, "default_value").is_some_and(|d| {
            d.kind() == "boolean_literal" && (trimmed(cx, d) == "true" || trimmed(cx, d) == "false")
        });
        out.push(Param {
            name: text(cx, nm),
            ty,
            primitive_kind: primitive_of(ty),
            has_default,
            bool_default,
            keyword_only: false,
            is_variadic: ty.is_some_and(|t| t.trim_end().ends_with('*')),
        });
    }
}

fn is_entry_point<'s>(cx: &Cx<'_, 's>, n: Node<'_>, name: &'s str, params: &[Param<'s>]) -> bool {
    if name != "main" || params.len() != 1 {
        return false;
    }
    if !params[0]
        .ty
        .is_some_and(|t| t.replace(' ', "").contains("Array[String]"))
    {
        return false;
    }
    if is_nested_fn(cx, n) {
        return false;
    }
    match cx.innermost_container() {
        None => true,
        Some(Container::Class { .. }) => cx
            .ancestors()
            .find_map(|a| match a.kind() {
                "object_definition" => Some(true),
                "class_definition" | "trait_definition" | "enum_definition" => Some(false),
                _ => None,
            })
            .unwrap_or(false),
        _ => false,
    }
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() != "function_definition" {
        return None;
    }
    let body = child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    let mut params = Vec::new();
    for c in named_children(n) {
        if c.kind() == "parameters" {
            params_from(cx, c, &mut params);
        }
    }
    let return_type = child_by_field(n, "return_type").map(|t| text(cx, t));
    let mut missing: Vec<&'s str> = params
        .iter()
        .filter(|p| p.ty.is_none())
        .map(|p| p.name)
        .collect();
    if return_type.is_none() && name != "this" {
        missing.push("return");
    }
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment", "block_comment"], &[]);
    let vis = modifiers_vis(cx, n).unwrap_or(Visibility::Exported);
    let vis = if is_nested_fn(cx, n) {
        Visibility::Private
    } else {
        vis
    };
    let vis = outer_vis(cx).unwrap_or(vis);
    let forced = first_named_child_matching(n, &["modifiers"])
        .is_some_and(|m| has_child_kind(m, "override"));
    let entry = is_entry_point(cx, n, name, &params);
    Some(FunctionHead {
        shadow_params: params.iter().map(|p| p.name).collect(),
        name,
        name_pos: pos_of(name_node),
        visibility: vis,
        signature_forced: forced,
        entry_point: entry,
        in_test: cx.inside_test_module(),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: matches!(cx.innermost_container(), Some(Container::Trait { .. })),
        region: None,
        receiver: None,
        params,
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: sole_call_of(body),
    })
}

fn clause_is_suite<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> bool {
    named_children(c)
        .iter()
        .any(|t| t.kind() != "arguments" && is_suite_base(seg_last(trimmed(cx, *t))))
}

fn suite_body(cx: &Cx<'_, '_>, _body: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "class_definition" && parent.kind() != "object_definition" {
        return false;
    }
    if in_test_dir(cx.path) {
        return true;
    }
    named_children(parent)
        .iter()
        .any(|c| c.kind() == "extends_clause" && clause_is_suite(cx, *c))
}

pub(crate) fn module_region(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
    if n.kind() != "template_body" {
        return None;
    }
    suite_body(cx, n).then_some(CodeRole::Test)
}
