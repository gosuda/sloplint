use tree_sitter::Node;

use super::dynamic_mutation::record_mutation;
use crate::hooks::{Cx, FileSetKind, child_by_field, code_children, named_children, text};

pub(crate) fn file_table_member(cx: &Cx<'_, '_>, name: &str) -> bool {
    cx.file_names(FileSetKind::Literal, || file_tables(cx))
        .binary_search(&name)
        .is_ok()
}

pub(crate) fn file_attr_member(cx: &Cx<'_, '_>, name: &str) -> bool {
    cx.file_names(FileSetKind::Member, || file_attr_tables(cx))
        .binary_search(&name)
        .is_ok()
}

pub(crate) fn file_non_loop_member(cx: &Cx<'_, '_>, name: &str) -> bool {
    cx.file_names(FileSetKind::Scope, || file_non_loop(cx))
        .binary_search(&name)
        .is_ok()
}

pub(crate) fn is_plain_value(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    is_plain_string(n) || is_plain_concat(n, cx)
}

pub(crate) fn is_literal_collection(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let mut stack = vec![n];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 256 {
            return false;
        }
        if !push_collection_kids(cur, cx, &mut stack) {
            return false;
        }
    }
    true
}

pub(crate) fn push_pattern_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" => out.push(text(cx, cur)),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "list_splat_pattern"
            | "dictionary_splat_pattern"
            | "splat_pattern" => {
                stack.extend(named_children(cur));
            }
            _ => {}
        }
    }
}

fn push_collection_kids<'t>(n: Node<'t>, cx: &Cx<'_, '_>, stack: &mut Vec<Node<'t>>) -> bool {
    let _ = cx;
    match n.kind() {
        "string" => is_plain_string(n),
        "concatenated_string" => {
            let kids = code_children(n);
            if kids.is_empty() {
                return false;
            }
            stack.extend(kids);
            true
        }
        "integer" | "float" | "true" | "false" | "none" => true,
        "list" | "tuple" | "set" => push_sequence_kids(n, stack),
        "dictionary" => push_dict_kids(n, stack),
        _ => false,
    }
}

fn push_sequence_kids<'t>(n: Node<'t>, stack: &mut Vec<Node<'t>>) -> bool {
    let kids = code_children(n);
    if kids.is_empty() {
        return false;
    }
    if kids.iter().any(|k| {
        matches!(
            k.kind(),
            "list_splat" | "parenthesized_list_splat" | "dictionary_splat" | "yield"
        )
    }) {
        return false;
    }
    stack.extend(kids);
    true
}

fn push_dict_kids<'t>(n: Node<'t>, stack: &mut Vec<Node<'t>>) -> bool {
    let kids = code_children(n);
    if kids.is_empty() {
        return false;
    }
    for kid in kids {
        if kid.kind() != "pair" {
            return false;
        }
        let (Some(key), Some(value)) = (child_by_field(kid, "key"), child_by_field(kid, "value"))
        else {
            return false;
        };
        stack.push(key);
        stack.push(value);
    }
    true
}

fn is_plain_string(n: Node<'_>) -> bool {
    if n.kind() != "string" {
        return false;
    }
    for c in named_children(n) {
        if c.kind() == "interpolation" {
            return false;
        }
    }
    true
}

fn is_plain_concat(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let _ = cx;
    if n.kind() != "concatenated_string" {
        return false;
    }
    let kids = code_children(n);
    if kids.is_empty() {
        return false;
    }
    kids.iter().all(|k| is_plain_string(*k))
}

fn keeps_allowed(kind: &str) -> bool {
    matches!(
        kind,
        "module"
            | "block"
            | "class_definition"
            | "decorated_definition"
            | "expression_statement"
            | "if_statement"
            | "elif_clause"
            | "else_clause"
    )
}

fn file_tables<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let mut names = collect_all(cx);
    filter_ident_tables(&mut names.all, &mut names.ident, &mut names.mutated)
}

fn file_attr_tables<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let mut names = collect_all(cx);
    filter_attr_tables(
        &mut names.all,
        &mut names.attr,
        &mut names.bad,
        &mut names.mutated,
    )
}

#[derive(Default)]
struct FileNames<'s> {
    all: Vec<&'s str>,
    ident: Vec<&'s str>,
    attr: Vec<&'s str>,
    mutated: Vec<&'s str>,
    bad: Vec<&'s str>,
}

fn collect_all<'s>(cx: &Cx<'_, 's>) -> FileNames<'s> {
    let mut names = FileNames::default();
    let Some(root) = cx.root() else {
        return names;
    };
    let mut queue: Vec<(Node<'_>, bool, bool)> = vec![(root, true, false)];
    let mut head = 0usize;
    while head < queue.len() {
        let (cur, allowed, in_class) = queue[head];
        head += 1;
        record_all_bindings(cur, cx, &mut names.all);
        if allowed {
            record_literal_binding(cur, cx, &mut names.ident);
            if in_class {
                record_literal_binding(cur, cx, &mut names.attr);
            }
        }
        record_mutation(cur, cx, &mut names.mutated, &mut names.bad);
        let child_allowed = allowed && keeps_allowed(cur.kind());
        let child_class = in_class || cur.kind() == "class_definition";
        for kid in named_children(cur) {
            queue.push((kid, child_allowed, child_class));
        }
    }
    names
}

fn file_non_loop<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut out: Vec<&'s str> = Vec::new();
    let mut queue: Vec<Node<'_>> = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        record_shared_bindings(cur, cx, &mut out);
        record_more(cur, cx, &mut out);
        for kid in named_children(cur) {
            queue.push(kid);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn record_all_bindings<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    record_shared_bindings(n, cx, out);
    match n.kind() {
        "for_statement" | "for_in_clause" => {
            if let Some(left) = child_by_field(n, "left") {
                push_pattern_names(left, cx, out);
            }
        }
        _ => {}
    }
    record_more(n, cx, out);
}

fn record_shared_bindings<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "assignment" | "augmented_assignment" => {
            if let Some(left) = child_by_field(n, "left") {
                push_pattern_names(left, cx, out);
            }
        }
        "named_expression" => {
            if let Some(name) = child_by_field(n, "name")
                && name.kind() == "identifier"
            {
                out.push(text(cx, name));
            }
        }
        "except_clause" | "as_pattern" => push_alias_binding(n, cx, out),
        _ => {}
    }
}

fn record_more<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "import_statement" | "import_from_statement" => push_import_bindings(n, cx, out),
        "function_definition" | "class_definition" => {
            if let Some(name) = child_by_field(n, "name") {
                out.push(text(cx, name));
            }
        }
        "parameters" | "lambda_parameters" => push_param_bindings(n, cx, out),
        "global_statement" | "nonlocal_statement" => {
            for c in named_children(n) {
                if c.kind() == "identifier" {
                    out.push(text(cx, c));
                }
            }
        }
        "delete_statement" => push_delete_binding(n, cx, out),
        _ => {}
    }
}

fn push_alias_binding<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let Some(alias) = child_by_field(n, "alias") else {
        return;
    };
    let mut stack = vec![alias];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            out.push(text(cx, cur));
        } else {
            stack.extend(named_children(cur));
        }
    }
}

fn push_param_bindings<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    for c in named_children(n) {
        if let Some(name) = param_ident(c, cx)
            && !name.is_empty()
        {
            out.push(name);
        }
    }
}

fn param_ident<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<&'s str> {
    match n.kind() {
        "identifier" => Some(text(cx, n)),
        "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => named_children(n)
            .into_iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| text(cx, c)),
        "default_parameter" | "typed_default_parameter" => {
            let v = child_by_field(n, "name").map_or("", |x| text(cx, x));
            if v.is_empty() { None } else { Some(v) }
        }
        _ => None,
    }
}

fn push_import_bindings<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if n.kind() == "import_statement" {
        for c in named_children(n) {
            if c.kind() == "aliased_import" {
                push_aliased_import(c, cx, out);
            } else if c.kind() == "dotted_name" {
                push_dotted_parts(c, cx, out);
            }
        }
        return;
    }
    let mod_id = child_by_field(n, "module_name").map(|m| m.id());
    for c in named_children(n) {
        if Some(c.id()) == mod_id {
            continue;
        }
        if c.kind() == "aliased_import" {
            push_aliased_import(c, cx, out);
        } else if c.kind() == "dotted_name" {
            let full = text(cx, c);
            if let Some(last) = full.split('.').next_back()
                && !last.is_empty()
            {
                out.push(last);
            }
        } else if c.kind() == "identifier" {
            out.push(text(cx, c));
        }
    }
}

fn push_aliased_import<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let alias = child_by_field(n, "alias").map_or("", |a| text(cx, a));
    if !alias.is_empty() {
        out.push(alias);
        return;
    }
    if let Some(name) = child_by_field(n, "name") {
        push_dotted_parts(name, cx, out);
    }
}

fn push_dotted_parts<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    let full = text(cx, n);
    if let Some(first) = full.split('.').next()
        && !first.is_empty()
    {
        out.push(first);
    }
    if let Some(last) = full.split('.').next_back()
        && !last.is_empty()
        && Some(last) != full.split('.').next()
    {
        out.push(last);
    }
}

fn push_delete_binding<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    for c in code_children(n) {
        if c.kind() == "identifier" {
            out.push(text(cx, c));
        }
    }
}

fn record_literal_binding<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if n.kind() != "assignment" {
        return;
    }
    let (Some(left), Some(right)) = (child_by_field(n, "left"), child_by_field(n, "right")) else {
        return;
    };
    if left.kind() != "identifier" {
        return;
    }
    if is_literal_collection(right, cx) || is_plain_value(right, cx) {
        out.push(text(cx, left));
    }
}

fn only_once<'s>(all: &mut Vec<&'s str>) -> Vec<&'s str> {
    all.sort_unstable();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < all.len() {
        let mut j = i + 1;
        while j < all.len() && all[j] == all[i] {
            j += 1;
        }
        if j == i + 1 {
            out.push(all[i]);
        }
        i = j;
    }
    out
}

fn dedup_sorted(names: &mut Vec<&str>) {
    names.sort_unstable();
    names.dedup();
}

fn filter_ident_tables<'s>(
    all: &mut Vec<&'s str>,
    closed: &mut Vec<&'s str>,
    mutated: &mut Vec<&'s str>,
) -> Vec<&'s str> {
    let singles = only_once(all);
    dedup_sorted(mutated);
    dedup_sorted(closed);
    let mut out = Vec::new();
    for name in closed.iter() {
        if singles.binary_search(name).is_ok() && mutated.binary_search(name).is_err() {
            out.push(*name);
        }
    }
    out
}

fn filter_attr_tables<'s>(
    all: &mut Vec<&'s str>,
    closed: &mut Vec<&'s str>,
    bad: &mut Vec<&'s str>,
    mutated: &mut Vec<&'s str>,
) -> Vec<&'s str> {
    let singles = only_once(all);
    dedup_sorted(bad);
    dedup_sorted(mutated);
    dedup_sorted(closed);
    let mut out = Vec::new();
    for name in closed.iter() {
        if singles.binary_search(name).is_ok()
            && bad.binary_search(name).is_err()
            && mutated.binary_search(name).is_err()
        {
            out.push(*name);
        }
    }
    out
}
