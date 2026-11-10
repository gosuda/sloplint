use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_named_child, first_named_child_matching, text, trimmed,
};

pub(crate) const COMMENTS: &[&str] = &["comment", "block_comment", "documentation_block_comment"];

pub(crate) fn has_anon(n: Node<'_>, kind: &str) -> bool {
    (0..n.child_count())
        .filter_map(|i| n.child(i))
        .any(|c| !c.is_named() && c.kind() == kind)
}

pub(crate) fn cut_generics(t: &str) -> &str {
    match t.find('<') {
        Some(i) => &t[..i],
        None => t,
    }
}

pub(crate) fn last_ident(t: &str) -> &str {
    let base = cut_generics(t);
    let bytes = base.as_bytes();
    let mut end = base.len();
    while end > 0 && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    &base[start..end]
}

pub(crate) fn annotation_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let t = trimmed(cx, n);
    let t = match t.strip_prefix('@') {
        Some(s) => s,
        None => t,
    };
    match t.find('(') {
        Some(i) => &t[..i],
        None => t,
    }
}

pub(crate) fn signature_of(n: Node<'_>) -> Option<Node<'_>> {
    if n.kind() == "function_declaration" {
        let s = child_by_field(n, "signature")?;
        return (s.kind() == "function_signature").then_some(s);
    }
    if n.kind() == "method_declaration" {
        let ms = child_by_field(n, "signature")?;
        if ms.kind() != "method_signature" {
            return None;
        }
        return first_named_child_matching(ms, &["function_signature"]);
    }
    first_named_child_matching(n, &["function_signature"])
}

pub(crate) fn label_text<'s>(cx: &Cx<'_, 's>, lab: Node<'_>) -> &'s str {
    first_named_child(lab)
        .map(|id| text(cx, id))
        .unwrap_or_else(|| text(cx, lab))
}
