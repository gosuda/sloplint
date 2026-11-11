use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, pos_of, text};

pub(crate) fn is_directive_comment(text: &str) -> bool {
    if !text.contains("sloplint-allow") {
        return false;
    }
    let head = text.trim_start();
    !["///", "//!", "/**", "/*!", "(**", "(*!"]
        .iter()
        .any(|prefix| head.starts_with(prefix))
}

pub(crate) fn preceding_comments<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    comment_kinds: &[&str],
    skip_kinds: &[&str],
) -> (Vec<&'s str>, Option<Pos>) {
    let (anchor, earlier) = cx
        .levels
        .iter()
        .rev()
        .filter_map(|level| level.split_last())
        .find(|(last, _)| last.id() == n.id())
        .expect("a header anchor is the visited node or one of its ancestors");
    let mut doc: Vec<&'s str> = Vec::new();
    let mut doc_pos = None;
    let mut header_row = anchor.start_position().row;
    for &sibling in earlier.iter().rev() {
        let kind = sibling.kind();
        if comment_kinds.contains(&kind) {
            let end = sibling.end_position();
            let last_row = if end.column == 0 {
                end.row.saturating_sub(1)
            } else {
                end.row
            };
            if last_row + 1 != header_row {
                break;
            }
            let comment = text(cx, sibling);
            if !is_directive_comment(comment) {
                doc.push(comment);
                doc_pos = Some(pos_of(sibling));
            }
            header_row = sibling.start_position().row;
        } else if skip_kinds.contains(&kind) {
            header_row = sibling.start_position().row;
        } else {
            break;
        }
    }
    doc.reverse();
    (doc, doc_pos)
}
