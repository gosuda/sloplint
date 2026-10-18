use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, Handler, Import, ImportKind, Param, Pos, Stmt,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, binary_concat, child_by_field, last_segment,
    named_children, narrow, pos_of, preceding_comments, string_arg_shape, text, trimmed,
};
use crate::spec::Role;

/// R adapter hooks: named closures, calls, assignment targets, and `tryCatch` handlers.
pub(crate) struct RHooks;

/// R node kinds mapped to walker roles.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Closure),
    ("if_statement", Role::If),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("repeat_statement", Role::Loop),
    ("call", Role::Call),
    ("binary_operator", Role::Bind),
    ("comment", Role::Comment),
    ("argument", Role::MatchArm),
];

/// The R adapter instance the grammar table points at.
pub(crate) static HOOKS: RHooks = RHooks;

const COMMENT_KINDS: &[&str] = &["comment"];

impl LangHooks for RHooks {
    fn function<'s>(&self, _n: Node<'_>, _cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        None
    }

    fn closure_as_function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let (name, binary) = bound_function(cx, n)?;
        let (doc, doc_pos) = preceding_comments(cx, binary, COMMENT_KINDS, &[]);
        Some(FunctionHead {
            name,
            name_pos: pos_of(binary),
            public: !name.starts_with('.'),
            interface_bound: false,
            in_test: cx.is_test_file,
            params: params(cx, n),
            return_type: None,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(child_by_field(n, "body")),
        })
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent().is_some_and(|parent| {
            parent.kind() == "if_statement"
                && child_by_field(parent, "alternative").is_some_and(|alt| alt.id() == n.id())
        })
    }

    fn control_call(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        child_by_field(n, "function").is_some_and(|f| trimmed(cx, f) == "switch")
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let target = match operator_text(cx, n) {
            "<-" | "=" | ":=" => child_by_field(n, "lhs"),
            "->" => child_by_field(n, "rhs"),
            _ => None,
        };
        if let Some(name) = target.filter(|t| t.kind() == "identifier") {
            out.push((text(cx, name), pos_of(name)));
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let target = match operator_text(cx, n) {
            "<-" | "=" | ":=" => child_by_field(n, "lhs").filter(|lhs| lhs.kind() != "identifier"),
            "<<-" => child_by_field(n, "lhs"),
            "->>" => child_by_field(n, "rhs"),
            _ => None,
        };
        out.extend(target.and_then(|node| write_root(cx, node)));
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        let callee = child_by_field(n, "function")?;
        Some(CallHead {
            callee: trimmed(cx, callee),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, callee),
            args: call_args(cx, n),
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let Some(function) = child_by_field(n, "function") else {
            return;
        };
        let kind = match last_segment(trimmed(cx, function), &["::"]) {
            "library" | "require" => ImportKind::Wildcard,
            "requireNamespace" | "source" => ImportKind::SideEffect,
            _ => return,
        };
        let Some(first) = argument_values(n).first().copied() else {
            return;
        };
        let module = if first.kind() == "string" {
            string_content(cx, first).unwrap_or("")
        } else {
            trimmed(cx, first)
        };
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(module),
            name: module,
            alias: None,
            kind,
            in_test: cx.is_test_file,
        });
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "argument"
            || child_by_field(n, "name").map(|name| trimmed(cx, name)) != Some("error")
        {
            return None;
        }
        let call = n.parent()?.parent().filter(|c| c.kind() == "call")?;
        let function = child_by_field(call, "function")?;
        if last_segment(trimmed(cx, function), &["::"]) != "tryCatch" {
            return None;
        }
        let handler_fn =
            child_by_field(n, "value").filter(|v| v.kind() == "function_definition")?;
        let binding = child_by_field(handler_fn, "parameters")
            .and_then(|list| {
                named_children(list)
                    .into_iter()
                    .find(|p| p.kind() == "parameter")
            })
            .map(|parameter| param_name(cx, parameter));
        Some(Handler {
            pos: pos_of(n),
            stmts: handler_statements(cx, handler_fn, binding),
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() != "call" {
            return;
        }
        let Some(function) = child_by_field(n, "function") else {
            return;
        };
        let name = trimmed(cx, function);
        let kind = match last_segment(name, &["::"]) {
            "Sys.getenv" => AmbientKind::EnvRead,
            "options" | "Sys.setenv" | "Sys.unsetenv" if in_function => AmbientKind::GlobalWrite,
            _ => return,
        };
        out.push((kind, name, pos_of(n)));
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        let short = last_segment(trimmed(cx, child_by_field(n, "function")?), &["::"]);
        let first = argument_values(n).first().copied();
        match short {
            "eval" => Some("eval() of runtime code"),
            "evalq" => Some("evalq() of runtime code"),
            "do.call" => first
                .filter(|v| !matches!(v.kind(), "string" | "identifier"))
                .map(|_| "do.call() with computed function name"),
            _ => first
                .filter(|v| v.kind() != "string")
                .and_then(|_| computed_lookup(short)),
        }
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        let function = child_by_field(n, "function")?;
        let arguments = argument_nodes(n);
        let pos = pos_of(n);
        match last_segment(trimmed(cx, function), &["::"]) {
            "stopifnot" => Some(Assertion {
                pos,
                has_message: arguments
                    .iter()
                    .any(|a| child_by_field(*a, "name").is_some()),
                comparison: false,
            }),
            "expect_true" | "expect_false" => Some(Assertion {
                pos,
                has_message: has_message(cx, &arguments, 1),
                comparison: first_is_comparison(cx, &arguments),
            }),
            "expect_equal" | "expect_identical" => Some(Assertion {
                pos,
                has_message: has_message(cx, &arguments, 2),
                comparison: true,
            }),
            _ => None,
        }
    }
}

fn operator_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    child_by_field(n, "operator").map_or("", |op| trimmed(cx, op))
}

fn bound_function<'s, 'a>(cx: &Cx<'_, 's>, func: Node<'a>) -> Option<(&'s str, Node<'a>)> {
    let binary = func.parent().filter(|p| p.kind() == "binary_operator")?;
    let op = operator_text(cx, binary);
    let lhs = child_by_field(binary, "lhs")?;
    let rhs = child_by_field(binary, "rhs")?;
    let (target, function_on_right) =
        if rhs.id() == func.id() && matches!(op, "<-" | "=" | ":=" | "<<-") {
            (lhs, true)
        } else if lhs.id() == func.id() && matches!(op, "->" | "->>") {
            (rhs, false)
        } else {
            return None;
        };
    let name = match target.kind() {
        "identifier" => trimmed(cx, target),
        "string" if function_on_right => child_by_field(target, "content")
            .map(|content| trimmed(cx, content))
            .filter(|s| !s.is_empty())?,
        _ => return None,
    };
    Some((name, binary))
}

fn argument_nodes(call: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(call, "arguments").map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|c| c.kind() == "argument")
            .collect()
    })
}

fn argument_values(call: Node<'_>) -> Vec<Node<'_>> {
    argument_nodes(call)
        .into_iter()
        .filter_map(|argument| child_by_field(argument, "value"))
        .collect()
}

fn string_content<'s>(cx: &Cx<'_, 's>, string: Node<'_>) -> Option<&'s str> {
    child_by_field(string, "content").map(|content| text(cx, content))
}

fn param_name<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> &'s str {
    child_by_field(parameter, "name")
        .map_or_else(|| trimmed(cx, parameter), |name| trimmed(cx, name))
}

fn params<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Vec<Param<'s>> {
    child_by_field(func, "parameters").map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|p| p.kind() == "parameter")
            .map(|parameter| Param {
                name: param_name(cx, parameter),
                ty: None,
                bool_default: child_by_field(parameter, "default")
                    .is_some_and(|value| matches!(value.kind(), "true" | "false")),
            })
            .collect()
    })
}

fn body_statements(body: Node<'_>) -> Vec<Node<'_>> {
    match body.kind() {
        "null" => Vec::new(),
        "braced_expression" => named_children(body)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect(),
        _ => vec![body],
    }
}

fn sole_call(body: Option<Node<'_>>) -> Option<usize> {
    let body = body?;
    if body.kind() == "call" {
        return Some(body.id());
    }
    if body.kind() != "braced_expression" {
        return None;
    }
    let statements = body_statements(body);
    match statements.as_slice() {
        [only] if only.kind() == "call" => Some(only.id()),
        _ => None,
    }
}

fn callee_root<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> Option<&'s str> {
    match callee.kind() {
        "identifier" => Some(text(cx, callee)),
        "extract_operator" => callee_root(cx, child_by_field(callee, "lhs")?),
        _ => None,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Vec<Arg<'s>> {
    argument_nodes(call)
        .into_iter()
        .map(|argument| match child_by_field(argument, "value") {
            Some(value) => arg(shape_of(cx, value), value),
            None => arg(ArgShape::Other, argument),
        })
        .collect()
}

fn arg<'s>(shape: ArgShape<'s>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape,
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn shape_of<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "string" => string_arg_shape(cx, value, &["string_content"]),
        "binary_operator" if binary_concat(cx, value, "+", &["string"]) => ArgShape::Concat,
        "identifier" => ArgShape::Ident(trimmed(cx, value)),
        _ => ArgShape::Other,
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    match n.kind() {
        "identifier" => Some((text(cx, n), pos_of(n))),
        "extract_operator" => write_root(cx, child_by_field(n, "lhs")?),
        "subset" | "subset2" => write_root(cx, child_by_field(n, "function")?),
        "call" => write_root(cx, argument_values(n).first().copied()?),
        _ => None,
    }
}

fn handler_statements<'s>(
    cx: &Cx<'_, 's>,
    function: Node<'_>,
    binding: Option<&str>,
) -> Vec<Stmt<'s>> {
    let Some(body) = child_by_field(function, "body") else {
        return Vec::new();
    };
    body_statements(body)
        .into_iter()
        .map(|statement| handler_stmt(cx, statement, binding))
        .collect()
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, statement: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    if statement.kind() != "call" {
        return Stmt::Other;
    }
    if is_rethrow(cx, statement, binding) {
        return Stmt::Rethrow;
    }
    child_by_field(statement, "function").map_or(Stmt::Other, |callee| Stmt::Call {
        callee: trimmed(cx, callee),
        kind: CallKind::Plain,
    })
}

fn is_rethrow(cx: &Cx<'_, '_>, call: Node<'_>, binding: Option<&str>) -> bool {
    let (Some(binding), Some(function)) = (binding, child_by_field(call, "function")) else {
        return false;
    };
    let values = argument_values(call);
    last_segment(trimmed(cx, function), &["::"]) == "stop"
        && matches!(values.as_slice(), [only] if only.kind() == "identifier" && trimmed(cx, *only) == binding)
}

fn computed_lookup(short: &str) -> Option<&'static str> {
    match short {
        "get" => Some("get() with computed name"),
        "get0" => Some("get0() with computed name"),
        "mget" => Some("mget() with computed name"),
        "assign" => Some("assign() with computed name"),
        "match.fun" => Some("match.fun() with computed name"),
        _ => None,
    }
}

fn has_message(cx: &Cx<'_, '_>, arguments: &[Node<'_>], operands: usize) -> bool {
    let positional = arguments
        .iter()
        .filter(|a| child_by_field(**a, "name").is_none())
        .count();
    positional > operands || arguments.iter().any(|a| arg_name_is(cx, *a, "info"))
}

fn arg_name_is(cx: &Cx<'_, '_>, argument: Node<'_>, want: &str) -> bool {
    child_by_field(argument, "name").is_some_and(|name| trimmed(cx, name) == want)
}

fn first_is_comparison(cx: &Cx<'_, '_>, arguments: &[Node<'_>]) -> bool {
    arguments
        .first()
        .and_then(|a| child_by_field(*a, "value"))
        .is_some_and(|value| {
            value.kind() == "binary_operator"
                && matches!(
                    operator_text(cx, value),
                    "==" | "!=" | "<" | ">" | "<=" | ">="
                )
        })
}
