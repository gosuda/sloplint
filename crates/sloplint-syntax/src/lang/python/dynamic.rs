//! Python dynamic-symbol classification with measured-noise suppression.

use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, last_segment, named_children, text};

use super::enclosing_function;

pub(crate) fn py_call_dynamic(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
    let last = last_segment(callee, &["."]);
    if is_attr_fallback(last) {
        return fallback_name(n, cx, last);
    }
    if last == "eval" {
        return Some("eval");
    }
    if last == "exec" {
        return Some("exec");
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
    if name_from_literals(name_arg, n, cx) {
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
    if first.kind() == "identifier" && name_from_literals(first, n, cx) {
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
    if idx.kind() == "identifier" && name_from_literals(idx, n, cx) {
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

fn name_from_literals(arg: Node<'_>, _site: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let Some(func) = enclosing_function(cx) else {
        return false;
    };
    let targets = literal_targets(func, cx);
    if targets.is_empty() {
        return false;
    }
    let mut ids = Vec::new();
    collect_idents(arg, cx, &mut ids);
    if ids.is_empty() {
        return false;
    }
    ids.iter().all(|id| targets.iter().any(|t| t == id))
}

fn collect_idents(n: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            out.push(text(cx, cur).to_owned());
        }
        stack.extend(named_children(cur).into_iter().rev());
    }
}

fn literal_targets(func: Node<'_>, cx: &Cx<'_, '_>) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![func];
    let mut budget = 2000u32;
    while let Some(cur) = stack.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        if cur.id() != func.id()
            && matches!(
                cur.kind(),
                "function_definition" | "lambda" | "class_definition"
            )
        {
            continue;
        }
        if cur.kind() == "for_statement" {
            push_for_targets(cur, cx, &mut out);
        }
        stack.extend(named_children(cur));
    }
    out
}

fn push_for_targets(for_node: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let Some(right) = child_by_field(for_node, "right") else {
        return;
    };
    if !is_literal_collection(right) && !module_alias_is_literal(right, cx) {
        return;
    }
    let Some(left) = child_by_field(for_node, "left") else {
        return;
    };
    bind_names(left, cx, out);
}

fn module_assignments(root: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    for child in named_children(root) {
        if child.kind() == "assignment" {
            out.push(child);
        } else if child.kind() == "expression_statement" {
            for sub in named_children(child) {
                if sub.kind() == "assignment" {
                    out.push(sub);
                }
            }
        }
    }
    out
}

fn module_alias_is_literal(alias: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    if alias.kind() != "identifier" {
        return false;
    }
    let want = text(cx, alias);
    let Some(root) = cx.root() else {
        return false;
    };
    module_assignments(root).into_iter().any(|stmt| {
        child_by_field(stmt, "left")
            .is_some_and(|left| left.kind() == "identifier" && text(cx, left) == want)
            && child_by_field(stmt, "right").is_some_and(is_literal_collection)
    })
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

fn is_literal_collection(n: Node<'_>) -> bool {
    match n.kind() {
        "list" | "tuple" | "set" => {
            let kids = named_children(n);
            !kids.is_empty() && kids.iter().all(|c| c.kind() == "string")
        }
        "dictionary" => {
            let kids = named_children(n);
            !kids.is_empty() && kids.iter().all(|c| c.kind() == "string")
        }
        _ => false,
    }
}
