//! Walks a Rust `use` tree with an explicit stack, so nesting depth cannot exhaust the thread stack.

use sloplint_core::facts::Import;
use tree_sitter::Node;

use super::{leaf_import, wild_import};
use crate::hooks::{Cx, child_by_field, named_children, text};

/// A pending step of [`push_use`]: visit a use-tree node, or restore the prefix length.
enum UseStep<'t> {
    Visit(Node<'t>),
    Restore(usize),
}

/// Pushes every import under the use tree `n`, prefixing each path with `prefix`.
pub(super) fn push_use<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    prefix: &mut String,
    out: &mut Vec<Import<'s>>,
) {
    let mut steps = vec![UseStep::Visit(n)];
    while let Some(step) = steps.pop() {
        match step {
            UseStep::Restore(len) => prefix.truncate(len),
            UseStep::Visit(node) => visit_use(cx, node, prefix, out, &mut steps),
        }
    }
}

/// Expands one use-tree node: a scoped list pushes its prefix and a restore, a list pushes its
/// children in reverse so the first child is visited first, and a leaf emits its import.
fn visit_use<'s, 't>(
    cx: &Cx<'_, 's>,
    n: Node<'t>,
    prefix: &mut String,
    out: &mut Vec<Import<'s>>,
    steps: &mut Vec<UseStep<'t>>,
) {
    match n.kind() {
        "scoped_use_list" => {
            if let Some(path) = child_by_field(n, "path") {
                let base = prefix.len();
                prefix.push_str(text(cx, path));
                prefix.push_str("::");
                steps.push(UseStep::Restore(base));
                steps.extend(child_by_field(n, "list").map(UseStep::Visit));
            }
        }
        "use_list" => steps.extend(named_children(n).into_iter().rev().map(UseStep::Visit)),
        "use_wildcard" => wild_import(cx, n, prefix, out),
        _ => leaf_import(cx, n, prefix, out),
    }
}
