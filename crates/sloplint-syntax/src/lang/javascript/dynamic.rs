use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, find_named_child, named_children, text,
};

pub(crate) fn closed_name(cx: &Cx<'_, '_>, expr: Node<'_>) -> bool {
    if literal_expr(expr) {
        return true;
    }
    if expr.kind() != "identifier" {
        return false;
    }
    let want = text(cx, expr);
    const_member(cx, want) || loop_member(cx, want, expr)
}

fn literal_expr(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 64 {
            return false;
        }
        match cur.kind() {
            "string" | "number" | "true" | "false" | "null" | "undefined" => {}
            "template_string" => {
                if find_named_child(cur, |child| child.kind() == "template_substitution").is_some()
                {
                    return false;
                }
            }
            "member_expression" => {
                if !child_by_field(cur, "property")
                    .is_some_and(|property| property.kind() == "property_identifier")
                {
                    return false;
                }
                if let Some(object) = child_by_field(cur, "object") {
                    stack.push(object);
                }
            }
            "ternary_expression" => {
                for field in ["consequence", "alternative"] {
                    match child_by_field(cur, field) {
                        Some(branch) => stack.push(branch),
                        None => return false,
                    }
                }
            }
            "binary_expression" | "parenthesized_expression" => stack.extend(named_children(cur)),
            _ => return false,
        }
    }
    true
}

fn const_member(cx: &Cx<'_, '_>, want: &str) -> bool {
    cx.file_names(FileSetKind::Literal, || const_bound_names(cx))
        .binary_search(&want)
        .is_ok()
}

fn const_bound_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut cand: Vec<&'s str> = Vec::new();
    let mut muts: Vec<&'s str> = Vec::new();
    collect_const_muts(root, cx, &mut cand, &mut muts);
    muts.sort_unstable();
    muts.dedup();
    let once = cx.file_names(FileSetKind::Member, || file_once_names(cx));
    let mut out: Vec<&'s str> = Vec::new();
    for name in cand {
        if once.binary_search(&name).is_ok() && muts.binary_search(&name).is_err() {
            out.push(name);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn file_once_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut seen: Vec<&'s str> = Vec::new();
    collect_all_seen(root, cx, &mut seen);
    keep_once(seen)
}

fn file_written_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut out: Vec<&'s str> = Vec::new();
    collect_written(root, cx, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

fn keep_once<'s>(mut seen: Vec<&'s str>) -> Vec<&'s str> {
    seen.sort_unstable();
    let mut out: Vec<&'s str> = Vec::new();
    let mut i = 0usize;
    while i < seen.len() {
        let mut j = i + 1;
        while j < seen.len() && seen[j] == seen[i] {
            j += 1;
        }
        if j == i + 1 {
            out.push(seen[i]);
        }
        i = j;
    }
    out
}

fn collect_const_muts<'s>(
    root: Node<'_>,
    cx: &Cx<'_, 's>,
    cand: &mut Vec<&'s str>,
    muts: &mut Vec<&'s str>,
) {
    let mut queue = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        push_const_cand(cur, cx, cand);
        push_assign_root(cur, cx, muts);
        push_delete_root(cur, cx, muts);
        push_expr_call_roots(cur, cx, muts);
        push_children(&mut queue, cur);
    }
}

fn collect_all_seen<'s>(root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let mut queue = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        push_decl_names(cur, cx, out);
        push_func_param_names(cur, cx, out);
        push_catch_for_names(cur, cx, out);
        push_import_names(cur, cx, out);
        push_children(&mut queue, cur);
    }
}

fn collect_written<'s>(root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let mut queue = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        push_assign_root(cur, cx, out);
        push_children(&mut queue, cur);
    }
}

fn push_const_cand<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if cur.kind() != "lexical_declaration" {
        return;
    }
    if !child_by_field(cur, "kind").is_some_and(|k| text(cx, k) == "const") {
        return;
    }
    let decls = named_children(cur);
    if decls.len() != 1 {
        return;
    }
    let Some(name) = child_by_field(decls[0], "name") else {
        return;
    };
    if name.kind() != "identifier" {
        return;
    }
    let Some(value) = child_by_field(decls[0], "value") else {
        return;
    };
    if is_closed_value(value) {
        out.push(text(cx, name));
    }
}

fn push_decl_names<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if cur.kind() == "lexical_declaration" || cur.kind() == "variable_declaration" {
        for d in named_children(cur) {
            if let Some(name) = child_by_field(d, "name") {
                push_pattern(cx, name, out);
            }
        }
    }
    if matches!(
        cur.kind(),
        "function_declaration" | "generator_function_declaration" | "class_declaration" | "class"
    ) && let Some(name) = child_by_field(cur, "name")
        && name.kind() == "identifier"
    {
        out.push(text(cx, name));
    }
}

fn push_func_param_names<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if !is_func_like(cur.kind()) {
        return;
    }
    if let Some(single) = child_by_field(cur, "parameter") {
        push_pattern(cx, single, out);
    }
    if let Some(list) = child_by_field(cur, "parameters") {
        for p in code_children(list) {
            push_pattern(cx, p, out);
        }
    }
}

fn push_catch_for_names<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if cur.kind() == "catch_clause"
        && let Some(p) = child_by_field(cur, "parameter")
    {
        push_pattern(cx, p, out);
    }
    if cur.kind() == "for_in_statement"
        && let Some(left) = child_by_field(cur, "left")
    {
        push_pattern(cx, left, out);
    }
}

fn push_import_names<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if cur.kind() != "import_statement" {
        return;
    }
    for clause in code_children(cur) {
        if clause.kind() != "import_clause" {
            continue;
        }
        for sub in code_children(clause) {
            if sub.kind() == "identifier" {
                out.push(text(cx, sub));
            }
            if sub.kind() == "namespace_import"
                && let Some(id) = find_named_child(sub, |n| n.kind() == "identifier")
            {
                out.push(text(cx, id));
            }
            if sub.kind() == "named_imports" {
                for spec in code_children(sub) {
                    let Some(name) = child_by_field(spec, "name") else {
                        continue;
                    };
                    if let Some(alias) = child_by_field(spec, "alias")
                        && text(cx, alias) != text(cx, name)
                    {
                        out.push(text(cx, alias));
                        continue;
                    }
                    if name.kind() == "identifier" {
                        out.push(text(cx, name));
                    }
                }
            }
        }
    }
}

fn push_pattern<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![n];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 64 {
            return;
        }
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => out.push(text(cx, cur)),
            "object_pattern" | "array_pattern" | "rest_pattern" | "parenthesized_expression" => {
                stack.extend(named_children(cur))
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(left) = child_by_field(cur, "left") {
                    stack.push(left);
                }
            }
            "pair_pattern" => {
                if let Some(v) = child_by_field(cur, "value") {
                    stack.push(v);
                }
            }
            "required_parameter" | "optional_parameter" => {
                if let Some(p) = child_by_field(cur, "pattern") {
                    stack.push(p);
                } else {
                    stack.extend(named_children(cur));
                }
            }
            _ => {}
        }
    }
}

fn pattern_has<'s>(cx: &Cx<'_, 's>, n: Node<'_>, want: &str) -> bool {
    let mut names: Vec<&str> = Vec::new();
    push_pattern(cx, n, &mut names);
    names.contains(&want)
}

fn func_param_has(cx: &Cx<'_, '_>, func: Node<'_>, want: &str) -> bool {
    if let Some(single) = child_by_field(func, "parameter")
        && pattern_has(cx, single, want)
    {
        return true;
    }
    let Some(list) = child_by_field(func, "parameters") else {
        return false;
    };
    for p in code_children(list) {
        if pattern_has(cx, p, want) {
            return true;
        }
    }
    false
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

fn root_name<'s>(cx: &Cx<'_, 's>, mut n: Node<'_>) -> Option<&'s str> {
    let mut steps = 0usize;
    loop {
        steps += 1;
        if steps > 32 {
            return None;
        }
        match n.kind() {
            "identifier" => return Some(text(cx, n)),
            "member_expression" | "subscript_expression" => {
                n = child_by_field(n, "object")?;
            }
            "parenthesized_expression" => {
                n = code_children(n).into_iter().next()?;
            }
            _ => return None,
        }
    }
}

fn push_assign_root<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if (cur.kind() == "assignment_expression" || cur.kind() == "augmented_assignment_expression")
        && let Some(left) = child_by_field(cur, "left")
        && let Some(name) = root_name(cx, left)
    {
        out.push(name);
    }
    if cur.kind() == "update_expression"
        && let Some(arg) = child_by_field(cur, "argument")
        && let Some(name) = root_name(cx, arg)
    {
        out.push(name);
    }
}

fn push_delete_root<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if cur.kind() != "unary_expression" {
        return;
    }
    let is_delete = child_by_field(cur, "operator").is_some_and(|op| text(cx, op) == "delete")
        || text(cx, cur).trim_start().starts_with("delete");
    if !is_delete {
        return;
    }
    let Some(arg) = child_by_field(cur, "argument") else {
        return;
    };
    if let Some(name) = root_name(cx, arg) {
        out.push(name);
    }
}

fn push_expr_call_roots<'s>(cur: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if cur.kind() != "expression_statement" {
        return;
    }
    let Some(call) = expr_call(cur) else {
        return;
    };
    if let Some(func) = child_by_field(call, "function")
        && (func.kind() == "member_expression" || func.kind() == "subscript_expression")
        && let Some(obj) = child_by_field(func, "object")
        && let Some(name) = root_name(cx, obj)
    {
        out.push(name);
    }
    let Some(args) = child_by_field(call, "arguments") else {
        return;
    };
    for arg in named_children(args) {
        if arg.kind() == "identifier" {
            out.push(text(cx, arg));
        } else if (arg.kind() == "member_expression" || arg.kind() == "subscript_expression")
            && let Some(name) = root_name(cx, arg)
        {
            out.push(name);
        }
    }
}

fn expr_call(n: Node<'_>) -> Option<Node<'_>> {
    for child in code_children(n) {
        if child.kind() == "call_expression" {
            return Some(child);
        }
        if child.kind() == "await_expression" {
            for inner in code_children(child) {
                if inner.kind() == "call_expression" {
                    return Some(inner);
                }
            }
        }
    }
    None
}

fn is_closed_value(root: Node<'_>) -> bool {
    let mut stack = vec![root];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 512 {
            return false;
        }
        match cur.kind() {
            "array" => {
                let kids = code_children(cur);
                if kids.is_empty() {
                    return false;
                }
                for k in kids {
                    if k.kind() == "spread_element" {
                        return false;
                    }
                    stack.push(k);
                }
            }
            "object" => {
                let kids = code_children(cur);
                if kids.is_empty() {
                    return false;
                }
                for k in kids {
                    if k.kind() != "pair" {
                        return false;
                    }
                    if !object_key(k) {
                        return false;
                    }
                    match child_by_field(k, "value") {
                        Some(v) => stack.push(v),
                        None => return false,
                    }
                }
            }
            _ => {
                if !literal_expr(cur) {
                    return false;
                }
            }
        }
    }
    true
}

fn object_key(pair: Node<'_>) -> bool {
    child_by_field(pair, "key")
        .is_some_and(|key| matches!(key.kind(), "property_identifier" | "string" | "number"))
}

fn loop_member(cx: &Cx<'_, '_>, want: &str, expr: Node<'_>) -> bool {
    let once = cx.file_names(FileSetKind::Member, || file_once_names(cx));
    if once.binary_search(&want).is_err() {
        return false;
    }
    let written = cx.file_names(FileSetKind::Module, || file_written_names(cx));
    if written.binary_search(&want).is_ok() {
        return false;
    }
    enclosing_ok(cx, want, expr)
}

fn enclosing_ok(cx: &Cx<'_, '_>, want: &str, expr: Node<'_>) -> bool {
    for anc in cx.ancestors() {
        if is_func_like(anc.kind()) && func_param_has(cx, anc, want) {
            return false;
        }
        if anc.kind() == "catch_clause"
            && let Some(p) = child_by_field(anc, "parameter")
            && pattern_has(cx, p, want)
        {
            return false;
        }
        if anc.kind() == "for_in_statement"
            && let Some(left) = child_by_field(anc, "left")
            && pattern_has(cx, left, want)
        {
            return check_loop(cx, anc, expr);
        }
    }
    false
}

fn check_loop(cx: &Cx<'_, '_>, loop_node: Node<'_>, expr: Node<'_>) -> bool {
    if !is_const_loop(cx, loop_node) {
        return false;
    }
    let Some(right) = child_by_field(loop_node, "right") else {
        return false;
    };
    if !closed_iterable(cx, right) {
        return false;
    }
    let Some(body) = child_by_field(loop_node, "body") else {
        return false;
    };
    body.start_byte() <= expr.start_byte() && expr.end_byte() <= body.end_byte()
}

fn is_const_loop(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "kind").is_some_and(|k| text(cx, k) == "const")
}

fn closed_iterable(cx: &Cx<'_, '_>, right: Node<'_>) -> bool {
    match right.kind() {
        "array" | "object" => is_closed_value(right),
        "call_expression" => keys_call(cx, right),
        "identifier" => const_member(cx, text(cx, right)),
        _ => false,
    }
}

fn keys_call(cx: &Cx<'_, '_>, call: Node<'_>) -> bool {
    let Some(function) = child_by_field(call, "function") else {
        return false;
    };
    if !matches!(
        text(cx, function),
        "Object.keys" | "Object.values" | "Object.entries"
    ) {
        return false;
    }
    let args = child_by_field(call, "arguments").map_or(Vec::new(), named_children);
    args.len() == 1 && keys_source(cx, args[0])
}

fn keys_source(cx: &Cx<'_, '_>, source: Node<'_>) -> bool {
    match source.kind() {
        "array" | "object" => is_closed_value(source),
        "identifier" => const_member(cx, text(cx, source)),
        _ => false,
    }
}

fn push_children<'t>(queue: &mut Vec<Node<'t>>, n: Node<'t>) {
    let mut kids = named_children(n);
    kids.reverse();
    queue.extend(kids);
}
