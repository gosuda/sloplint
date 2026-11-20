//! Call heads for the Dafny adapter.

use sloplint_core::facts::CallKind;
use tree_sitter::Node;

use super::shapes::{arg_of, call_args, callee_parts, has_actual_binding};
use crate::hooks::{CallHead, Cx, child_by_field, named_children};

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "call_expression" => {
            let function = child_by_field(n, "function")?;
            let (callee, callee_root, is_simple_path) = callee_parts(cx, function);
            Some(CallHead {
                callee,
                kind: CallKind::Plain,
                callee_root,
                is_simple_path,
                initializes_binding: false,
                args: call_args(cx, n),
            })
        }
        "suffix_expression" => {
            if !has_actual_binding(n) {
                return None;
            }
            let function = n.named_child(0)?;
            let (callee, callee_root, is_simple_path) = callee_parts(cx, function);
            Some(CallHead {
                callee,
                kind: CallKind::Plain,
                callee_root,
                is_simple_path,
                initializes_binding: false,
                args: call_args(cx, n),
            })
        }
        "print_statement" => Some(CallHead {
            callee: "print",
            kind: CallKind::Plain,
            callee_root: Some("print"),
            is_simple_path: true,
            initializes_binding: false,
            args: named_children(n)
                .into_iter()
                .map(|value| arg_of(cx, value))
                .collect(),
        }),
        _ => None,
    }
}
