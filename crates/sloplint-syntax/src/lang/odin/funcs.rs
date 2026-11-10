use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::{has_operator, is_test_ident};
use crate::hooks::{
    Cx, FunctionHead, any_named_child, attributed_region, find_named_child, first_named_child,
    first_named_child_matching, first_of_kind, named_children, only_named_child_matching, pos_of,
    preceding_comments, text,
};
fn odin_attributes<'a>(decl: Node<'a>) -> Option<Node<'a>> {
    first_named_child_matching(decl, &["attributes"])
}

pub(crate) fn attr_has(cx: &Cx<'_, '_>, decl: Node<'_>, wanted: &str) -> bool {
    let Some(attrs) = odin_attributes(decl) else {
        return false;
    };
    let mut stack = vec![attrs];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" && text(cx, node) == wanted {
            return true;
        }
        for child in named_children(node) {
            stack.push(child);
        }
    }
    false
}

fn parent_test_attr<'a>(cx: &Cx<'_, '_>, decl: Node<'a>) -> Option<Node<'a>> {
    let attrs = odin_attributes(decl)?;
    let mut stack = vec![attrs];
    while let Some(node) = stack.pop() {
        if node.kind() == "attribute" && any_named_child(node, |c| is_test_ident(cx, c)) {
            return Some(node);
        }
        stack.extend(named_children(node));
    }
    None
}

fn is_identifier(c: &Node<'_>) -> bool {
    c.kind() == "identifier"
}

fn names_before_sep<'a>(cx: &Cx<'_, '_>, parent: Node<'a>, seps: &[&str]) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for i in 0..parent.child_count() {
        let Some(child) = parent.child(i) else {
            continue;
        };
        if child.is_named() {
            if is_identifier(&child) {
                out.push(child);
            }
        } else if seps.iter().any(|sep| text(cx, child) == *sep) {
            break;
        }
    }
    out
}

fn odin_params<'s>(cx: &Cx<'_, 's>, holder: Option<Node<'_>>) -> (Vec<Param<'s>>, Vec<&'s str>) {
    let mut params = Vec::new();
    let mut missing = Vec::new();
    let Some(holder) = holder else {
        return (params, missing);
    };
    for child in named_children(holder) {
        if child.kind() == "parameter" {
            let ty = first_of_kind(child, &["type"]).map(|t| text(cx, t));
            let primitive_kind = if ty == Some("bool") {
                PrimitiveKind::Bool
            } else {
                PrimitiveKind::Other
            };
            for name_node in names_before_sep(cx, child, &[":"]) {
                let param = Param {
                    name: text(cx, name_node),
                    ty,
                    primitive_kind,
                    has_default: false,
                    bool_default: false,
                    keyword_only: false,
                    is_variadic: false,
                };
                params.push(param);
            }
        } else if child.kind() == "default_parameter" {
            let name = find_named_child(child, |c| is_identifier(&c))
                .map(|c| text(cx, c))
                .unwrap_or("");
            let value = find_named_child(child, |c| !is_identifier(&c))
                .map(|e| text(cx, e))
                .unwrap_or("");
            let defaulted = value == "true" || value == "false";
            params.push(Param {
                name,
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: defaulted,
                keyword_only: false,
                is_variadic: false,
            });
            if !name.is_empty() {
                missing.push(name);
            }
        }
    }
    (params, missing)
}

fn odin_is_code(child: Node<'_>) -> bool {
    child.kind() != "comment" && child.kind() != "block_comment"
}

fn odin_sole(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    if body.kind() != "block" {
        return None;
    }
    let mut node = only_named_child_matching(body, odin_is_code)?;
    if node.kind() == "statement" {
        node = find_named_child(node, odin_is_code)?;
    }
    if node.kind() == "return_statement" {
        node = find_named_child(node, odin_is_code)?;
    }
    (node.kind() == "call_expression").then_some(node.id())
}

pub(crate) fn closure_head<'s>(
    n: Node<'_>,
    _parent: Option<Node<'_>>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    if n.kind() != "procedure" {
        return None;
    }
    let parent = cx.parent()?;
    let name_node = match parent.kind() {
        "procedure_declaration" | "const_declaration" | "variable_declaration" => {
            first_named_child_matching(parent, &["expression", "identifier"])?
        }
        "assignment_statement" => {
            if !has_operator(parent, cx, ":=") {
                return None;
            }
            let first = first_named_child(parent)?;
            if !is_identifier(&first) {
                return None;
            }
            first
        }
        _ => return None,
    };
    let holder = first_of_kind(n, &["parameters"]);
    let (params, missing_annotations) = odin_params(cx, holder);
    let return_type = first_of_kind(n, &["type", "named_type"]).map(|t| text(cx, t));
    let (doc, doc_pos) = preceding_comments(cx, parent, &["comment", "block_comment"], &[]);
    let top = cx
        .parent_of(parent)
        .is_some_and(|g| g.kind() == "source_file");
    let visibility = if top && !attr_has(cx, parent, "private") {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    let test_attr = parent_test_attr(cx, parent);
    let body = first_of_kind(n, &["block"]);
    Some(FunctionHead {
        name: text(cx, name_node),
        name_pos: pos_of(name_node),
        visibility,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: test_attr.map(|attr| attributed_region(n, Some(attr), CodeRole::Test)),
        params,
        return_type,
        missing_annotations,
        doc,
        doc_pos,
        sole_call: body.and_then(|b| odin_sole(cx, b)),
    })
}
