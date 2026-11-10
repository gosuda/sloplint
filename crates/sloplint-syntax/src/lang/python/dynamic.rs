//! Python dynamic-symbol classification with measured-noise suppression.

use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, last_segment, named_children, text,
};

use super::enclosing_function;

pub(crate) fn py_call_dynamic(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    let func = child_by_field(n, "function")?;
    let callee = text(cx, func);
    let last = last_segment(callee, &["."]);
    if is_attr_fallback(last) {
        return fallback_name(n, cx, last);
    }
    if func.kind() == "identifier" {
        if last == "eval" {
            return Some("eval");
        }
        if last == "exec" {
            return Some("exec");
        }
    }
    if last == "__import__" {
        return import_name(n, cx, "__import__");
    }
    if callee != "importlib.import_module" {
        return None;
    }
    import_name(n, cx, "dynamic import")
}

fn is_attr_fallback(last: &str) -> bool {
    matches!(last, "getattr" | "setattr" | "hasattr" | "delattr")
}

fn positional(n: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(n, "arguments")
        .map_or(Vec::new(), named_children)
        .into_iter()
        .filter(|a| a.kind() != "keyword_argument")
        .collect()
}

fn fallback_name(n: Node<'_>, cx: &Cx<'_, '_>, which: &str) -> Option<&'static str> {
    let args = positional(n);
    let name_arg = args.into_iter().nth(1)?;
    if name_arg.kind() == "string" {
        return None;
    }
    if is_name_attr(cx, name_arg) {
        return None;
    }
    if proxy_forwarded(n, cx, name_arg) {
        return None;
    }
    if name_from_literals(name_arg, cx) {
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
    if first.kind() == "string" {
        return None;
    }
    if first.kind() == "identifier" && name_from_literals(first, cx) {
        return None;
    }
    Some(label)
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
    if idx.kind() == "string" {
        return None;
    }
    if idx.kind() == "identifier" && name_from_literals(idx, cx) {
        return None;
    }
    Some("globals() lookup")
}

fn is_name_attr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "attribute" {
        return false;
    }
    child_by_field(n, "attribute").is_some_and(|a| text(cx, a) == "name")
}

fn proxy_forwarded(_call: Node<'_>, cx: &Cx<'_, '_>, arg: Node<'_>) -> bool {
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
        "parameter" => {
            for sub in named_children(n) {
                param_names(cx, sub, out);
            }
        }
        _ => {}
    }
}

fn name_from_literals(arg: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let mut ids = Vec::new();
    collect_idents(arg, cx, &mut ids);
    if ids.is_empty() {
        return false;
    }
    let mut chain = Vec::new();
    if let Some(object) = zero_arg_call_object(arg) {
        collect_idents(object, cx, &mut chain);
    }
    let check: &[String] = if chain.is_empty() { &ids } else { &chain };
    let closed = |targets: &[String]| ids_closed(check, targets);
    if let Some(func) = enclosing_function(cx) {
        if cx.literal_targets(func.id(), || scope_targets(func, cx), closed) {
            return true;
        }
        return check.iter().all(|id| file_table_member(id, cx));
    }
    let Some(root) = cx.root() else {
        return false;
    };
    let scope = cx
        .ancestors()
        .find(|ancestor| ancestor.kind() == "for_statement")
        .unwrap_or(root);
    if cx.literal_targets(scope.id(), || scope_targets(scope, cx), closed) {
        return true;
    }
    check.iter().all(|id| file_table_member(id, cx))
}

fn zero_arg_call_object(arg: Node<'_>) -> Option<Node<'_>> {
    let mut cur = arg;
    loop {
        if cur.kind() != "call" {
            return None;
        }
        let args = child_by_field(cur, "arguments")?;
        if !named_children(args).is_empty() {
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

fn ids_closed(ids: &[String], targets: &[String]) -> bool {
    !targets.is_empty() && ids.iter().all(|id| targets.iter().any(|t| t == id))
}

fn collect_idents(n: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            out.push(text(cx, cur).to_owned());
        }
        let mut cursor = cur.walk();
        stack.extend(cur.children(&mut cursor).filter(|child| child.is_named()));
    }
}

fn scope_targets(scope: Node<'_>, cx: &Cx<'_, '_>) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![scope];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        if cur.id() != scope.id()
            && matches!(
                cur.kind(),
                "function_definition" | "lambda" | "class_definition"
            )
        {
            continue;
        }
        match cur.kind() {
            "for_statement" => push_for_targets(cur, cx, &mut out),
            "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => push_comprehension_targets(cur, cx, &mut out),
            _ => {}
        }
        push_children(&mut queue, cur);
    }
    out
}

fn push_children<'t>(queue: &mut Vec<Node<'t>>, n: Node<'t>) {
    let mut kids = named_children(n);
    kids.reverse();
    queue.extend(kids);
}

fn push_comprehension_targets(comp: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    for clause in named_children(comp) {
        if clause.kind() != "for_in_clause" {
            continue;
        }
        let Some(left) = child_by_field(clause, "left") else {
            continue;
        };
        let rights: Vec<Node<'_>> = named_children(clause)
            .into_iter()
            .filter(|child| child.id() != left.id() && child.kind() != "if_clause")
            .collect();
        if rights.len() == 1 && closed_iterable(rights[0], cx, out) {
            bind_names(left, cx, out);
        }
    }
}

fn push_for_targets(for_node: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let Some(left) = child_by_field(for_node, "left") else {
        return;
    };
    let Some(right) = child_by_field(for_node, "right") else {
        return;
    };
    if closed_iterable(right, cx, out) {
        bind_names(left, cx, out);
    }
}

fn closed_iterable(right: Node<'_>, cx: &Cx<'_, '_>, bound: &[String]) -> bool {
    if is_literal_collection(right) || alias_ref(right, cx) {
        return true;
    }
    if right.kind() == "call" {
        return wrapped_literal(right, cx);
    }
    if right.kind() == "identifier" {
        return bound.iter().any(|t| t == text(cx, right));
    }
    false
}

fn alias_ref(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let name = match n.kind() {
        "identifier" => text(cx, n),
        "attribute" => child_by_field(n, "attribute")
            .map(|attr| text(cx, attr))
            .unwrap_or(""),
        _ => "",
    };
    !name.is_empty() && file_table_member(name, cx)
}

fn wrapped_literal(call_node: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let Some(func) = child_by_field(call_node, "function") else {
        return false;
    };
    let args = child_by_field(call_node, "arguments").map_or(Vec::new(), named_children);
    if func.kind() == "identifier" {
        return args.len() == 1
            && matches!(text(cx, func), "sorted" | "list" | "reversed")
            && (is_literal_collection(args[0]) || alias_ref(args[0], cx));
    }
    if func.kind() == "attribute" {
        let method = child_by_field(func, "attribute")
            .map(|attr| text(cx, attr))
            .unwrap_or("");
        return args.is_empty()
            && matches!(method, "keys" | "items" | "values")
            && child_by_field(func, "object").is_some_and(|object| alias_ref(object, cx));
    }
    false
}

fn bind_names(n: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" => out.push(text(cx, cur).to_owned()),
            "pattern_list" | "tuple_pattern" | "list_pattern" => {
                stack.extend(named_children(cur).into_iter().rev());
            }
            _ => {}
        }
    }
}

fn file_table_member(name: &str, cx: &Cx<'_, '_>) -> bool {
    cx.file_names(FileSetKind::Literal, || file_tables(cx))
        .binary_search(&name)
        .is_ok()
}

fn file_tables<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut assigned: Vec<&'s str> = Vec::new();
    let mut closed: Vec<&'s str> = Vec::new();
    let mut queue = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        if cur.id() != root.id() && matches!(cur.kind(), "function_definition" | "lambda") {
            continue;
        }
        if cur.kind() == "assignment" {
            record_module_assignment(cur, cx, &mut assigned, &mut closed);
        }
        push_children(&mut queue, cur);
    }
    let mut keep: Vec<&'s str> = closed
        .into_iter()
        .filter(|name| assigned.iter().filter(|other| *other == name).count() == 1)
        .collect();
    keep.sort_unstable();
    keep.dedup();
    keep
}

fn record_module_assignment<'s>(
    assignment: Node<'_>,
    cx: &Cx<'_, 's>,
    assigned: &mut Vec<&'s str>,
    closed: &mut Vec<&'s str>,
) {
    let (Some(left), Some(right)) = (
        child_by_field(assignment, "left"),
        child_by_field(assignment, "right"),
    ) else {
        return;
    };
    if left.kind() != "identifier" {
        return;
    }
    let name = text(cx, left);
    assigned.push(name);
    if is_literal_collection(right) || right.kind() == "string" {
        closed.push(name);
    }
}

fn is_literal_collection(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 256 {
            return false;
        }
        match cur.kind() {
            "string" | "integer" | "float" | "true" | "false" | "none" => {}
            "list" | "tuple" | "set" => {
                let kids = code_children(cur);
                if kids.is_empty() {
                    return false;
                }
                stack.extend(kids);
            }
            "dictionary" => {
                for kid in code_children(cur) {
                    if kid.kind() != "pair" {
                        return false;
                    }
                    match (child_by_field(kid, "key"), child_by_field(kid, "value")) {
                        (Some(key), Some(value)) => {
                            stack.push(key);
                            stack.push(value);
                        }
                        _ => return false,
                    }
                }
            }
            _ => return false,
        }
    }
    true
}
