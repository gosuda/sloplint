//! Static-text measurement for string and concatenation argument shapes.

use sloplint_core::facts::ArgShape;
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::cx::Cx;
use crate::hooks::text;

pub(crate) fn string_arg_shape<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    fragment_kinds: &[&str],
) -> ArgShape<'s> {
    let mut segments: Vec<&'s str> = Vec::new();
    let mut cursor = n.walk();
    for child in n.children(&mut cursor) {
        if fragment_kinds.contains(&child.kind()) {
            segments.push(text(cx, child));
        }
    }
    ArgShape::Str {
        longest_static: longest_static_chunk(&segments),
    }
}

pub(crate) fn binary_concat(
    cx: &Cx<'_, '_>,
    n: Node<'_>,
    op_text: &str,
    string_kinds: &[&str],
) -> bool {
    let mut current = n;
    loop {
        let (Some(left), Some(op), Some(right)) =
            (current.child(0), current.child(1), current.child(2))
        else {
            return false;
        };
        if current.child_count() != 3 || text(cx, op) != op_text {
            return false;
        }
        if string_kinds.contains(&left.kind()) || string_kinds.contains(&right.kind()) {
            return true;
        }
        if left.kind() != current.kind() {
            return false;
        }
        current = left;
    }
}
