use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::{
    annotation_names, first_code, has_modifier_word, is_comment, modifiers_of, stricter_visibility,
    type_after_colon, type_visibility,
};
use crate::hooks::{
    Cx, FunctionHead, any_named_child, attributed_region, child_by_field, first_of_kind,
    named_children, only_named_child_matching, pos_of, preceding_comments, rfind_named_child, text,
};

use super::COMMENTS;

pub(crate) fn primitive_of(ty: &str) -> PrimitiveKind {
    let stepped = ty.trim().trim_end_matches('?').trim();
    if stepped.contains('<') || stepped.contains(' ') || stepped.contains('(') {
        return PrimitiveKind::Other;
    }
    match stepped.rsplit('.').next().unwrap_or(stepped) {
        "String" => PrimitiveKind::Str,
        "Boolean" => PrimitiveKind::Bool,
        "Int" | "Long" | "Short" | "Byte" | "Char" | "UInt" | "ULong" | "UShort" | "UByte" => {
            PrimitiveKind::Int
        }
        "Float" | "Double" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn has_visibility(cx: &Cx<'_, '_>, n: Node<'_>, words: &[&str]) -> bool {
    modifiers_of(n).is_some_and(|mods| {
        any_named_child(mods, |child| {
            child.kind() == "visibility_modifier" && words.contains(&text(cx, child))
        })
    })
}

fn default_value(n: Node<'_>) -> Option<Node<'_>> {
    let mut sib = n.next_sibling();
    while let Some(s) = sib {
        if s.is_named() {
            return None;
        }
        if s.kind() == "=" {
            let mut value = s.next_sibling();
            while let Some(v) = value {
                if v.is_named() {
                    return (!is_comment(v)).then_some(v);
                }
                value = v.next_sibling();
            }
            return None;
        }
        sib = s.next_sibling();
    }
    None
}

fn is_variadic(cx: &Cx<'_, '_>, parameter: Node<'_>) -> bool {
    let mut prev = parameter.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "parameter_modifiers" {
            return named_children(p)
                .into_iter()
                .any(|child| child.kind() == "parameter_modifier" && text(cx, child) == "vararg");
        }
        if p.is_named() {
            return false;
        }
        prev = p.prev_sibling();
    }
    false
}

fn build_param<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> Option<Param<'s>> {
    let name = text(cx, first_of_kind(parameter, &["identifier"])?);
    let ty = type_after_colon(parameter).map(|ty| text(cx, ty));
    let default = default_value(parameter);
    Some(Param {
        name,
        ty,
        primitive_kind: primitive_of(ty.unwrap_or("")),
        has_default: default.is_some(),
        bool_default: default.is_some_and(|d| matches!(text(cx, d), "true" | "false")),
        keyword_only: false,
        is_variadic: is_variadic(cx, parameter),
    })
}

pub(crate) fn function_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    if n.kind() == "primary_constructor" {
        return first_of_kind(n, &["class_parameters"]).map_or_else(Vec::new, |list| {
            named_children(list)
                .into_iter()
                .filter(|child| child.kind() == "class_parameter")
                .filter_map(|parameter| build_param(cx, parameter))
                .collect()
        });
    }
    let Some(list) = first_of_kind(n, &["function_value_parameters"]) else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .filter(|child| child.kind() == "parameter")
        .filter_map(|parameter| build_param(cx, parameter))
        .collect()
}

fn lambda_param<'s>(cx: &Cx<'_, 's>, decl: Node<'_>) -> Option<Param<'s>> {
    let name = text(cx, first_of_kind(decl, &["identifier"])?);
    let ty = first_of_kind(decl, &["type"]).map(|ty| text(cx, ty));
    Some(Param {
        name,
        ty,
        primitive_kind: primitive_of(ty.unwrap_or("")),
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    })
}

fn push_lambda_multi<'s>(cx: &Cx<'_, 's>, params: &mut Vec<Param<'s>>, n: Node<'_>) {
    for inner in named_children(n) {
        if inner.kind() == "variable_declaration" {
            params.extend(lambda_param(cx, inner));
        }
    }
}

fn push_lambda_child<'s>(cx: &Cx<'_, 's>, params: &mut Vec<Param<'s>>, child: Node<'_>) {
    if child.kind() == "variable_declaration" {
        params.extend(lambda_param(cx, child));
    } else if child.kind() == "multi_variable_declaration" {
        push_lambda_multi(cx, params, child);
    }
}

fn lambda_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let Some(list) = first_of_kind(n, &["lambda_parameters"]) else {
        return Vec::new();
    };
    let mut params = Vec::new();
    for child in named_children(list) {
        push_lambda_child(cx, &mut params, child);
    }
    params
}

fn lambda_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    let params = lambda_params(cx, n);
    let implicit = params.is_empty();
    FunctionHead {
        shadow_params: implicit.then_some("it").into_iter().collect(),
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: None,
    }
}

fn fun_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    FunctionHead {
        shadow_params: Vec::new(),
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params: function_params(cx, n),
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: None,
    }
}

pub(crate) fn closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    match n.kind() {
        "lambda_literal" => Some(lambda_head(cx, n)),
        "anonymous_function" => Some(fun_head(cx, n)),
        _ => None,
    }
}

fn statement_call(statement: Node<'_>) -> Option<usize> {
    match statement.kind() {
        "call_expression" => Some(statement.id()),
        "return_expression" => rfind_named_child(statement, |child| {
            child.kind() != "line_comment" && child.kind() != "block_comment"
        })
        .filter(|inner| inner.kind() == "call_expression")
        .map(|inner| inner.id()),
        _ => None,
    }
}

fn block_call(block: Node<'_>) -> Option<usize> {
    let only = only_named_child_matching(block, |c| {
        c.kind() != "line_comment" && c.kind() != "block_comment"
    })?;
    statement_call(only)
}

fn sole_call_of(n: Node<'_>) -> Option<usize> {
    if n.kind() == "secondary_constructor" {
        return first_of_kind(n, &["block"]).and_then(block_call);
    }
    let body = first_of_kind(n, &["function_body"])?;
    let inner = first_code(body)?;
    if inner.kind() == "block" {
        block_call(inner)
    } else {
        (inner.kind() == "call_expression").then(|| inner.id())
    }
}

fn expression_body(n: Node<'_>) -> bool {
    first_of_kind(n, &["function_body"])
        .and_then(first_code)
        .is_some_and(|inner| inner.kind() != "block")
}

fn scope_flags(cx: &Cx<'_, '_>) -> (Visibility, bool) {
    let mut cap = Visibility::Exported;
    let mut nested = false;
    for p in cx.ancestors() {
        match p.kind() {
            "object_literal" => cap = Visibility::Private,
            "class_declaration" | "object_declaration" => {
                cap = stricter_visibility(cap, type_visibility(cx, p));
            }
            "function_declaration"
            | "secondary_constructor"
            | "lambda_literal"
            | "anonymous_function"
            | "anonymous_initializer"
            | "block"
                if !nested =>
            {
                nested = true;
            }
            _ => {}
        }
        if cap == Visibility::Private && nested {
            break;
        }
    }
    (cap, nested)
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let (name, name_pos) = match n.kind() {
        "function_declaration" => {
            let name = child_by_field(n, "name")?;
            (text(cx, name), pos_of(name))
        }
        "secondary_constructor" => {
            first_of_kind(n, &["block"])?;
            ("constructor", pos_of(n))
        }
        "primary_constructor" => ("constructor", pos_of(n)),
        _ => return None,
    };
    let is_declaration = n.kind() == "function_declaration";
    let return_type = if is_declaration {
        type_after_colon(n).map(|ty| text(cx, ty))
    } else {
        None
    };
    let mut missing: Vec<&'s str> = Vec::new();
    if is_declaration && return_type.is_none() && expression_body(n) {
        missing.push("return");
    }
    let annotations = annotation_names(cx, n);
    let tested = annotations
        .iter()
        .any(|a| super::TEST_ANNOTATIONS.contains(a));
    let benched = !tested && annotations.contains(&"Benchmark");
    let (enclosing, nested) = scope_flags(cx);
    let own = if has_visibility(cx, n, &["private"]) || nested {
        Visibility::Private
    } else if has_visibility(cx, n, &["internal"]) {
        Visibility::Restricted
    } else {
        Visibility::Exported
    };
    let visibility = stricter_visibility(own, enclosing);
    let in_trait = matches!(
        cx.innermost_container(),
        Some(crate::hooks::Container::Trait { .. })
    );
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENTS, &[]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos,
        visibility,
        signature_forced: has_modifier_word(cx, n, "override"),
        entry_point: name == "main" && cx.parent().is_some_and(|p| p.kind() == "source_file"),
        in_test: tested || benched,
        is_overload: false,
        is_deprecated: annotations.contains(&"Deprecated"),
        is_decorated: !annotations.is_empty(),
        is_trait_item: in_trait,
        region: tested
            .then(|| attributed_region(n, modifiers_of(n), CodeRole::Test))
            .or_else(|| benched.then(|| attributed_region(n, modifiers_of(n), CodeRole::Bench))),
        receiver: None,
        params: function_params(cx, n),
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: sole_call_of(n),
    })
}
