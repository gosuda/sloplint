use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, code_children, last_segment, named_children, text};

use super::dynamic_tables::{
    file_attr_member, file_non_loop_member, file_table_member, is_literal_collection,
    is_plain_value, push_pattern_names,
};
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

fn fallback_label(which: &str) -> &'static str {
    match which {
        "getattr" => "getattr with computed name",
        "setattr" => "setattr with computed name",
        "hasattr" => "hasattr with computed name",
        _ => "delattr with computed name",
    }
}

fn import_name(n: Node<'_>, cx: &Cx<'_, '_>, label: &'static str) -> Option<&'static str> {
    let mut args = positional(n).into_iter();
    let first = args.next()?;
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
    if arg.kind() == "string" || arg.kind() == "concatenated_string" {
        return false;
    }
    if arg.kind() == "attribute" {
        return attr_literal(cx, arg);
    }
    ident_names_closed(cx, arg)
}

fn attr_literal(cx: &Cx<'_, '_>, arg: Node<'_>) -> bool {
    let attr = child_by_field(arg, "attribute").map_or("", |a| text(cx, a));
    if attr.is_empty() {
        return false;
    }
    file_attr_member(cx, attr)
}

fn ident_names_closed(cx: &Cx<'_, '_>, arg: Node<'_>) -> bool {
    let mut ids = Vec::new();
    collect_idents(arg, cx, &mut ids);
    if ids.is_empty() {
        return false;
    }
    if let Some(object) = zero_arg_call_object(arg) {
        let mut chain = Vec::new();
        collect_idents(object, cx, &mut chain);
        if chain.is_empty() {
            return false;
        }
        return chain
            .iter()
            .all(|id| is_loop_bound(cx, id, arg) || file_table_member(cx, id));
    }
    ids.iter()
        .all(|id| is_loop_bound(cx, id, arg) || file_table_member(cx, id))
}

fn zero_arg_call_object<'t>(arg: Node<'t>) -> Option<Node<'t>> {
    let mut cur = arg;
    loop {
        if cur.kind() != "call" {
            return None;
        }
        let args = child_by_field(cur, "arguments")?;
        if !code_children(args).is_empty() {
            return None;
        }
        let func = child_by_field(cur, "function")?;
        if func.kind() != "attribute" {
            return None;
        }
        let object = child_by_field(func, "object")?;
        if object.kind() == "call" {
            cur = object;
            continue;
        }
        return Some(object);
    }
}

fn collect_idents<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            out.push(text(cx, cur));
        }
        let mut cursor = cur.walk();
        stack.extend(cur.children(&mut cursor).filter(|child| child.is_named()));
    }
}

fn contains_range(outer: Node<'_>, inner: Node<'_>) -> bool {
    outer.start_byte() <= inner.start_byte() && inner.end_byte() <= outer.end_byte()
}

fn is_loop_bound(cx: &Cx<'_, '_>, id: &str, arg: Node<'_>) -> bool {
    if file_non_loop_member(cx, id) {
        return false;
    }
    let loops = ancestor_loops(cx, arg);
    if loops.is_empty() {
        return false;
    }
    let mut proven: Vec<&str> = Vec::new();
    for lp in loops {
        extend_proven(cx, lp, &mut proven);
    }
    proven.contains(&id)
}

fn ancestor_loops<'t>(cx: &Cx<'t, '_>, arg: Node<'t>) -> Vec<Node<'t>> {
    let mut loops = Vec::new();
    for a in cx.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if a.kind() == "for_statement" {
            if child_by_field(a, "body").is_some_and(|b| contains_range(b, arg)) {
                loops.push(a);
            }
        } else if matches!(
            a.kind(),
            "list_comprehension"
                | "set_comprehension"
                | "dictionary_comprehension"
                | "generator_expression"
        ) && child_by_field(a, "body").is_some_and(|b| contains_range(b, arg))
        {
            loops.push(a);
        }
    }
    loops
}

fn extend_proven<'s>(cx: &Cx<'_, 's>, lp: Node<'_>, proven: &mut Vec<&'s str>) {
    if lp.kind() == "for_statement" {
        apply_for_loop(lp, cx, proven);
    } else {
        apply_comp_loop(lp, cx, proven);
    }
}

fn apply_for_loop<'s>(lp: Node<'_>, cx: &Cx<'_, 's>, proven: &mut Vec<&'s str>) {
    let Some(left) = child_by_field(lp, "left") else {
        return;
    };
    let Some(right) = child_by_field(lp, "right") else {
        return;
    };
    let mut targets = Vec::new();
    push_pattern_names(left, cx, &mut targets);
    proven.retain(|t| !targets.iter().any(|x| x == t));
    if closed_iterable(cx, right, proven) {
        proven.extend(targets);
    }
}

fn apply_comp_loop<'s>(comp: Node<'_>, cx: &Cx<'_, 's>, proven: &mut Vec<&'s str>) {
    for clause in code_children(comp) {
        if clause.kind() != "for_in_clause" {
            continue;
        }
        apply_for_in_clause(clause, cx, proven);
    }
}

fn apply_for_in_clause<'s>(clause: Node<'_>, cx: &Cx<'_, 's>, proven: &mut Vec<&'s str>) {
    let Some(left) = child_by_field(clause, "left") else {
        return;
    };
    let Some(right) = clause_right(clause, left) else {
        return;
    };
    let mut targets = Vec::new();
    push_pattern_names(left, cx, &mut targets);
    proven.retain(|t| !targets.iter().any(|x| x == t));
    if closed_iterable(cx, right, proven) {
        proven.extend(targets);
    }
}

fn clause_right<'t>(clause: Node<'t>, left: Node<'t>) -> Option<Node<'t>> {
    let mut found: Option<Node<'t>> = None;
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

fn closed_iterable(cx: &Cx<'_, '_>, right: Node<'_>, proven: &[&str]) -> bool {
    if is_literal_collection(right, cx) {
        return true;
    }
    if right.kind() == "identifier" || right.kind() == "attribute" {
        return is_closed_ref(cx, right, proven);
    }
    if right.kind() == "call" {
        return wrapped_literal(cx, right, proven);
    }
    false
}

fn is_closed_ref(cx: &Cx<'_, '_>, n: Node<'_>, proven: &[&str]) -> bool {
    if n.kind() == "identifier" {
        let name = text(cx, n);
        if proven.contains(&name) {
            return true;
        }
        return file_table_member(cx, name);
    }
    if n.kind() == "attribute" {
        let attr = child_by_field(n, "attribute").map_or("", |a| text(cx, a));
        if attr.is_empty() {
            return false;
        }
        return file_attr_member(cx, attr);
    }
    false
}

fn wrapped_literal(cx: &Cx<'_, '_>, call_node: Node<'_>, proven: &[&str]) -> bool {
    let Some(func) = child_by_field(call_node, "function") else {
        return false;
    };
    let args = child_by_field(call_node, "arguments").map_or(Vec::new(), code_children);
    if func.kind() == "identifier" {
        return wrapped_sorted(cx, func, &args, proven);
    }
    if func.kind() == "attribute" {
        return wrapped_keys(cx, func, call_node, &args, proven);
    }
    false
}

fn wrapped_sorted(cx: &Cx<'_, '_>, func: Node<'_>, args: &[Node<'_>], proven: &[&str]) -> bool {
    if args.len() != 1 {
        return false;
    }
    if !matches!(text(cx, func), "sorted" | "list" | "reversed") {
        return false;
    }
    let a = args[0];
    if is_literal_collection(a, cx) {
        return true;
    }
    is_closed_ref(cx, a, proven)
}

fn wrapped_keys(
    cx: &Cx<'_, '_>,
    func: Node<'_>,
    call_node: Node<'_>,
    args: &[Node<'_>],
    proven: &[&str],
) -> bool {
    let _ = call_node;
    if !args.is_empty() {
        return false;
    }
    let method = child_by_field(func, "attribute").map_or("", |a| text(cx, a));
    if !matches!(method, "keys" | "items" | "values") {
        return false;
    }
    let Some(object) = child_by_field(func, "object") else {
        return false;
    };
    if object.kind() == "call" {
        return false;
    }
    is_closed_ref(cx, object, proven)
}
