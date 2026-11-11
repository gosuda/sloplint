use sloplint_core::facts::{Arg, ArgShape, CallKind};
use tree_sitter::{Node, TreeCursor};

use crate::hooks::{
    CallHead, Cx, Link, any_named_child, child_by_field, find_named_child,
    first_named_child_matching, first_of_kind, last_segment, named_children, narrow,
    rfind_named_child, slice, string_arg_shape, text,
};

pub(crate) fn arg_expr(arg: Node<'_>) -> Node<'_> {
    let named = child_by_field(arg, "name").map(|v| v.id());
    rfind_named_child(arg, |c| {
        Some(c.id()) != named && c.kind() != "reference_modifier"
    })
    .unwrap_or(arg)
}

fn fragment_run<'s>(cx: &Cx<'_, 's>, n: Node<'_>, body: &str, frag: &str) -> u32 {
    match find_named_child(n, |c| c.kind() == body) {
        Some(found) => {
            let parts: Vec<&'s str> = named_children(found)
                .into_iter()
                .filter(|c| c.kind() == frag)
                .map(|c| text(cx, c))
                .collect();
            sloplint_core::text::longest_static_chunk(&parts)
        }
        None => 0,
    }
}

fn string_longest<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> u32 {
    match string_arg_shape(cx, n, &["string_content"]) {
        ArgShape::Str { longest_static } => longest_static,
        _ => 0,
    }
}

fn interpolated(n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() != "string_content" && c.kind() != "escape_sequence"
    })
}

fn concat_run(longest: u32) -> ArgShape<'static> {
    ArgShape::Concat {
        longest_static: longest,
    }
}

fn str_run(longest: u32) -> ArgShape<'static> {
    ArgShape::Str {
        longest_static: longest,
    }
}

fn is_dot(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "operator").is_some_and(|o| text(cx, o) == ".")
}

fn static_run<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> u32 {
    match n.kind() {
        "string" | "encapsed_string" => string_longest(cx, n),
        "heredoc" => fragment_run(cx, n, "heredoc_body", "string_content"),
        "nowdoc" => fragment_run(cx, n, "nowdoc_body", "nowdoc_string"),
        _ => 0,
    }
}

fn string_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let longest = string_longest(cx, n);
    if interpolated(n) {
        concat_run(longest)
    } else {
        str_run(longest)
    }
}

fn heredoc_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let longest = fragment_run(cx, n, "heredoc_body", "string_content");
    if interpolated(n) {
        concat_run(longest)
    } else {
        str_run(longest)
    }
}

fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if !is_dot(cx, n) {
        return ArgShape::Other;
    }
    let mut longest = 0;
    let mut stack = vec![n];
    while let Some(node) = stack.pop() {
        if is_dot(cx, node) {
            stack.extend(child_by_field(node, "left"));
            stack.extend(child_by_field(node, "right"));
            continue;
        }
        longest = longest.max(static_run(cx, node));
    }
    concat_run(longest)
}

fn array_shape(n: Node<'_>) -> ArgShape<'static> {
    let mut has_closure = false;
    let mut has_key = false;
    for element in named_children(n) {
        if element.kind() != "array_element_initializer" {
            continue;
        }
        has_closure |= any_named_child(element, |c| {
            matches!(c.kind(), "anonymous_function" | "arrow_function")
        });
        has_key |= element.named_child_count() > 1;
    }
    if has_closure {
        ArgShape::CodeBody
    } else if has_key {
        ArgShape::Other
    } else {
        ArgShape::Collection
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" | "encapsed_string" => string_shape(cx, n),
        "heredoc" => heredoc_shape(cx, n),
        "nowdoc" => str_run(fragment_run(cx, n, "nowdoc_body", "nowdoc_string")),
        "binary_expression" => concat_shape(cx, n),
        "array_creation_expression" => array_shape(n),
        "anonymous_function" | "arrow_function" | "match_expression" => ArgShape::CodeBody,
        "variable_name" => ArgShape::Ident(text(cx, n).strip_prefix('$').unwrap_or(text(cx, n))),
        _ => ArgShape::Other,
    }
}

pub(crate) struct ArgsIter<'n> {
    cursor: Option<TreeCursor<'n>>,
}

impl<'n> Iterator for ArgsIter<'n> {
    type Item = Node<'n>;

    fn next(&mut self) -> Option<Node<'n>> {
        let mut cursor = self.cursor.take()?;
        loop {
            let node = cursor.node();
            let is_arg = node.kind() == "argument";
            let advanced = cursor.goto_next_sibling();
            if is_arg {
                if advanced {
                    self.cursor = Some(cursor);
                }
                return Some(node);
            }
            if !advanced {
                return None;
            }
        }
    }
}

pub(crate) fn arg_nodes<'n>(n: Node<'n>) -> ArgsIter<'n> {
    let list = child_by_field(n, "arguments").or_else(|| first_of_kind(n, &["arguments"]));
    ArgsIter {
        cursor: list.map(|list| {
            let mut cursor = list.walk();
            cursor.goto_first_child();
            cursor
        }),
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arg_nodes(n)
        .map(|a| {
            let expr = arg_expr(a);
            let shape = if expr.kind() == "variadic_unpacking" {
                ArgShape::Other
            } else {
                arg_shape(cx, expr)
            };
            Arg {
                shape,
                start_line: narrow(a.start_position().row) + 1,
                end_line: narrow(a.end_position().row) + 1,
            }
        })
        .collect()
}

fn plain_root(callee: &str) -> Option<&str> {
    callee.split('\\').find(|seg| !seg.is_empty())
}

const RECEIVER_BOTTOM: u8 = 3;
const RECEIVER_BLOCKER: u8 = 5;

fn next_receiver(current: Node<'_>) -> Option<Node<'_>> {
    child_by_field(current, "object").or_else(|| child_by_field(current, "scope"))
}

fn receiver_bottom<'n>(current: Node<'n>) -> Link<'n> {
    if current.kind() == "variable_name" {
        return Link::Root(current);
    }
    match next_receiver(current) {
        Some(next) => Link::Next(next),
        None => Link::Root(current),
    }
}

fn is_simple_static(n: Node<'_>) -> bool {
    child_by_field(n, "name").is_some_and(|v| v.kind() == "name")
}

fn receiver_blocker<'n>(current: Node<'n>) -> Link<'n> {
    if is_call_kind(current.kind()) {
        return Link::Root(current);
    }
    if current.kind() == "subscript_expression" {
        return match current.named_child(0) {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        };
    }
    match next_receiver(current) {
        Some(next) => Link::Next(next),
        None => Link::Root(current),
    }
}

fn is_simple_path(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    match n.kind() {
        "function_call_expression" => child_by_field(n, "function")
            .is_some_and(|f| f.kind() == "name" || f.kind() == "qualified_name"),
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            if !is_simple_static(n) {
                return false;
            }
            let field = if n.kind() == "scoped_call_expression" {
                "scope"
            } else {
                "object"
            };
            let Some(base) = child_by_field(n, field) else {
                return true;
            };
            if is_call_kind(base.kind()) || base.kind() == "subscript_expression" {
                return false;
            }
            cx.chain_root(RECEIVER_BOTTOM, base, receiver_bottom)
                == cx.chain_root(RECEIVER_BLOCKER, base, receiver_blocker)
        }
        "object_creation_expression" => {
            first_named_child_matching(n, &["name", "qualified_name", "relative_name"])
                .is_some_and(|class| class.kind() != "relative_name")
        }
        _ => false,
    }
}

fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "assignment_expression"
        && parent.kind() != "reference_assignment_expression"
    {
        return false;
    }
    if !child_by_field(parent, "right").is_some_and(|r| r.id() == n.id()) {
        return false;
    }
    child_by_field(parent, "left").is_some_and(|l| l.kind() == "variable_name")
}

fn object_root<'s>(cx: &Cx<'_, 's>, obj: Node<'_>) -> Option<&'s str> {
    if obj.kind() != "variable_name" && child_by_field(obj, "object").is_none() {
        return None;
    }
    let root = cx.chain_root(RECEIVER_BOTTOM, obj, receiver_bottom);
    let root = root.strip_prefix('$').unwrap_or(root);
    (!root.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
        && root
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || !b.is_ascii()))
    .then_some(root)
}

fn scope_root<'s>(cx: &Cx<'_, 's>, scope: Node<'_>) -> Option<&'s str> {
    let t = text(cx, scope);
    match scope.kind() {
        "variable_name" => Some(t.strip_prefix('$').unwrap_or(t)),
        "name" | "relative_scope" if !t.is_empty() => Some(t),
        _ => None,
    }
}

const PATH_RECEIVERS: [&str; 9] = [
    "name",
    "qualified_name",
    "relative_name",
    "relative_scope",
    "variable_name",
    "member_access_expression",
    "nullsafe_member_access_expression",
    "scoped_property_access_expression",
    "class_constant_access_expression",
];

fn path_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, bool) {
    let Some(name) = child_by_field(n, "name") else {
        return ("", false);
    };
    let field = if n.kind() == "scoped_call_expression" {
        "scope"
    } else {
        "object"
    };
    let mut start = name.start_byte();
    let mut current = child_by_field(n, field);
    while let Some(receiver) = current {
        if !PATH_RECEIVERS.contains(&receiver.kind()) {
            if is_call_kind(receiver.kind()) || receiver.kind() == "subscript_expression" {
                return (text(cx, name), true);
            }
            break;
        }
        start = receiver.start_byte();
        current = child_by_field(receiver, "object").or_else(|| child_by_field(receiver, "scope"));
    }
    (slice(cx.src, start, name.end_byte()), false)
}

pub(crate) fn stmt_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, CallKind) {
    match n.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            (path_callee(cx, n).0, CallKind::Plain)
        }
        "object_creation_expression" => (
            first_named_child_matching(n, &["name", "qualified_name", "relative_name"])
                .map(|c| text(cx, c))
                .unwrap_or(""),
            CallKind::New,
        ),
        _ => (
            child_by_field(n, "function")
                .map(|f| text(cx, f))
                .unwrap_or(""),
            CallKind::Plain,
        ),
    }
}

pub(crate) fn callee_last<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> &'s str {
    const SEPS: &[&str] = &["::", "->", ".", "\\"];
    let field = if n.kind() == "scoped_call_expression" {
        "scope"
    } else {
        "object"
    };
    let (Some(receiver), Some(name)) = (child_by_field(n, field), child_by_field(n, "name")) else {
        return last_segment(callee, SEPS);
    };
    let callee_tail = slice(cx.src, receiver.end_byte(), name.end_byte());
    last_segment(callee_tail, SEPS)
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "function_call_expression" => {
            let fun = child_by_field(n, "function")?;
            let callee = text(cx, fun);
            Some(CallHead {
                callee,
                kind: CallKind::Plain,
                callee_root: plain_root(callee),
                is_simple_path: is_simple_path(cx, n),
                initializes_binding: initializes_binding(cx, n),
                args: call_args(cx, n),
            })
        }
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            let scoped = n.kind() == "scoped_call_expression";
            let field = if scoped { "scope" } else { "object" };
            let base = child_by_field(n, field)?;
            let (callee, collapsed) = path_callee(cx, n);
            if callee.is_empty() {
                return None;
            }
            let root = if scoped {
                scope_root(cx, base)
            } else {
                object_root(cx, base)
            };
            Some(CallHead {
                callee,
                kind: CallKind::Plain,
                callee_root: if collapsed { None } else { root },
                is_simple_path: if collapsed {
                    false
                } else {
                    is_simple_path(cx, n)
                },
                initializes_binding: initializes_binding(cx, n),
                args: call_args(cx, n),
            })
        }
        "object_creation_expression" => {
            let (callee, kind) = stmt_callee(cx, n);
            if callee.is_empty() {
                return None;
            }
            let named_class =
                first_named_child_matching(n, &["name", "qualified_name", "relative_name"])
                    .is_some_and(|class| class.kind() == "name");
            Some(CallHead {
                callee,
                kind,
                callee_root: if named_class {
                    plain_root(callee)
                } else {
                    None
                },
                is_simple_path: is_simple_path(cx, n),
                initializes_binding: initializes_binding(cx, n),
                args: call_args(cx, n),
            })
        }
        _ => None,
    }
}

pub(crate) fn dynamic_head(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() == "dynamic_variable_name" {
        return Some("variable variable");
    }
    if n.kind() == "function_call_expression" {
        let fun = child_by_field(n, "function")?;
        if matches!(
            fun.kind(),
            "variable_name"
                | "dynamic_variable_name"
                | "string"
                | "encapsed_string"
                | "parenthesized_expression"
        ) {
            return Some("variable function");
        }
        let qualified = text(cx, fun);
        let callee = qualified.strip_prefix('\\').unwrap_or(qualified);
        if callee == "eval" {
            return Some("eval");
        }
        if callee.starts_with("call_user_func") || callee.starts_with("forward_static_call") {
            return Some("call_user_func");
        }
        return None;
    }
    if matches!(
        n.kind(),
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression"
    ) {
        let name = child_by_field(n, "name")?;
        if name.kind() != "name" {
            return Some("variable method name");
        }
    }
    None
}

pub(crate) fn first_arg_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let list = child_by_field(n, "arguments")?;
    first_named_child_matching(list, &["argument"]).map(|a| text(cx, arg_expr(a)))
}

pub(crate) fn is_call_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_call_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "nullsafe_member_call_expression"
    )
}

pub(crate) fn creation_args(n: Node<'_>) -> Vec<Node<'_>> {
    arg_nodes(n).collect()
}

pub(crate) fn call_stmt_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, CallKind) {
    stmt_callee(cx, n)
}

pub(crate) fn call_callee_last<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> &'s str {
    callee_last(cx, n, callee)
}
