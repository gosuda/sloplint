use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, first_named_child, first_of_kind, has_child_kind,
    last_segment, named_children, pos_of, text,
};

fn is_cell_token(tok: &str) -> bool {
    tok == "Mutex"
        || tok == "RwLock"
        || tok == "RefCell"
        || tok == "Cell"
        || tok.starts_with("Atomic")
}

fn is_cell_type(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "type").is_some_and(|t| {
        text(cx, t)
            .split(|c: char| !c.is_alphanumeric())
            .any(is_cell_token)
    })
}

fn static_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function {
        return;
    }
    if has_child_kind(n, "mutable_specifier") || is_cell_type(cx, n) {
        let name = child_by_field(n, "name").map_or("", |x| text(cx, x));
        out.push((AmbientKind::GlobalMutable, name, pos_of(n)));
    }
}

fn thread_local_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let Some(tree) = first_of_kind(n, &["token_tree"]) else {
        return "thread_local";
    };
    let mut cursor = tree.walk();
    let mut tokens = tree
        .children(&mut cursor)
        .skip_while(|t| t.kind() != "static");
    tokens
        .nth(1)
        .filter(|t| t.kind() == "identifier")
        .map_or("thread_local", |t| text(cx, t))
}

fn thread_local_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function {
        return;
    }
    let mac = child_by_field(n, "macro").map_or("", |m| text(cx, m));
    if last_segment(mac, &["::"]) != "thread_local" {
        return;
    }
    out.push((
        AmbientKind::GlobalMutable,
        thread_local_name(cx, n),
        pos_of(n),
    ));
}

fn is_env_callee(callee: &str) -> bool {
    const ENVS: [&str; 4] = ["env::var", "env::var_os", "env::vars", "env::vars_os"];
    if ENVS.iter().any(|s| callee.ends_with(*s)) {
        return true;
    }
    const MAX_ENV_TAIL: usize = "env::vars_os".len();
    let mut tail = ['\0'; MAX_ENV_TAIL];
    let mut len = 0;
    for c in callee.chars().rev() {
        if c.is_whitespace() {
            continue;
        }
        if len == MAX_ENV_TAIL {
            break;
        }
        tail[len] = c;
        len += 1;
    }
    ENVS.iter()
        .any(|s| s.len() <= len && s.chars().rev().enumerate().all(|(i, c)| tail[i] == c))
}

fn env_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let callee = text(cx, func);
    if is_env_callee(callee) {
        out.push((AmbientKind::EnvRead, callee, pos_of(func)));
    }
}

pub(super) fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some((text(cx, cur), pos_of(cur))),
            "field_expression" => cur = child_by_field(cur, "value")?,
            "index_expression" => cur = first_named_child(cur)?,
            _ => return None,
        }
    }
}

pub(super) fn write_roots<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if !matches!(
        n.kind(),
        "assignment_expression" | "compound_assignment_expr"
    ) {
        return;
    }
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    let mut targets = Vec::new();
    push_destructure_targets(left, &mut targets);
    for target in targets {
        out.extend(write_root(cx, target));
    }
}

fn push_struct_targets<'a>(cur: Node<'a>, stack: &mut Vec<Node<'a>>) {
    let name = child_by_field(cur, "name").map(|node| node.id());
    for child in named_children(cur) {
        if name.is_some_and(|id| id == child.id()) {
            continue;
        }
        if child.kind() == "field_initializer" {
            if let Some(value) = child_by_field(child, "value") {
                stack.push(value);
            }
        } else if child.kind() == "shorthand_field_initializer" {
            if let Some(id) = first_named_child(child) {
                stack.push(id);
            }
        } else {
            stack.push(child);
        }
    }
}

fn push_destructure_targets<'a>(left: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![left];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "tuple_expression" | "array_expression" => stack.extend(named_children(cur)),
            "parenthesized_expression" => {
                if let Some(inner) = first_named_child(cur) {
                    stack.push(inner);
                }
            }
            "struct_expression" => push_struct_targets(cur, &mut stack),
            _ => out.push(cur),
        }
    }
}

fn collect_static_mut_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "static_item" && has_child_kind(node, "mutable_specifier") {
            if let Some(name) = child_by_field(node, "name") {
                out.push(text(cx, name));
            }
            continue;
        }
        if node.kind() == "function_item" || node.kind() == "closure_expression" {
            continue;
        }
        stack.extend(named_children(node));
    }
}

fn assign_global_write<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    let left = match n.kind() {
        "assignment_expression" | "compound_assignment_expr" => child_by_field(n, "left"),
        _ => None,
    };
    let Some(left) = left else {
        return;
    };
    let Some((root, _)) = write_root(cx, left) else {
        return;
    };
    let globals = cx.file_names(FileSetKind::Scope, || {
        let mut names = Vec::new();
        if let Some(file) = cx.root() {
            collect_static_mut_names(cx, file, &mut names);
        }
        names
    });
    if root != "_" && globals.contains(&root) {
        out.push((AmbientKind::GlobalWrite, root, pos_of(n)));
    }
}

pub(super) fn ambient<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "static_item" {
        static_ambient(cx, n, in_function, out);
        return;
    }
    if n.kind() == "macro_invocation" {
        thread_local_ambient(cx, n, in_function, out);
        return;
    }
    if n.kind() == "call_expression" {
        env_ambient(cx, n, out);
        return;
    }
    if in_function {
        assign_global_write(cx, n, out);
    }
}
