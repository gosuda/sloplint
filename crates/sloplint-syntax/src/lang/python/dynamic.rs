use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, code_children, last_segment, named_children, text};

use super::enclosing_function;

pub(crate) fn py_call_dynamic(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    let func = child_by_field(n, "function")?;
    let callee = text(cx, func);
    let last = last_segment(callee, &["."]);
    if is_attr_fallback(last) {
        return fallback_name(n, cx, last);
    }
    if func.kind() == "identifier" && (last == "eval" || last == "exec") {
        return Some(if last == "eval" { "eval" } else { "exec" });
    }
    if last == "__import__" {
        return import_name(n, cx, "__import__");
    }
    if callee != "importlib.import_module" {
        return None;
    }
    import_name(n, cx, "dynamic import")
}

pub(crate) fn py_subscript_dynamic(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    let v = child_by_field(n, "value")?;
    if v.kind() != "call" {
        return None;
    }
    let f = child_by_field(v, "function").map_or("", |x| text(cx, x));
    let last = last_segment(f, &["."]);
    if last != "globals" && last != "locals" {
        return None;
    }
    let idx = child_by_field(n, "subscript")?;
    subscript_decision(idx, cx)
}

fn subscript_decision(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    if is_plain_value(n, cx) {
        return None;
    }
    if n.kind() == "string" || n.kind() == "concatenated_string" {
        return Some("globals() lookup");
    }
    if n.kind() == "identifier" && name_from_literals(cx, n) {
        return None;
    }
    Some("globals() lookup")
}

fn is_attr_fallback(last: &str) -> bool {
    matches!(last, "getattr" | "setattr" | "hasattr" | "delattr")
}

fn positional(n: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(n, "arguments")
        .map_or(Vec::new(), code_children)
        .into_iter()
        .filter(|a| a.kind() != "keyword_argument")
        .collect()
}

fn fallback_name(n: Node<'_>, cx: &Cx<'_, '_>, which: &str) -> Option<&'static str> {
    let args = positional(n);
    if has_splat_before(&args, 1) {
        return Some(fallback_label(which));
    }
    let name_arg = args.into_iter().nth(1)?;
    if is_plain_value(name_arg, cx) {
        return None;
    }
    if name_arg.kind() == "string" || name_arg.kind() == "concatenated_string" {
        return Some(fallback_label(which));
    }
    if proxy_forwarded(n, cx, name_arg) {
        return None;
    }
    if name_from_literals(cx, name_arg) {
        return None;
    }
    Some(fallback_label(which))
}

fn has_splat_before(args: &[Node<'_>], upto: usize) -> bool {
    for arg in args.iter().take(upto.saturating_add(1)) {
        if is_splat(arg) {
            return true;
        }
    }
    false
}

fn is_splat(n: &Node<'_>) -> bool {
    matches!(n.kind(), "list_splat" | "dictionary_splat")
}

fn fallback_label(which: &str) -> &'static str {
    match which {
        "getattr" => "getattr with computed name",
        "setattr" => "setattr with computed name",
        "hasattr" => "hasattr with computed name",
        _ => "delattr with computed name",
    }
}

fn import_name(n: Node<'_>, cx: &Cx<'_, '_>, label: &'static str) -> Option<&'static str> {
    let args = positional(n);
    if has_splat_before(&args, 0) {
        return Some(label);
    }
    let mut it = args.into_iter();
    let first = it.next()?;
    if is_plain_value(first, cx) {
        return None;
    }
    if first.kind() == "string" || first.kind() == "concatenated_string" {
        return Some(label);
    }
    if first.kind() == "identifier" && name_from_literals(cx, first) {
        return None;
    }
    Some(label)
}

fn proxy_forwarded(call: Node<'_>, cx: &Cx<'_, '_>, arg: Node<'_>) -> bool {
    let _ = call;
    if arg.kind() != "identifier" {
        return false;
    }
    let want = text(cx, arg);
    let Some(func) = enclosing_function(cx) else {
        return false;
    };
    let fname = child_by_field(func, "name").map_or("", |v| text(cx, v));
    if fname != "__getattr__" && fname != "__getattribute__" {
        return false;
    }
    second_param(cx, func).is_some_and(|p| p == want)
}

fn second_param<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let list = child_by_field(func, "parameters")?;
    let mut names = Vec::new();
    for c in named_children(list) {
        param_names(cx, c, &mut names);
    }
    names.into_iter().nth(1)
}

fn param_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "identifier" => out.push(text(cx, n)),
        "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
            push_typed_param(cx, n, out);
        }
        "parameter" => {
            for sub in named_children(n) {
                param_names(cx, sub, out);
            }
        }
        _ => {}
    }
}

fn push_typed_param<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let key = if n.kind() == "typed_parameter" {
        named_children(n)
            .into_iter()
            .find(|c| c.kind() == "identifier")
            .map_or("", |c| text(cx, c))
    } else {
        child_by_field(n, "name").map_or("", |v| text(cx, v))
    };
    if !key.is_empty() {
        out.push(key);
    }
}

fn name_from_literals(cx: &Cx<'_, '_>, arg: Node<'_>) -> bool {
    if is_plain_value(arg, cx) {
        return true;
    }
    if arg.kind() != "identifier" {
        return false;
    }
    bound_by_inline_loop(cx, text(cx, arg), arg)
}

fn is_plain_value(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let _ = cx;
    is_plain_string(n) || is_plain_concat(n)
}

fn is_plain_string(n: Node<'_>) -> bool {
    if n.kind() != "string" {
        return false;
    }
    !named_children(n)
        .iter()
        .any(|c| c.kind() == "interpolation")
}

fn is_plain_concat(n: Node<'_>) -> bool {
    if n.kind() != "concatenated_string" {
        return false;
    }
    let kids = code_children(n);
    if kids.is_empty() {
        return false;
    }
    kids.iter().all(|k| is_plain_string(*k))
}

fn bound_by_inline_loop(cx: &Cx<'_, '_>, name: &str, arg: Node<'_>) -> bool {
    for ancestor in cx.ancestors() {
        if ancestor.kind() == "for_statement" {
            if !for_targets_name(cx, ancestor, name) {
                continue;
            }
            return for_allows(cx, ancestor, name, arg);
        }
        if is_comprehension(ancestor) {
            if !comp_targets_name(cx, ancestor, name) {
                continue;
            }
            return comp_allows(cx, ancestor, name, arg);
        }
    }
    false
}

fn is_comprehension(n: Node<'_>) -> bool {
    matches!(
        n.kind(),
        "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression"
    )
}

fn for_targets_name(cx: &Cx<'_, '_>, lp: Node<'_>, name: &str) -> bool {
    child_by_field(lp, "left").is_some_and(|left| targets_include(cx, left, name))
}

fn for_allows(cx: &Cx<'_, '_>, lp: Node<'_>, name: &str, arg: Node<'_>) -> bool {
    let Some(left) = child_by_field(lp, "left") else {
        return false;
    };
    let Some(right) = child_by_field(lp, "right") else {
        return false;
    };
    let Some(body) = child_by_field(lp, "body") else {
        return false;
    };
    if !contains_range(body, arg) {
        return false;
    }
    if !targets_include(cx, left, name) {
        return false;
    }
    if !iterable_allows(cx, left, right) {
        return false;
    }
    !body_rebinds(cx, body, name, arg)
}

fn iterable_allows(cx: &Cx<'_, '_>, left: Node<'_>, right: Node<'_>) -> bool {
    if left.kind() != "identifier" {
        return false;
    }
    single_table_ok(cx, right)
}

fn single_table_ok(cx: &Cx<'_, '_>, right: Node<'_>) -> bool {
    if !matches!(right.kind(), "list" | "tuple") {
        return false;
    }
    let kids = code_children(right);
    if kids.is_empty() {
        return false;
    }
    kids.iter().all(|k| is_plain_value(*k, cx))
}

fn comp_targets_name(cx: &Cx<'_, '_>, comp: Node<'_>, name: &str) -> bool {
    for clause in code_children(comp) {
        if clause.kind() != "for_in_clause" {
            continue;
        }
        if child_by_field(clause, "left").is_some_and(|left| targets_include(cx, left, name)) {
            return true;
        }
    }
    false
}

fn comp_allows(cx: &Cx<'_, '_>, comp: Node<'_>, name: &str, arg: Node<'_>) -> bool {
    let Some(body) = child_by_field(comp, "body") else {
        return false;
    };
    if !contains_range(body, arg) {
        return false;
    }
    for clause in code_children(comp) {
        if clause.kind() != "for_in_clause" {
            continue;
        }
        let Some(left) = child_by_field(clause, "left") else {
            return false;
        };
        if !targets_include(cx, left, name) {
            continue;
        }
        if !clause_allows(cx, clause, left) {
            return false;
        }
    }
    !body_rebinds(cx, body, name, arg)
}

fn clause_allows(cx: &Cx<'_, '_>, clause: Node<'_>, left: Node<'_>) -> bool {
    let Some(right) = clause_right(clause, left) else {
        return false;
    };
    iterable_allows(cx, left, right)
}

fn clause_right<'t>(clause: Node<'t>, left: Node<'_>) -> Option<Node<'t>> {
    let mut found = None;
    let mut count = 0usize;
    for c in code_children(clause) {
        if c.id() == left.id() || c.kind() == "if_clause" {
            continue;
        }
        count += 1;
        found = Some(c);
    }
    if count == 1 { found } else { None }
}

fn targets_include(cx: &Cx<'_, '_>, left: Node<'_>, name: &str) -> bool {
    let mut stack = vec![left];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            if text(cx, cur) == name {
                return true;
            }
        } else if is_target_pattern(cur) {
            stack.extend(named_children(cur));
        } else {
            return false;
        }
    }
    false
}

fn is_target_pattern(n: Node<'_>) -> bool {
    matches!(
        n.kind(),
        "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "expression_list"
            | "tuple"
            | "list"
            | "splat_pattern"
            | "list_splat_pattern"
            | "dictionary_splat_pattern"
    )
}

fn contains_range(outer: Node<'_>, inner: Node<'_>) -> bool {
    outer.start_byte() <= inner.start_byte() && inner.end_byte() <= outer.end_byte()
}

const BODY_CAP: usize = 10000;

fn body_rebinds(cx: &Cx<'_, '_>, body: Node<'_>, name: &str, arg: Node<'_>) -> bool {
    let mut stack = vec![body];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > BODY_CAP {
            return true;
        }
        if binds_here(cx, cur, name) || defers(cur, arg) || import_here(cx, cur, name) {
            return true;
        }
        stack.extend(named_children(cur));
    }
    false
}

fn defers(cur: Node<'_>, arg: Node<'_>) -> bool {
    matches!(
        cur.kind(),
        "lambda" | "function_definition" | "generator_expression"
    ) && contains_range(cur, arg)
}

fn import_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    match cur.kind() {
        "wildcard_import" => true,
        "import_statement" | "import_from_statement" | "type_alias_statement" => {
            params_hold(cx, cur, name)
        }
        _ => false,
    }
}

fn binds_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if assign_here(cx, cur, name) {
        return true;
    }
    if loop_here(cx, cur, name) {
        return true;
    }
    scope_here(cx, cur, name)
}

fn assign_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if cur.kind() == "named_expression" {
        return child_by_field(cur, "name")
            .is_some_and(|n| n.kind() == "identifier" && text(cx, n) == name);
    }
    if cur.kind() != "assignment" && cur.kind() != "augmented_assignment" {
        return false;
    }
    child_by_field(cur, "left").is_some_and(|left| targets_include(cx, left, name))
}

fn loop_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if cur.kind() != "for_statement" && cur.kind() != "for_in_clause" {
        return false;
    }
    child_by_field(cur, "left").is_some_and(|left| targets_include(cx, left, name))
}

fn scope_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if def_here(cx, cur, name) {
        return true;
    }
    if alias_here(cx, cur, name) {
        return true;
    }
    misc_here(cx, cur, name)
}

fn def_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if (cur.kind() == "function_definition" || cur.kind() == "class_definition")
        && child_by_field(cur, "name").is_some_and(|n| text(cx, n) == name)
    {
        return true;
    }
    if cur.kind() == "parameters" || cur.kind() == "lambda_parameters" {
        return params_hold(cx, cur, name);
    }
    false
}

fn params_hold(cx: &Cx<'_, '_>, root: Node<'_>, name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            if text(cx, cur) == name {
                return true;
            }
        } else {
            stack.extend(named_children(cur));
        }
    }
    false
}

fn alias_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if matches!(cur.kind(), "except_clause" | "as_pattern") {
        return child_by_field(cur, "alias").is_some_and(|a| pattern_holds(cx, a, name));
    }
    false
}

fn pattern_holds(cx: &Cx<'_, '_>, root: Node<'_>, name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if matches!(cur.kind(), "identifier" | "dotted_name") {
            if text(cx, cur) == name {
                return true;
            }
        } else {
            stack.extend(named_children(cur));
        }
    }
    false
}

fn misc_here(cx: &Cx<'_, '_>, cur: Node<'_>, name: &str) -> bool {
    if cur.kind() == "global_statement"
        || cur.kind() == "nonlocal_statement"
        || cur.kind() == "delete_statement"
    {
        return named_children(cur)
            .iter()
            .any(|c| c.kind() == "identifier" && text(cx, *c) == name);
    }
    if cur.kind() == "case_pattern" {
        return pattern_holds(cx, cur, name);
    }
    false
}
