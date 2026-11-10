use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use crate::hooks::{
    Cx, FunctionHead, any_named_child, attributed_region, child_by_field,
    first_named_child_matching, first_of_kind, only_named_child_matching, pos_of,
    preceding_comments, slice, trimmed,
};

use super::syntax::{BOUNDARY, COMMENT_KINDS, PUBLIC_WORDS};

pub(crate) fn swift_primitive(ty: Option<&str>) -> PrimitiveKind {
    let Some(raw) = ty else {
        return PrimitiveKind::Other;
    };
    let seg = raw.trim().trim_end_matches(['?', '!']);
    let seg = seg.rsplit('.').next().unwrap_or(seg);
    let seg = seg.split(['<', '(', ' ']).next().unwrap_or(seg);
    match seg.trim() {
        "String" => PrimitiveKind::Str,
        "Bool" => PrimitiveKind::Bool,
        "Int" | "Int8" | "Int16" | "Int32" | "Int64" | "UInt" | "UInt8" | "UInt16" | "UInt32"
        | "UInt64" => PrimitiveKind::Int,
        "Float" | "Double" | "CGFloat" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn has_public_modifier(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "modifiers"
            && any_named_child(c, |m| {
                m.kind() == "visibility_modifier" && PUBLIC_WORDS.contains(&trimmed(cx, m))
            })
    })
}

fn has_private_modifier(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "modifiers"
            && any_named_child(c, |m| {
                m.kind() == "visibility_modifier"
                    && matches!(trimmed(cx, m), "private" | "fileprivate")
            })
    })
}

fn enclosing_public(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    !cx.ancestors()
        .take_while(|a| !BOUNDARY.contains(&a.kind()))
        .any(|a| a.kind() == "class_declaration" && !has_public_modifier(cx, a))
}

pub(crate) fn visibility_of(cx: &Cx<'_, '_>, n: Node<'_>) -> Visibility {
    if has_public_modifier(cx, n) && enclosing_public(cx, n) {
        Visibility::Exported
    } else if has_private_modifier(cx, n) {
        Visibility::Private
    } else {
        Visibility::Restricted
    }
}

fn is_override(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "modifiers"
            && any_named_child(c, |m| {
                m.kind() == "member_modifier" && trimmed(cx, m) == "override"
            })
    })
}

fn swift_test_attribute(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "modifiers"
            && any_named_child(c, |a| {
                a.kind() == "attribute"
                    && any_named_child(a, |u| u.kind() == "user_type" && trimmed(cx, u) == "Test")
            })
    })
}

fn sole_call(body: Node<'_>) -> Option<usize> {
    let statements = first_named_child_matching(body, &["statements"])?;
    let only = only_named_child_matching(statements, |c| !COMMENT_KINDS.contains(&c.kind()))?;
    let call = unwrap_statement(only)?;
    (call.kind() == "call_expression" && !super::calls::is_subscript(call)).then(|| call.id())
}

fn unwrap_statement(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        current = match current.kind() {
            "control_transfer_statement" => child_by_field(current, "result")?,
            "try_expression" | "await_expression" => child_by_field(current, "expr")?,
            _ => return Some(current),
        };
    }
}

fn param_type<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> Option<&'s str> {
    child_by_field(parameter, "type").map(|ty| trimmed(cx, ty))
}

fn default_in<'a>(
    defaults: &[Node<'a>],
    cursor: &mut usize,
    after: usize,
    before: usize,
) -> Option<Node<'a>> {
    while *cursor < defaults.len() && defaults[*cursor].start_byte() < after {
        *cursor += 1;
    }
    let found = defaults
        .get(*cursor)
        .filter(|value| value.start_byte() < before)
        .copied();
    if found.is_some() {
        *cursor += 1;
    }
    found
}

fn params<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Vec<Param<'s>> {
    let count = func.child_count();
    let mut parameters: Vec<Node<'_>> = Vec::new();
    let mut defaults: Vec<Node<'_>> = Vec::new();
    for index in 0..count {
        let Some(child) = func.child(index) else {
            continue;
        };
        if child.kind() == "parameter" {
            parameters.push(child);
        } else if func.field_name_for_child(index) == Some("default_value") {
            defaults.push(child);
        }
    }
    let mut cursor = 0;
    parameters
        .iter()
        .enumerate()
        .map(|(position, parameter)| {
            let next = parameters
                .get(position + 1)
                .map_or(usize::MAX, |p| p.start_byte());
            let default_value = default_in(&defaults, &mut cursor, parameter.end_byte(), next);
            let ty = param_type(cx, *parameter);
            Param {
                name: child_by_field(*parameter, "name").map_or("", |name| trimmed(cx, name)),
                ty,
                primitive_kind: swift_primitive(ty),
                has_default: default_value.is_some(),
                bool_default: default_value
                    .is_some_and(|value| matches!(trimmed(cx, value), "true" | "false")),
                keyword_only: false,
                is_variadic: false,
            }
        })
        .collect()
}

fn return_type<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let count = func.child_count();
    let mut arrow = None;
    for index in 0..count {
        if func.child(index).is_some_and(|child| child.kind() == "->") {
            arrow = Some(index);
            break;
        }
    }
    let arrow = arrow?;
    let mut start = None;
    let mut end = None;
    for index in arrow + 1..count {
        if !matches!(
            func.field_name_for_child(index),
            Some("return_type") | Some("name")
        ) {
            continue;
        }
        let Some(child) = func.child(index) else {
            continue;
        };
        if start.is_none() {
            start = Some(child.start_byte());
        }
        end = Some(child.end_byte());
    }
    Some(slice(cx.src, start?, end?))
}

pub(crate) fn untyped_throws(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    first_of_kind(n, &["throws"]).is_some_and(|clause| trimmed(cx, clause) == "throws")
}

pub(crate) fn is_else_if(_n: Node<'_>, parent: Option<Node<'_>>) -> bool {
    parent.is_some_and(|p| p.kind() == "if_statement")
}

fn has_main_attribute(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "modifiers"
            && any_named_child(c, |a| {
                a.kind() == "attribute"
                    && any_named_child(a, |u| u.kind() == "user_type" && trimmed(cx, u) == "main")
            })
    })
}

fn is_main_entry(cx: &Cx<'_, '_>, name: &str) -> bool {
    name == "main"
        && cx
            .ancestors()
            .find(|a| a.kind() == "class_declaration")
            .is_some_and(|a| has_main_attribute(cx, a))
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body")?;
    let is_init = n.kind() == "init_declaration";
    let name_node = child_by_field(n, "name");
    let (name, name_pos) = if is_init {
        match name_node {
            Some(node) => (trimmed(cx, node), pos_of(node)),
            None => ("init", pos_of(n)),
        }
    } else {
        let node = name_node?;
        (trimmed(cx, node), pos_of(node))
    };
    let class = cx.ancestors().find(|a| a.kind() == "class_declaration");
    let xctest = class.is_some_and(|c| {
        any_named_child(c, |s| {
            s.kind() == "inheritance_specifier" && trimmed(cx, s).contains("XCTestCase")
        })
    });
    let tested = xctest || swift_test_attribute(cx, n);
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENT_KINDS, &[]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos,
        visibility: visibility_of(cx, n),
        signature_forced: is_override(cx, n),
        entry_point: is_main_entry(cx, name),
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: tested
            .then(|| attributed_region(n, first_of_kind(n, &["modifiers"]), CodeRole::Test)),
        receiver: None,
        params: params(cx, n),
        return_type: return_type(cx, n),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call(body),
    })
}
