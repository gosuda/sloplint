use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, code_children, named_children, text};

pub(crate) fn record_mutation<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    mutated: &mut Vec<&'s str>,
    bad_attr: &mut Vec<&'s str>,
) {
    match n.kind() {
        "assignment" | "augmented_assignment" => {
            if let Some(left) = child_by_field(n, "left") {
                if left.kind() == "identifier" && n.kind() == "augmented_assignment" {
                    mutated.push(text(cx, left));
                } else {
                    push_lhs_mutation(left, cx, mutated, bad_attr);
                }
            }
        }
        "delete_statement" => {
            push_delete_mutation(n, cx, mutated, bad_attr);
        }
        "expression_statement" => {
            push_discarded_mutation(n, cx, mutated, bad_attr);
        }
        _ => {}
    }
}

fn push_lhs_mutation<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    mutated: &mut Vec<&'s str>,
    bad_attr: &mut Vec<&'s str>,
) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "subscript" {
            if let Some(root) = target_root(cur, cx) {
                mutated.push(root);
            }
        } else if cur.kind() == "attribute" {
            if let Some(root) = target_root(cur, cx) {
                mutated.push(root);
            }
            let attr = child_by_field(cur, "attribute").map_or("", |a| text(cx, a));
            if !attr.is_empty() {
                bad_attr.push(attr);
            }
        } else if matches!(
            cur.kind(),
            "pattern_list"
                | "tuple_pattern"
                | "list_pattern"
                | "expression_list"
                | "list_splat_pattern"
                | "dictionary_splat_pattern"
                | "splat_pattern"
        ) {
            stack.extend(named_children(cur));
        }
    }
}

fn target_root<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<&'s str> {
    let mut cur = n;
    loop {
        if cur.kind() == "identifier" {
            return Some(text(cx, cur));
        }
        if cur.kind() == "subscript" {
            cur = child_by_field(cur, "value")?;
            continue;
        }
        if cur.kind() == "attribute" {
            cur = child_by_field(cur, "object")?;
            continue;
        }
        return None;
    }
}

fn push_delete_mutation<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    mutated: &mut Vec<&'s str>,
    bad_attr: &mut Vec<&'s str>,
) {
    for c in code_children(n) {
        if c.kind() == "identifier" {
            mutated.push(text(cx, c));
        } else if c.kind() == "expression_list" {
            for k in code_children(c) {
                if k.kind() == "identifier" {
                    mutated.push(text(cx, k));
                } else if k.kind() == "subscript" || k.kind() == "attribute" {
                    push_lhs_mutation(k, cx, mutated, bad_attr);
                }
            }
        } else if c.kind() == "subscript" || c.kind() == "attribute" {
            push_lhs_mutation(c, cx, mutated, bad_attr);
        }
    }
}

fn push_discarded_mutation<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    mutated: &mut Vec<&'s str>,
    bad_attr: &mut Vec<&'s str>,
) {
    let kids = code_children(n);
    if kids.len() != 1 {
        return;
    }
    let mut call = kids[0];
    if call.kind() == "await" {
        let inner = named_children(call);
        if inner.len() != 1 || inner[0].kind() != "call" {
            return;
        }
        call = inner[0];
    }
    if call.kind() != "call" {
        return;
    }
    push_discarded_receiver(call, cx, mutated, bad_attr);
    let Some(args) = child_by_field(call, "arguments") else {
        return;
    };
    push_discarded_args(args, cx, mutated, bad_attr);
}

fn push_discarded_receiver<'s>(
    call: Node<'_>,
    cx: &Cx<'_, 's>,
    mutated: &mut Vec<&'s str>,
    bad_attr: &mut Vec<&'s str>,
) {
    let Some(func) = child_by_field(call, "function") else {
        return;
    };
    if func.kind() != "attribute" {
        return;
    }
    let Some(obj) = child_by_field(func, "object") else {
        return;
    };
    if obj.kind() == "identifier" {
        mutated.push(text(cx, obj));
    } else if obj.kind() == "subscript" || obj.kind() == "attribute" {
        push_lhs_mutation(obj, cx, mutated, bad_attr);
    }
    let attr = child_by_field(func, "attribute").map_or("", |a| text(cx, a));
    if !attr.is_empty() {
        bad_attr.push(attr);
    }
}

fn push_discarded_args<'s>(
    args: Node<'_>,
    cx: &Cx<'_, 's>,
    mutated: &mut Vec<&'s str>,
    bad_attr: &mut Vec<&'s str>,
) {
    let mut stack = named_children(args);
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            mutated.push(text(cx, cur));
        } else if cur.kind() == "attribute" {
            let attr = child_by_field(cur, "attribute").map_or("", |a| text(cx, a));
            if !attr.is_empty() {
                bad_attr.push(attr);
            }
            stack.extend(named_children(cur));
        } else {
            stack.extend(named_children(cur));
        }
    }
}
