//! Go function, closure, parameter, and type heads.

use sloplint_core::facts::{Param, PrimitiveKind, TypeDef, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use crate::hooks::{
    Cx, FunctionHead, TypeHead, child_by_field, first_named_child, first_named_child_matching,
    named_children, pos_of, preceding_comments, region_of, text,
};

pub(crate) fn go_is_test_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .ends_with("_test.go")
}

pub(crate) fn go_visibility(name: &str) -> Visibility {
    if name.chars().next().is_some_and(|c| c.is_uppercase()) {
        Visibility::Exported
    } else {
        Visibility::Private
    }
}

pub(crate) fn go_primitive_kind(ty: Option<&str>) -> PrimitiveKind {
    let ty = ty.map_or("", |v| v.strip_prefix("...").unwrap_or(v));
    match ty {
        "string" => PrimitiveKind::Str,
        "bool" => PrimitiveKind::Bool,
        "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8" | "uint16" | "uint32"
        | "uint64" | "uintptr" | "byte" | "rune" => PrimitiveKind::Int,
        "float32" | "float64" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn variadic_ty<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let ty = text(cx, n);
    ty.find("...").map_or("", |i| &ty[i..])
}

fn go_named_param<'s>(
    cx: &Cx<'_, 's>,
    child: Node<'_>,
    ty: Option<&'s str>,
    primitive_kind: PrimitiveKind,
    out: &mut Vec<Param<'s>>,
) {
    let mut any = false;
    for name in named_children(child) {
        if name.kind() != "identifier" {
            continue;
        }
        any = true;
        out.push(Param {
            name: text(cx, name),
            ty,
            primitive_kind,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        });
    }
    if !any {
        out.push(Param {
            name: "",
            ty,
            primitive_kind,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        });
    }
}

pub(crate) fn go_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        if child.kind() == "parameter_declaration" {
            let ty = child_by_field(child, "type").map(|t| text(cx, t));
            go_named_param(cx, child, ty, go_primitive_kind(ty), out);
        } else if child.kind() == "variadic_parameter_declaration" {
            let ty = Some(variadic_ty(cx, child));
            out.push(Param {
                name: child_by_field(child, "name").map_or("", |v| text(cx, v)),
                ty,
                primitive_kind: go_primitive_kind(ty),
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: true,
            });
        }
    }
}

pub(crate) fn go_package_is_main(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    let Some(root) = cx.root() else {
        return false;
    };
    for child in named_children(root) {
        if child.kind() == "package_clause" {
            return first_named_child(child).is_some_and(|c| text(cx, c) == "main");
        }
    }
    false
}

fn go_name_ok(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix).is_some_and(|rest| {
        rest.is_empty() || rest.chars().next().is_some_and(|c| !c.is_lowercase())
    })
}

fn go_sig_ok(params: &[Param<'_>], want: &str) -> bool {
    params.len() == 1 && params[0].ty.is_some_and(|ty| ty.trim() == want)
}

pub(crate) fn go_marker(name: &str, params: &[Param<'_>], path: &str) -> Option<CodeRole> {
    if !go_is_test_file(path) {
        return None;
    }
    if go_name_ok(name, "Test") && go_sig_ok(params, "*testing.T") {
        return Some(CodeRole::Test);
    }
    if go_name_ok(name, "Benchmark") && go_sig_ok(params, "*testing.B") {
        return Some(CodeRole::Bench);
    }
    if go_name_ok(name, "Fuzz") && go_sig_ok(params, "*testing.F") {
        return Some(CodeRole::Bench);
    }
    if go_name_ok(name, "Example") && params.is_empty() {
        return Some(CodeRole::Example);
    }
    None
}

fn go_return_expr(n: Node<'_>) -> Option<Node<'_>> {
    let only = n.named_child(0)?;
    if n.named_child(1).is_some() {
        return None;
    }
    if only.kind() == "expression_list" {
        let one = only.named_child(0)?;
        if only.named_child(1).is_some() {
            return None;
        }
        Some(one)
    } else {
        Some(only)
    }
}

fn go_single_call(n: Node<'_>) -> Option<usize> {
    match n.kind() {
        "expression_statement" => {
            let inner = first_named_child(n)?;
            (inner.kind() == "call_expression").then_some(inner.id())
        }
        "return_statement" => {
            let expr = go_return_expr(n)?;
            (expr.kind() == "call_expression").then_some(expr.id())
        }
        _ => None,
    }
}

pub(crate) fn go_sole(body: Node<'_>) -> Option<usize> {
    let list = first_named_child_matching(body, &["statement_list"])?;
    let only = list.named_child(0)?;
    if list.named_child(1).is_some() {
        return None;
    }
    go_single_call(only)
}

pub(crate) fn go_function_head<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    name_node: Node<'_>,
    params: Vec<Param<'s>>,
    marker: Option<CodeRole>,
) -> FunctionHead<'s> {
    let name = text(cx, name_node);
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    FunctionHead {
        name,
        name_pos: pos_of(name_node),
        visibility: go_visibility(name),
        signature_forced: false,
        entry_point: n.kind() == "function_declaration"
            && name == "main"
            && go_package_is_main(cx, n),
        in_test: go_is_test_file(cx.path) || marker.is_some(),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: marker.map(|role| region_of(n, role)),
        params,
        return_type: child_by_field(n, "result").map(|t| text(cx, t)),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: child_by_field(n, "body").and_then(go_sole),
    }
}

pub(crate) fn go_closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    let mut params = Vec::new();
    if let Some(list) = child_by_field(n, "parameters") {
        go_params(cx, list, &mut params);
    }
    FunctionHead {
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: go_is_test_file(cx.path),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        return_type: child_by_field(n, "result").map(|t| text(cx, t)),
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: child_by_field(n, "body").and_then(go_sole),
    }
}

pub(crate) fn go_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if n.kind() != "type_spec" {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility: go_visibility(name),
            abstraction: child_by_field(n, "type").is_some_and(|t| t.kind() == "interface_type"),
        }),
        impls: Vec::new(),
        container: None,
    })
}
