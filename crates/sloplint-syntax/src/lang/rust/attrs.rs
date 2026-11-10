use tree_sitter::Node;

use crate::hooks::{
    Cx, any_preceding, child_by_field, first_named_child, first_named_child_matching,
    named_children, text,
};

pub(super) fn attr_last_segment<'s>(cx: &Cx<'_, 's>, attr: Node<'_>) -> &'s str {
    let path = first_named_child(attr).map_or("", |p| text(cx, p));
    path.rsplit("::").next().unwrap_or("")
}

fn cfg_test_ids<'s>(cx: &Cx<'_, 's>, args: Node<'_>) -> Vec<&'s str> {
    named_children(args)
        .into_iter()
        .filter(|c| c.kind() == "identifier")
        .map(|c| text(cx, c))
        .collect()
}

fn cfg_all_implies_test(cx: &Cx<'_, '_>, args: Node<'_>) -> bool {
    let kids = named_children(args);
    if kids.len() != 2 || kids[0].kind() != "identifier" || kids[1].kind() != "token_tree" {
        return false;
    }
    if text(cx, kids[0]) != "all" {
        return false;
    }
    cfg_test_ids(cx, kids[1]).contains(&"test")
}

fn cfg_implies_test(cx: &Cx<'_, '_>, attr: Node<'_>) -> bool {
    let Some(args) = child_by_field(attr, "arguments") else {
        return false;
    };
    if cfg_test_ids(cx, args) == ["test"] {
        return true;
    }
    cfg_all_implies_test(cx, args)
}

fn attribute_is_test(cx: &Cx<'_, '_>, sib: Node<'_>) -> bool {
    let Some(attr) = first_named_child_matching(sib, &["attribute"]) else {
        return false;
    };
    if attr_last_segment(cx, attr) == "test" {
        return true;
    }
    let path = first_named_child(attr).map_or("", |p| text(cx, p));
    if path != "cfg" && !path.ends_with("::cfg") {
        return false;
    }
    cfg_implies_test(cx, attr)
}

fn attribute_is_bench(cx: &Cx<'_, '_>, sib: Node<'_>) -> bool {
    let Some(attr) = first_named_child_matching(sib, &["attribute"]) else {
        return false;
    };
    if child_by_field(attr, "arguments").is_some() {
        return false;
    }
    let path = first_named_child(attr).map_or("", |p| text(cx, p));
    path == "bench" || path.ends_with("::bench")
}

fn attribute_is_deprecated(cx: &Cx<'_, '_>, sib: Node<'_>) -> bool {
    let Some(attr) = first_named_child_matching(sib, &["attribute"]) else {
        return false;
    };
    attr_last_segment(cx, attr) == "deprecated"
}

pub(super) fn has_sibling_attr(
    cx: &Cx<'_, '_>,
    n: Node<'_>,
    mut hit: impl FnMut(&Cx<'_, '_>, Node<'_>) -> bool,
) -> bool {
    any_preceding(
        n,
        "attribute_item",
        &["line_comment", "block_comment"],
        |sib| hit(cx, sib),
    )
}

pub(super) fn has_test_attr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, attribute_is_test)
}

pub(super) fn has_bench_attr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, attribute_is_bench)
}

pub(super) fn has_deprecated_attr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, attribute_is_deprecated)
}

pub(super) fn has_any_attr(n: Node<'_>) -> bool {
    any_preceding(
        n,
        "attribute_item",
        &["line_comment", "block_comment"],
        |_| true,
    )
}
pub(super) fn mod_is_test(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, attribute_is_test)
}
