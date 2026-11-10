use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use sloplint_core::role::{CodeRole, RoleRegion};
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, FunctionHead, any_named_child, child_by_field, first_named_child_matching,
    named_children, only_named_child_matching, pos_of, preceding_comments, region_of, text,
};

use super::syntax::{COMMENTS, annotation_name, has_anon, signature_of};

pub(crate) fn dart_primitive(ty: Option<&str>) -> PrimitiveKind {
    let Some(raw) = ty else {
        return PrimitiveKind::Other;
    };
    match raw.split(['<', '?', ' ']).next().unwrap_or("").trim() {
        "String" => PrimitiveKind::Str,
        "bool" => PrimitiveKind::Bool,
        "int" => PrimitiveKind::Int,
        "double" | "num" | "float" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

struct ParamCell<'n> {
    node: Node<'n>,
    default: Option<Node<'n>>,
    named: bool,
}

fn param_from<'s>(cx: &Cx<'_, 's>, cell: ParamCell<'_>, out: &mut Vec<Param<'s>>) {
    let Some(nm) = child_by_field(cell.node, "name") else {
        return;
    };
    let ty = first_named_child_matching(cell.node, &["type"]).map(|t| text(cx, t));
    let has_default = cell.default.is_some_and(|nx| {
        !matches!(
            nx.kind(),
            "formal_parameter"
                | "optional_formal_parameters"
                | "annotation"
                | "comment"
                | "block_comment"
                | "documentation_block_comment"
        )
    });
    out.push(Param {
        name: text(cx, nm),
        ty,
        primitive_kind: dart_primitive(ty),
        has_default,
        bool_default: cell
            .default
            .is_some_and(|nx| nx.kind() == "true" || nx.kind() == "false"),
        keyword_only: cell.named,
        is_variadic: false,
    });
}

fn params_from_list<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    let mut frames: Vec<(Node<'_>, u32, bool)> = vec![(list, 0, false)];
    while let Some((parent, index, named)) = frames.last().copied() {
        let Some(kid) = parent.named_child(index) else {
            frames.pop();
            continue;
        };
        if let Some(frame) = frames.last_mut() {
            frame.1 = index.saturating_add(1);
        }
        if kid.kind() == "formal_parameter" {
            let mut following = index.saturating_add(1);
            while parent
                .named_child(following)
                .is_some_and(|nx| COMMENTS.contains(&nx.kind()))
            {
                following = following.saturating_add(1);
            }
            param_from(
                cx,
                ParamCell {
                    node: kid,
                    default: parent.named_child(following),
                    named,
                },
                out,
            );
        } else if kid.kind() == "optional_formal_parameters" {
            frames.push((kid, 0, has_anon(kid, "{")));
        }
    }
}

fn sole_call_of(body: Node<'_>) -> Option<usize> {
    let only = only_named_child_matching(body, |c| {
        c.kind() != "comment" && c.kind() != "block_comment"
    })?;
    if only.kind() == "call_expression" {
        return Some(only.id());
    }
    if only.kind() != "block" {
        return None;
    }
    let stmt = only_named_child_matching(only, |c| {
        c.kind() != "comment" && c.kind() != "block_comment"
    })?;
    if stmt.kind() != "return_statement" && stmt.kind() != "expression_statement" {
        return None;
    }
    first_named_child_matching(stmt, &["call_expression"]).map(|c| c.id())
}

fn is_fn_kind(kind: &str) -> bool {
    kind == "function_declaration"
        || kind == "method_declaration"
        || kind == "local_function_declaration"
}

fn fn_parts(n: Node<'_>) -> Option<(Node<'_>, Node<'_>, Node<'_>)> {
    let sig = signature_of(n)?;
    let name_node = child_by_field(sig, "name")?;
    let body = child_by_field(n, "body")?;
    Some((sig, name_node, body))
}

pub(crate) fn visibility_of(cx: &Cx<'_, '_>, name: &str) -> Visibility {
    let outer = match cx.innermost_container() {
        Some(Container::Class { exported, .. }) => exported,
        Some(Container::Trait { public, .. }) => public,
        _ => true,
    };
    if !name.starts_with('_') && outer {
        Visibility::Exported
    } else {
        Visibility::Private
    }
}

fn is_forced(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let overridden = n.kind() == "method_declaration"
        && any_named_child(n, |c| {
            c.kind() == "annotation" && annotation_name(cx, c) == "override"
        });
    overridden
        || matches!(
            cx.innermost_container(),
            Some(Container::Class {
                implements: true,
                ..
            })
        )
}

fn collect_params<'s>(cx: &Cx<'_, 's>, sig: Node<'_>) -> Vec<Param<'s>> {
    let mut params = Vec::new();
    for c in named_children(sig) {
        if c.kind() == "formal_parameter_list" {
            params_from_list(cx, c, &mut params);
        }
    }
    params
}

fn missing_of<'s>(params: &[Param<'s>], return_type: Option<&'s str>) -> Vec<&'s str> {
    let mut missing = Vec::new();
    for p in params {
        if p.ty.is_none() {
            missing.push(p.name);
        }
    }
    if return_type.is_none() {
        missing.push("return");
    }
    missing
}

pub(crate) fn is_main_root(cx: &Cx<'_, '_>, node: Node<'_>) -> bool {
    let named = signature_of(node)
        .and_then(|sig| child_by_field(sig, "name"))
        .is_some_and(|id| text(cx, id) == "main");
    named
        && cx
            .parent()
            .is_some_and(|parent| parent.kind() == "source_file")
}

fn is_test_call(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "call_expression"
        && child_by_field(n, "function")
            .is_some_and(|f| matches!(text(cx, f), "test" | "testWidgets" | "group"))
}

fn body_has_test(cx: &Cx<'_, '_>, body: Node<'_>) -> bool {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if is_test_call(cx, node) {
            return true;
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    false
}

fn main_test_region(cx: &Cx<'_, '_>, n: Node<'_>, body: Node<'_>) -> Option<RoleRegion> {
    if is_main_root(cx, n) && body_has_test(cx, body) {
        Some(region_of(body, CodeRole::Test))
    } else {
        None
    }
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if !is_fn_kind(n.kind()) {
        return None;
    }
    let (sig, name_node, body) = fn_parts(n)?;
    if body.kind() != "function_body" {
        return None;
    }
    let name = text(cx, name_node);
    let params = collect_params(cx, sig);
    let return_type = child_by_field(sig, "return_type").map(|t| text(cx, t));
    let missing = missing_of(&params, return_type);
    let (doc, doc_pos) = preceding_comments(
        cx,
        n,
        &["comment", "block_comment", "documentation_block_comment"],
        &[],
    );
    Some(FunctionHead {
        shadow_params: params.iter().map(|param| param.name).collect(),
        name,
        name_pos: pos_of(name_node),
        visibility: visibility_of(cx, name),
        signature_forced: is_forced(cx, n),
        entry_point: is_main_root(cx, n),
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: main_test_region(cx, n, body),
        receiver: None,
        params,
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: sole_call_of(body),
    })
}

pub(crate) fn is_else_if(n: Node<'_>, parent: Option<Node<'_>>) -> bool {
    if n.kind() != "if_statement" {
        return false;
    }
    let Some(p) = parent else {
        return false;
    };
    if p.kind() != "if_statement" {
        return false;
    }
    p.child_by_field_name("alternative")
        .is_some_and(|a| a.id() == n.id())
}

pub(crate) fn closure_shadow_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if n.kind() != "function_expression" {
        return;
    }
    out.extend(collect_params(cx, n).into_iter().map(|param| param.name));
}
