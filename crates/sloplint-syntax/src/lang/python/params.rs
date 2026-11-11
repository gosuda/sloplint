//! Python parameter, decorator, and function-head flag classification.

use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, last_segment, named_children, text};

use super::{Placement, class_is_abstract, enclosing_class, is_test_path};

pub(crate) struct FunctionFlags {
    pub visibility: Visibility,
    pub signature_forced: bool,
    pub entry_point: bool,
    pub in_test: bool,
    pub is_overload: bool,
    pub is_deprecated: bool,
    pub is_decorated: bool,
    pub is_trait_item: bool,
}

pub(crate) fn primitive_of(ty: Option<&str>) -> PrimitiveKind {
    let Some(raw) = ty else {
        return PrimitiveKind::Other;
    };
    let seg = last_segment(raw.trim(), &["."]).trim();
    if seg.contains('[') || seg.contains('(') || seg.contains(' ') {
        return PrimitiveKind::Other;
    }
    match seg {
        "str" => PrimitiveKind::Str,
        "bool" => PrimitiveKind::Bool,
        "int" => PrimitiveKind::Int,
        "float" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn decorator_text<'s>(cx: &Cx<'_, 's>, d: Node<'_>) -> &'s str {
    let raw = text(cx, d).trim_start_matches('@');
    match raw.find('(') {
        Some(i) => &raw[..i],
        None => raw,
    }
}

fn has_decorator(cx: &Cx<'_, '_>, _n: Node<'_>, want: &str) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "decorated_definition" {
        return false;
    }
    named_children(parent)
        .iter()
        .any(|d| d.kind() == "decorator" && last_segment(decorator_text(cx, *d), &["."]) == want)
}

fn has_any_decorator(cx: &Cx<'_, '_>) -> bool {
    cx.parent().is_some_and(|p| {
        p.kind() == "decorated_definition"
            && named_children(p).iter().any(|d| d.kind() == "decorator")
    })
}

fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

pub(crate) fn function_flags(
    cx: &Cx<'_, '_>,
    n: Node<'_>,
    name: &str,
    place: &Placement,
) -> FunctionFlags {
    let dunder = is_dunder(name);
    let exported_here = cx.nearest_class_exported().unwrap_or(true);
    let visible = !place.local
        && (!name.starts_with('_') || dunder)
        && (!place.class_member || exported_here);
    let visibility = if visible {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    let test_marked = is_test_path(cx.path);
    FunctionFlags {
        visibility,
        signature_forced: dunder,
        entry_point: name == "main" && !place.local && !place.class_member,
        in_test: test_marked,
        is_overload: has_decorator(cx, n, "overload"),
        is_deprecated: has_decorator(cx, n, "deprecated"),
        is_decorated: has_any_decorator(cx),
        is_trait_item: enclosing_class(cx).is_some_and(|c| class_is_abstract(cx, c)),
    }
}

fn splat_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    if n.kind() == "identifier" {
        return text(cx, n);
    }
    for c in named_children(n) {
        if c.kind() == "identifier" {
            return text(cx, c);
        }
    }
    ""
}

fn push_plain_typed<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    keyword_only: bool,
    out: &mut Vec<Param<'s>>,
) {
    let name = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "identifier")
        .map_or("", |c| text(cx, c));
    let ty = child_by_field(n, "type").map(|t| text(cx, t));
    out.push(Param {
        name,
        ty,
        primitive_kind: primitive_of(ty),
        has_default: false,
        bool_default: false,
        keyword_only,
        is_variadic: false,
    });
}

fn push_splat_typed<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    pat: Node<'_>,
    keyword_only: bool,
    out: &mut Vec<Param<'s>>,
) {
    let ty = child_by_field(n, "type").map(|t| text(cx, t));
    out.push(Param {
        name: splat_name(cx, pat),
        ty,
        primitive_kind: primitive_of(ty),
        has_default: false,
        bool_default: false,
        keyword_only,
        is_variadic: true,
    });
}

fn push_typed<'s>(cx: &Cx<'_, 's>, n: Node<'_>, keyword_only: bool, out: &mut Vec<Param<'s>>) {
    let mut inner = None;
    for c in named_children(n) {
        if c.kind() == "list_splat_pattern" || c.kind() == "dictionary_splat_pattern" {
            inner = Some(c);
        }
    }
    match inner {
        Some(pat) => push_splat_typed(cx, n, pat, keyword_only, out),
        None => push_plain_typed(cx, n, keyword_only, out),
    }
}

fn push_untyped_default<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    keyword_only: bool,
    out: &mut Vec<Param<'s>>,
) {
    let name = child_by_field(n, "name").map_or("", |v| text(cx, v));
    let bool_default =
        child_by_field(n, "value").is_some_and(|v| matches!(v.kind(), "true" | "false"));
    out.push(Param {
        name,
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: true,
        bool_default,
        keyword_only,
        is_variadic: false,
    });
}

fn push_typed_default<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    keyword_only: bool,
    out: &mut Vec<Param<'s>>,
) {
    let name = child_by_field(n, "name").map_or("", |v| text(cx, v));
    let ty = child_by_field(n, "type").map(|t| text(cx, t));
    let bool_default =
        child_by_field(n, "value").is_some_and(|v| matches!(v.kind(), "true" | "false"));
    out.push(Param {
        name,
        ty,
        primitive_kind: primitive_of(ty),
        has_default: true,
        bool_default,
        keyword_only,
        is_variadic: false,
    });
}

fn push_wrapped<'s>(
    cx: &Cx<'_, 's>,
    child: Node<'_>,
    keyword_only: bool,
    out: &mut Vec<Param<'s>>,
) -> bool {
    let mut rest_only = keyword_only;
    for sub in named_children(child) {
        push_one(cx, sub, rest_only, out);
        if sub.kind() == "list_splat_pattern" {
            rest_only = true;
        }
    }
    rest_only
}

pub(crate) fn push_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    let mut keyword_only = false;
    for child in named_children(list) {
        if child.kind() == "keyword_separator" {
            keyword_only = true;
            continue;
        }
        if child.kind() == "positional_separator" {
            keyword_only = false;
            continue;
        }
        if child.kind() == "parameter" {
            keyword_only = push_wrapped(cx, child, keyword_only, out);
            continue;
        }
        push_one(cx, child, keyword_only, out);
        if child.kind() == "list_splat_pattern" || wraps_splat(child) {
            keyword_only = true;
        }
    }
}

fn wraps_splat(child: Node<'_>) -> bool {
    if child.kind() != "typed_parameter" {
        return false;
    }
    named_children(child)
        .iter()
        .any(|c| c.kind() == "list_splat_pattern" || c.kind() == "dictionary_splat_pattern")
}

fn push_one<'s>(cx: &Cx<'_, 's>, n: Node<'_>, keyword_only: bool, out: &mut Vec<Param<'s>>) {
    match n.kind() {
        "identifier" => out.push(Param {
            name: text(cx, n),
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only,
            is_variadic: false,
        }),
        "typed_parameter" => push_typed(cx, n, keyword_only, out),
        "default_parameter" => push_untyped_default(cx, n, keyword_only, out),
        "typed_default_parameter" => push_typed_default(cx, n, keyword_only, out),
        "list_splat_pattern" | "dictionary_splat_pattern" => out.push(Param {
            name: splat_name(cx, n),
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only,
            is_variadic: true,
        }),
        _ => {}
    }
}
