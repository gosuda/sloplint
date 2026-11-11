use sloplint_core::facts::{Param, Pos, PrimitiveKind, Visibility};
use sloplint_core::role::{CodeRole, RoleRegion};
use tree_sitter::Node;

use super::is_test_ident;
use crate::hooks::{
    Cx, FunctionHead, any_named_child, attributed_region, find_named_child, first_named_child,
    first_named_child_matching, first_of_kind, last_named_child, named_children,
    only_named_child_matching, pos_of, preceding_comments, text,
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

fn odin_primitive(ty: Option<&str>) -> PrimitiveKind {
    match ty {
        Some("bool") => PrimitiveKind::Bool,
        Some("string" | "cstring") => PrimitiveKind::Str,
        _ => PrimitiveKind::Other,
    }
}

fn odin_defaulted(cx: &Cx<'_, '_>, child: Node<'_>) -> (bool, bool) {
    let present = (0..child.child_count()).any(|i| {
        child
            .child(i)
            .is_some_and(|c| !c.is_named() && text(cx, c) == "=")
    });
    let boolean = present
        && named_children(child)
            .last()
            .is_some_and(|last| matches!(text(cx, *last), "true" | "false"));
    (present, boolean)
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
            let primitive_kind = odin_primitive(ty);
            let (has_default, bool_default) = odin_defaulted(cx, child);
            for name_node in names_before_sep(cx, child, &[":"]) {
                let param = Param {
                    name: text(cx, name_node),
                    ty,
                    primitive_kind,
                    has_default,
                    bool_default,
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
                has_default: true,
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

enum OdinOuter {
    File,
    Foreign,
    Nested,
}

fn odin_outer(cx: &Cx<'_, '_>, decl: Node<'_>) -> OdinOuter {
    let mut current = cx.parent_of(decl);
    for _ in 0..8 {
        let Some(node) = current else {
            return OdinOuter::Nested;
        };
        match node.kind() {
            "source_file" => return OdinOuter::File,
            "foreign_block" => return OdinOuter::Foreign,
            "block" | "statement" | "when_statement" | "else_clause" | "else_when_clause" => {
                current = cx.parent_of(node);
            }
            _ => return OdinOuter::Nested,
        }
    }
    OdinOuter::Nested
}

fn odin_top_level(cx: &Cx<'_, '_>, decl: Node<'_>) -> bool {
    !matches!(odin_outer(cx, decl), OdinOuter::Nested)
}

fn odin_in_foreign(cx: &Cx<'_, '_>, decl: Node<'_>) -> bool {
    matches!(odin_outer(cx, decl), OdinOuter::Foreign)
}

fn odin_package_is_main(cx: &Cx<'_, '_>) -> bool {
    let Some(root) = cx.root() else {
        return false;
    };
    named_children(root).iter().any(|c| {
        c.kind() == "package_declaration"
            && first_named_child(*c).is_some_and(|id| text(cx, id) == "main")
    })
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
        node = only_named_child_matching(node, odin_is_code)?;
    }
    while node.kind() == "member_expression" {
        node = last_named_child(node)?;
    }
    (node.kind() == "call_expression").then_some(node.id())
}

struct ClosureMeta<'s> {
    name: &'s str,
    pos: Pos,
    visibility: Visibility,
    forced: bool,
    entry: bool,
    region: Option<RoleRegion>,
    doc: Vec<&'s str>,
    doc_pos: Option<Pos>,
}

fn closure_name_node<'a>(parent: Node<'a>) -> Option<Node<'a>> {
    match parent.kind() {
        "procedure_declaration" | "const_declaration" | "variable_declaration" => {
            first_named_child_matching(parent, &["expression", "identifier"])
        }
        "assignment_statement" => {
            let first = first_named_child(parent)?;
            is_identifier(&first).then_some(first)
        }
        _ => None,
    }
}

fn closure_meta<'s>(cx: &Cx<'_, 's>, n: Node<'_>, parent: Node<'_>) -> ClosureMeta<'s> {
    let Some(name_node) = closure_name_node(parent) else {
        return ClosureMeta {
            name: "",
            pos: pos_of(n),
            visibility: Visibility::Private,
            forced: false,
            entry: false,
            region: None,
            doc: Vec::new(),
            doc_pos: None,
        };
    };
    let (doc, doc_pos) = preceding_comments(cx, parent, &["comment", "block_comment"], &[]);
    let visibility = if odin_top_level(cx, parent) && !attr_has(cx, parent, "private") {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    let forced = attr_has(cx, parent, "export")
        || attr_has(cx, parent, "link_name")
        || odin_in_foreign(cx, parent);
    let name = text(cx, name_node);
    ClosureMeta {
        name,
        pos: pos_of(name_node),
        visibility,
        forced,
        entry: name == "main" && odin_package_is_main(cx),
        region: parent_test_attr(cx, parent)
            .map(|attr| attributed_region(n, Some(attr), CodeRole::Test)),
        doc,
        doc_pos,
    }
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
    let holder = first_of_kind(n, &["parameters"]);
    let (params, missing_annotations) = odin_params(cx, holder);
    let return_type = first_of_kind(n, &["type", "named_type"]).map(|t| text(cx, t));
    let meta = closure_meta(cx, n, parent);
    let body = first_of_kind(n, &["block"]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name: meta.name,
        name_pos: meta.pos,
        visibility: meta.visibility,
        signature_forced: meta.forced,
        entry_point: meta.entry,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: meta.region,
        receiver: None,
        params,
        return_type,
        missing_annotations,
        doc: meta.doc,
        doc_pos: meta.doc_pos,
        sole_call: body.and_then(|b| odin_sole(cx, b)),
    })
}
