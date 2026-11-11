use sloplint_core::facts::{Pos, Visibility};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use super::OcamlHooks;
use super::params::{is_let_binding, is_parameter, missing_of, ocaml_params, spec_signature};
use crate::hooks::{
    Cx, FunctionHead, any_named_child, child_by_field, find_named_child, first_named_child,
    first_of_kind, named_children, pos_of, preceding_comments, text,
};

fn module_constrained(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        if ancestor.kind() == "module_binding" {
            return child_by_field(ancestor, "module_type").is_some();
        }
    }
    false
}

fn push_pattern_name<'s>(cx: &Cx<'_, 's>, node: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let name = text(cx, node);
    if out.iter().any(|(found, _)| *found == name) {
        return;
    }
    out.push((name, pos_of(node)));
}

pub(crate) fn pattern_names<'s>(cx: &Cx<'_, 's>, pat: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![pat];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind == "typed_pattern" {
            if let Some(inner) = child_by_field(node, "pattern") {
                stack.push(inner);
            }
            continue;
        }
        if kind == "field_pattern" {
            if let Some(inner) = child_by_field(node, "pattern") {
                stack.push(inner);
            }
            continue;
        }
        if matches!(kind, "value_pattern" | "value_name") {
            if node.named_child_count() == 0 {
                push_pattern_name(cx, node, out);
            } else {
                stack.extend(named_children(node).into_iter().rev());
            }
            continue;
        }
        if kind == "identifier" {
            push_pattern_name(cx, node, out);
            continue;
        }
        if matches!(
            kind,
            "constructor_path" | "tag" | "field_path" | "module_path"
        ) {
            continue;
        }
        stack.extend(named_children(node).into_iter().rev());
    }
}

pub(crate) fn pattern_ident<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> Option<(&'s str, Pos)> {
    if matches!(pat.kind(), "identifier" | "value_name") {
        return Some((text(cx, pat), pos_of(pat)));
    }
    let mut found = Vec::new();
    pattern_names(cx, pat, &mut found);
    found.into_iter().next()
}

fn ocaml_header<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<&'s str>, Option<Pos>) {
    if let Some(parent) = cx.parent_of(n)
        && parent.kind() == "value_definition"
    {
        let first = find_named_child(parent, |c| is_let_binding(&c));
        if first.is_some_and(|f| f.id() != n.id()) {
            return (Vec::new(), None);
        }
        return preceding_comments(cx, parent, &["comment"], &[]);
    }
    preceding_comments(cx, n, &["comment"], &[])
}

fn ocaml_sole(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    let mut inner = body;
    if inner.kind() == "parenthesized_expression" {
        inner = first_named_child(inner)?;
    }
    (inner.kind() == "application_expression").then_some(inner.id())
}

fn has_attribute(cx: &Cx<'_, '_>, n: Node<'_>, wanted: Option<&str>) -> bool {
    any_named_child(n, |child| {
        if child.kind() != "item_attribute" {
            return false;
        }
        match wanted {
            None => true,
            Some(name) => {
                first_of_kind(child, &["attribute_id"]).is_some_and(|id| text(cx, id) == name)
            }
        }
    })
}

fn in_module_signature(cx: &Cx<'_, '_>) -> bool {
    let mut chain = cx.ancestors();
    if !chain.next().is_some_and(|p| p.kind() == "signature") {
        return false;
    }
    chain
        .next()
        .is_some_and(|p| p.kind() == "module_type_definition")
}

fn let_head<'s>(
    cx: &Cx<'_, 's>,
    lb: Node<'_>,
    params_src: Node<'_>,
    body: Node<'_>,
    visibility: Visibility,
) -> FunctionHead<'s> {
    let name_node = child_by_field(lb, "pattern");
    let params = ocaml_params(cx, params_src);
    let return_type = child_by_field(lb, "type").map(|t| text(cx, t));
    let missing = missing_of(&params, return_type);
    let (doc, doc_pos) = ocaml_header(cx, lb);
    let shadow_params: Vec<&'s str> = params
        .iter()
        .map(|p| p.name)
        .filter(|name| !name.is_empty())
        .collect();
    FunctionHead {
        shadow_params,
        name: name_node.map(|p| text(cx, p)).unwrap_or(""),
        name_pos: name_node.map(pos_of).unwrap_or_else(|| pos_of(lb)),
        visibility,
        signature_forced: module_constrained(cx),
        entry_point: false,
        in_test: cx.inside_test_module(),
        is_overload: false,
        is_deprecated: has_attribute(cx, lb, Some("deprecated")),
        is_decorated: has_attribute(cx, lb, None),
        is_trait_item: false,
        region: None,
        receiver: None,
        params,
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: ocaml_sole(cx, body),
    }
}

pub(crate) fn function_head<'s>(
    hooks: &OcamlHooks,
    n: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    match n.kind() {
        "let_binding" => {
            if cx.lang != Lang::Ocaml {
                return None;
            }
            let body = child_by_field(n, "body")?;
            let closure = body.kind() == "fun_expression" || body.kind() == "function_expression";
            let typed = any_named_child(n, |c| is_parameter(&c));
            if closure || !typed {
                return None;
            }
            Some(let_head(cx, n, n, body, hooks.visibility_at(cx, n)))
        }
        "method_definition" => {
            let name_node = child_by_field(n, "method_name")?;
            let body = child_by_field(n, "body")?;
            let mut head = let_head(cx, n, n, body, Visibility::Exported);
            head.name = text(cx, name_node);
            head.name_pos = pos_of(name_node);
            Some(head)
        }
        "value_specification" => {
            let name_node = first_of_kind(n, &["value_name"])?;
            let (params, return_type) = spec_signature(cx, n);
            let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
            Some(FunctionHead {
                shadow_params: Vec::new(),
                name: text(cx, name_node),
                name_pos: pos_of(name_node),
                visibility: Visibility::Exported,
                signature_forced: false,
                entry_point: false,
                in_test: false,
                is_overload: false,
                is_deprecated: has_attribute(cx, n, Some("deprecated")),
                is_decorated: has_attribute(cx, n, None),
                is_trait_item: in_module_signature(cx),
                region: None,
                receiver: None,
                params,
                return_type,
                missing_annotations: Vec::new(),
                doc,
                doc_pos,
                sole_call: None,
            })
        }
        _ => None,
    }
}

fn anonymous_closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() == "function_expression" {
        return Some(FunctionHead {
            shadow_params: Vec::new(),
            name: "",
            name_pos: pos_of(n),
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test: cx.inside_test_module(),
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: None,
            receiver: None,
            params: Vec::new(),
            return_type: None,
            missing_annotations: Vec::new(),
            doc: Vec::new(),
            doc_pos: None,
            sole_call: None,
        });
    }
    let body = child_by_field(n, "body")?;
    let params = ocaml_params(cx, n);
    let missing = missing_of(&params, None);
    let shadow = params
        .iter()
        .map(|p| p.name)
        .filter(|name| !name.is_empty())
        .collect();
    Some(FunctionHead {
        shadow_params: shadow,
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: cx.inside_test_module(),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        receiver: None,
        params,
        return_type: None,
        missing_annotations: missing,
        doc: Vec::new(),
        doc_pos: None,
        sole_call: ocaml_sole(cx, body),
    })
}

pub(crate) fn closure_head<'s>(
    hooks: &OcamlHooks,
    n: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    if n.kind() != "fun_expression" && n.kind() != "function_expression" {
        return None;
    }
    if let Some(parent) = cx.parent()
        && parent.kind() == "let_binding"
        && child_by_field(parent, "body").is_some_and(|b| b.id() == n.id())
    {
        if n.kind() == "function_expression" {
            return Some(let_head(
                cx,
                parent,
                parent,
                n,
                hooks.visibility_at(cx, parent),
            ));
        }
        let fun_body = child_by_field(n, "body")?;
        let mut head = let_head(cx, parent, n, fun_body, hooks.visibility_at(cx, parent));
        let mut params = ocaml_params(cx, parent);
        params.append(&mut head.params);
        head.params = params;
        head.missing_annotations = missing_of(&head.params, head.return_type);
        head.shadow_params = head
            .params
            .iter()
            .map(|p| p.name)
            .filter(|name| !name.is_empty())
            .collect();
        return Some(head);
    }
    anonymous_closure_head(cx, n)
}
