use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, code_children, named_children, text};

pub(crate) fn closed_name(cx: &Cx<'_, '_>, expr: Node<'_>) -> bool {
    if expr.kind() != "identifier" {
        return false;
    }
    let want = text(cx, expr);
    lookup_closed(cx, want, expr)
}

fn lookup_closed(cx: &Cx<'_, '_>, want: &str, expr: Node<'_>) -> bool {
    let mut visits = 0usize;
    for anc in cx.ancestors() {
        if ancestor_shadow(cx, anc, want) {
            return false;
        }
        if anc.kind() == "for_in_statement" || anc.kind() == "for_statement" {
            if let Some(decided) = for_ancestor_decides(cx, anc, want, expr) {
                return decided;
            }
            continue;
        }
        if let Some(decided) = level_binding(cx, anc, want, &mut visits) {
            return decided;
        }
    }
    false
}

fn ancestor_shadow(cx: &Cx<'_, '_>, anc: Node<'_>, want: &str) -> bool {
    if is_func_like(anc.kind()) && func_param_binds(cx, anc, want) {
        return true;
    }
    if named_scope_binds(cx, anc, want) {
        return true;
    }
    shadowed_catch(cx, anc, want)
}

fn shadowed_catch(cx: &Cx<'_, '_>, anc: Node<'_>, want: &str) -> bool {
    anc.kind() == "catch_clause"
        && child_by_field(anc, "parameter").is_some_and(|param| binds_name(cx, param, want))
}

fn for_ancestor_decides(
    cx: &Cx<'_, '_>,
    anc: Node<'_>,
    want: &str,
    expr: Node<'_>,
) -> Option<bool> {
    if anc.kind() == "for_in_statement" {
        if child_by_field(anc, "left").is_some_and(|left| binds_name(cx, left, want)) {
            return Some(loop_ok(cx, anc, want, expr));
        }
        return None;
    }
    if for_head_binds(cx, anc, want) {
        return Some(false);
    }
    None
}

fn loop_ok(cx: &Cx<'_, '_>, loop_node: Node<'_>, want: &str, expr: Node<'_>) -> bool {
    if !head_const(cx, loop_node) {
        return false;
    }
    if !is_of_loop(cx, loop_node) {
        return false;
    }
    if !head_binds_want(cx, loop_node, want) {
        return false;
    }
    let Some(right) = child_by_field(loop_node, "right") else {
        return false;
    };
    if !inline_string_array(right) {
        return false;
    }
    let Some(body) = child_by_field(loop_node, "body") else {
        return false;
    };
    if !contains(body, expr) {
        return false;
    }
    body_clean(cx, body, want)
}

fn head_const(cx: &Cx<'_, '_>, loop_node: Node<'_>) -> bool {
    child_by_field(loop_node, "kind").is_some_and(|kind| text(cx, kind) == "const")
}

fn is_of_loop(cx: &Cx<'_, '_>, loop_node: Node<'_>) -> bool {
    child_by_field(loop_node, "operator").is_some_and(|op| text(cx, op) == "of")
}

fn head_binds_want(cx: &Cx<'_, '_>, loop_node: Node<'_>, want: &str) -> bool {
    child_by_field(loop_node, "left")
        .is_some_and(|left| left.kind() == "identifier" && text(cx, left) == want)
}

fn inline_string_array(right: Node<'_>) -> bool {
    if right.kind() != "array" {
        return false;
    }
    let kids = code_children(right);
    if kids.is_empty() {
        return false;
    }
    kids.iter().all(|kid| kid.kind() == "string")
}

fn contains(body: Node<'_>, expr: Node<'_>) -> bool {
    body.start_byte() <= expr.start_byte() && expr.end_byte() <= body.end_byte()
}

const BODY_VISIT_CAP: usize = 16384;

fn body_clean(cx: &Cx<'_, '_>, body: Node<'_>, want: &str) -> bool {
    let mut stack = vec![body];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > BODY_VISIT_CAP {
            return false;
        }
        if !visit_body(cx, cur, want, &mut stack) {
            return false;
        }
    }
    true
}

fn visit_body<'t>(cx: &Cx<'_, '_>, cur: Node<'t>, want: &str, stack: &mut Vec<Node<'t>>) -> bool {
    match cur.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            if assigns_target(cx, cur, want) {
                return false;
            }
            push_kids(stack, cur);
        }
        "update_expression" => {
            if updates_name(cx, cur, want) {
                return false;
            }
            push_kids(stack, cur);
        }
        "for_in_statement" => {
            if undeclared_head_binds(cx, cur, want) {
                return false;
            }
            push_kids(stack, cur);
        }
        _ => push_kids(stack, cur),
    }
    true
}

fn assigns_target(cx: &Cx<'_, '_>, cur: Node<'_>, want: &str) -> bool {
    child_by_field(cur, "left").is_some_and(|left| assigns_root(cx, left, want))
}

fn updates_name(cx: &Cx<'_, '_>, cur: Node<'_>, want: &str) -> bool {
    child_by_field(cur, "argument")
        .is_some_and(|arg| arg.kind() == "identifier" && text(cx, arg) == want)
}

fn undeclared_head_binds(cx: &Cx<'_, '_>, loop_node: Node<'_>, want: &str) -> bool {
    if child_by_field(loop_node, "kind").is_some() {
        return false;
    }
    child_by_field(loop_node, "left").is_some_and(|left| binds_name(cx, left, want))
}

fn assigns_root(cx: &Cx<'_, '_>, mut cur: Node<'_>, want: &str) -> bool {
    let mut steps = 0usize;
    loop {
        steps += 1;
        if steps > 32 {
            return true;
        }
        if cur.kind() == "identifier" {
            return text(cx, cur) == want;
        }
        if cur.kind() == "member_expression" || cur.kind() == "subscript_expression" {
            let Some(obj) = child_by_field(cur, "object") else {
                return true;
            };
            cur = obj;
            continue;
        }
        return binds_name(cx, cur, want);
    }
}

fn is_func_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "generator_function_declaration"
            | "function_expression"
            | "generator_function"
            | "arrow_function"
            | "method_definition"
    )
}

fn func_param_binds(cx: &Cx<'_, '_>, func: Node<'_>, want: &str) -> bool {
    if let Some(single) = child_by_field(func, "parameter")
        && binds_name(cx, single, want)
    {
        return true;
    }
    let Some(list) = child_by_field(func, "parameters") else {
        return false;
    };
    for param in code_children(list) {
        if binds_name(cx, param, want) {
            return true;
        }
    }
    false
}

fn binds_name(cx: &Cx<'_, '_>, pat: Node<'_>, want: &str) -> bool {
    let mut stack = vec![pat];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 64 {
            return true;
        }
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                if text(cx, cur) == want {
                    return true;
                }
            }
            "object_pattern" | "array_pattern" => {
                stack.extend(named_children(cur));
            }
            "pair_pattern" => {
                if let Some(value) = child_by_field(cur, "value") {
                    stack.push(value);
                }
            }
            "rest_pattern" => {
                stack.extend(named_children(cur));
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(left) = child_by_field(cur, "left") {
                    stack.push(left);
                }
            }
            "required_parameter" | "optional_parameter" => {
                if let Some(inner) = child_by_field(cur, "pattern") {
                    stack.push(inner);
                } else {
                    stack.extend(named_children(cur));
                }
            }
            "parenthesized_expression" => {
                stack.extend(named_children(cur));
            }
            _ => {}
        }
    }
    false
}

fn push_kids<'t>(stack: &mut Vec<Node<'t>>, cur: Node<'t>) {
    stack.extend(named_children(cur));
}

const CONST_STMT_CAP: usize = 16384;
const CONST_VALUE_CAP: usize = 64;
const CONST_HEAD_CAP: usize = 64;

fn named_scope_binds(cx: &Cx<'_, '_>, anc: Node<'_>, want: &str) -> bool {
    if !matches!(
        anc.kind(),
        "function_declaration"
            | "generator_function_declaration"
            | "function_expression"
            | "generator_function"
            | "class_declaration"
            | "class_expression"
            | "abstract_class_declaration"
    ) {
        return false;
    }
    func_name_binds(cx, anc, want)
}

fn func_name_binds(cx: &Cx<'_, '_>, node: Node<'_>, want: &str) -> bool {
    child_by_field(node, "name")
        .is_some_and(|name| name.kind() == "identifier" && text(cx, name) == want)
}

fn level_binding(cx: &Cx<'_, '_>, anc: Node<'_>, want: &str, visits: &mut usize) -> Option<bool> {
    let mut good = false;
    for stmt in named_children(anc) {
        *visits += 1;
        if *visits > CONST_STMT_CAP {
            return Some(false);
        }
        let decided = value_binding(cx, stmt, want).or_else(|| scope_binding(cx, stmt, want));
        if decided == Some(false) {
            return Some(false);
        }
        if decided == Some(true) {
            good = true;
        }
    }
    good.then_some(true)
}

fn value_binding(cx: &Cx<'_, '_>, stmt: Node<'_>, want: &str) -> Option<bool> {
    match stmt.kind() {
        "lexical_declaration" => lexical_binding(cx, stmt, want),
        "variable_declaration" | "using_declaration" => {
            declarators_bind(cx, stmt, want).then_some(false)
        }
        "function_declaration" | "generator_function_declaration" => {
            func_name_binds(cx, stmt, want).then_some(false)
        }
        "class_declaration" | "abstract_class_declaration" => {
            func_name_binds(cx, stmt, want).then_some(false)
        }
        _ => None,
    }
}

fn lexical_binding(cx: &Cx<'_, '_>, stmt: Node<'_>, want: &str) -> Option<bool> {
    for declarator in named_children(stmt) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name) = child_by_field(declarator, "name") else {
            continue;
        };
        if !binds_name(cx, name, want) {
            continue;
        }
        if const_declarator_value(cx, stmt, declarator, name) {
            return Some(true);
        }
        return Some(false);
    }
    None
}

fn const_declarator_value(
    cx: &Cx<'_, '_>,
    stmt: Node<'_>,
    declarator: Node<'_>,
    name: Node<'_>,
) -> bool {
    if name.kind() != "identifier" {
        return false;
    }
    if !child_by_field(stmt, "kind").is_some_and(|kind| text(cx, kind) == "const") {
        return false;
    }
    child_by_field(declarator, "value").is_some_and(const_string_value)
}

fn declarators_bind(cx: &Cx<'_, '_>, stmt: Node<'_>, want: &str) -> bool {
    for declarator in named_children(stmt) {
        if declarator.kind() == "variable_declarator"
            && child_by_field(declarator, "name").is_some_and(|name| binds_name(cx, name, want))
        {
            return true;
        }
    }
    false
}

fn scope_binding(cx: &Cx<'_, '_>, stmt: Node<'_>, want: &str) -> Option<bool> {
    match stmt.kind() {
        "import_statement" => subtree_ident(cx, stmt, want).then_some(false),
        "export_statement" => export_binding(cx, stmt, want),
        "for_in_statement" | "for_statement" => {
            sibling_for_shadows(cx, stmt, want).then_some(false)
        }
        _ => None,
    }
}

fn export_binding(cx: &Cx<'_, '_>, stmt: Node<'_>, want: &str) -> Option<bool> {
    let mut good = false;
    for child in named_children(stmt) {
        let decided = value_binding(cx, child, want).or_else(|| scope_binding(cx, child, want));
        if decided == Some(false) {
            return Some(false);
        }
        if decided == Some(true) {
            good = true;
        }
    }
    good.then_some(true)
}

fn for_head_binds(cx: &Cx<'_, '_>, node: Node<'_>, want: &str) -> bool {
    if node.kind() == "for_in_statement" {
        return child_by_field(node, "left").is_some_and(|left| binds_name(cx, left, want));
    }
    for_init(node).is_some_and(|init| init_head_binds(cx, init, want))
}

fn init_head_binds(cx: &Cx<'_, '_>, init: Node<'_>, want: &str) -> bool {
    match init.kind() {
        "lexical_declaration" | "variable_declaration" => declarators_bind(cx, init, want),
        "assignment_expression" | "augmented_assignment_expression" => {
            assigns_target(cx, init, want)
        }
        "update_expression" => updates_name(cx, init, want),
        _ => false,
    }
}

fn sibling_for_shadows(cx: &Cx<'_, '_>, stmt: Node<'_>, want: &str) -> bool {
    if stmt.kind() == "for_in_statement" {
        if is_loop_scoped_head(cx, stmt) {
            return false;
        }
        return child_by_field(stmt, "left").is_some_and(|left| binds_name(cx, left, want));
    }
    for_init(stmt).is_some_and(|init| sibling_init_shadows(cx, init, want))
}

fn sibling_init_shadows(cx: &Cx<'_, '_>, init: Node<'_>, want: &str) -> bool {
    match init.kind() {
        "variable_declaration" => declarators_bind(cx, init, want),
        "assignment_expression" | "augmented_assignment_expression" => {
            assigns_target(cx, init, want)
        }
        "update_expression" => updates_name(cx, init, want),
        _ => false,
    }
}

fn is_loop_scoped_head(cx: &Cx<'_, '_>, loop_node: Node<'_>) -> bool {
    child_by_field(loop_node, "kind").is_some_and(|kind| {
        let keyword = text(cx, kind);
        keyword == "const" || keyword == "let"
    })
}

fn for_init(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(init) = child_by_field(node, "init") {
        return Some(init);
    }
    match child_by_field(node, "body") {
        Some(body) => named_children(node)
            .into_iter()
            .find(|child| child.id() != body.id()),
        None => named_children(node).into_iter().next(),
    }
}

fn const_string_value(init: Node<'_>) -> bool {
    let mut stack = vec![init];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > CONST_VALUE_CAP {
            return false;
        }
        match cur.kind() {
            "string" => {}
            "ternary_expression" => {
                let Some(then_branch) = child_by_field(cur, "consequence") else {
                    return false;
                };
                let Some(else_branch) = child_by_field(cur, "alternative") else {
                    return false;
                };
                stack.push(then_branch);
                stack.push(else_branch);
            }
            "parenthesized_expression" => {
                let kids = named_children(cur);
                if kids.len() != 1 {
                    return false;
                }
                stack.push(kids[0]);
            }
            _ => return false,
        }
    }
    true
}

fn subtree_ident(cx: &Cx<'_, '_>, root: Node<'_>, want: &str) -> bool {
    let mut stack = vec![root];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > CONST_HEAD_CAP {
            return true;
        }
        if cur.kind() == "identifier" && text(cx, cur) == want {
            return true;
        }
        stack.extend(named_children(cur));
    }
    false
}
