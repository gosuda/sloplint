use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, AssertShape, Assertion, CallKind, Handler, Import, ImportKind,
    ImportScope, Pos, Stmt, Visibility,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, binary_concat, child_by_field, find_named_child,
    first_named_child_matching, last_segment, named_children, narrow, only_named_child_matching,
    pos_of, preceding_comments, string_arg_shape, text, trimmed,
};
use crate::spec::Role;

mod ambient;
mod callee;
mod params;

use ambient::{global_assign_target, push_global_binding};
use callee::callee_parts;
use params::{param_name, params};

const R_STDLIB: &[&str] = &[
    "base",
    "compiler",
    "datasets",
    "graphics",
    "grDevices",
    "grid",
    "methods",
    "parallel",
    "splines",
    "stats",
    "stats4",
    "tcltk",
    "tools",
    "utils",
];

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

pub(crate) struct RHooks;

pub(crate) static HOOKS: RHooks = RHooks;

const COMMENT_KINDS: &[&str] = &["comment"];

impl LangHooks for RHooks {
    fn function<'s>(&self, _n: Node<'_>, _cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        None
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        let bound = bound_function(cx, n);
        let (name, name_pos, visibility, doc, doc_pos) = match bound {
            Some((name, binary)) => {
                let (doc, doc_pos) = preceding_comments(cx, binary, COMMENT_KINDS, &[]);
                let visibility = if nested_function(cx) || name.starts_with('.') {
                    Visibility::Private
                } else {
                    Visibility::Exported
                };
                (name, pos_of(binary), visibility, doc, doc_pos)
            }
            None => ("", pos_of(n), Visibility::Private, Vec::new(), None),
        };
        Some(FunctionHead {
            shadow_params: Vec::new(),
            name,
            name_pos,
            visibility,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: None,
            receiver: None,
            params: params(cx, n),
            return_type: None,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(child_by_field(n, "body")),
        })
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|parent| {
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
            _ => None,
        };
        out.extend(target.and_then(|node| write_root(cx, node)));
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        let function = child_by_field(n, "function")?;
        let (callee, root) = callee_parts(cx, function);
        Some(CallHead {
            callee,
            kind: CallKind::Plain,
            callee_root: root,
            is_simple_path: root.is_some(),
            initializes_binding: false,
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
        let Some(first) = first_argument_value(n) else {
            return;
        };
        let module = if first.kind() == "string" {
            string_content(cx, first).unwrap_or("")
        } else {
            trimmed(cx, first)
        };
        let external_root =
            (kind == ImportKind::Wildcard && !R_STDLIB.contains(&module)).then_some(module);
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(module),
            name: module,
            alias: None,
            kind,
            scope: if nested_function(cx) {
                ImportScope::FunctionLocal
            } else {
                ImportScope::Module
            },
            in_test: false,
            is_reexport: false,
            external_root,
        });
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "argument"
            || child_by_field(n, "name").map(|name| trimmed(cx, name)) != Some("error")
        {
            return None;
        }
        let parent = cx.parent()?;
        let call = cx.parent_of(parent).filter(|c| c.kind() == "call")?;
        let function = child_by_field(call, "function")?;
        if last_segment(trimmed(cx, function), &["::"]) != "tryCatch" {
            return None;
        }
        let handler_fn =
            child_by_field(n, "value").filter(|v| v.kind() == "function_definition")?;
        let binding = child_by_field(handler_fn, "parameters")
            .and_then(|list| first_named_child_matching(list, &["parameter"]))
            .map(|parameter| param_name(cx, parameter));
        Some(Handler {
            pos: pos_of(n),
            stmts: handler_statements(cx, handler_fn, binding),
            has_else: false,
            try_imports_only: false,
            in_cleanup: false,
        })
    }

    fn ambient_bind(&self) -> bool {
        true
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "binary_operator" {
            push_global_binding(cx, n, in_function, out);
            return;
        }
        if n.kind() != "call" {
            return;
        }
        let Some(function) = child_by_field(n, "function") else {
            return;
        };
        let name = trimmed(cx, function);
        match last_segment(name, &["::"]) {
            "Sys.getenv" => out.push((AmbientKind::EnvRead, name, pos_of(n))),
            "assign" if in_function => {
                if let Some(target) = global_assign_target(cx, n) {
                    out.push((AmbientKind::GlobalWrite, target, pos_of(n)));
                }
            }
            "Sys.setenv" | "Sys.unsetenv" if in_function => {
                out.push((AmbientKind::GlobalWrite, "os.environ", pos_of(n)));
            }
            _ => {}
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        let short = last_segment(trimmed(cx, child_by_field(n, "function")?), &["::"]);
        let first = first_argument_value(n);
        match short {
            "eval" => Some("eval() of runtime code"),
            "evalq" => Some("evalq() of runtime code"),
            "do.call" => first
                .filter(|v| !matches!(v.kind(), "string" | "identifier"))
                .map(|_| "do.call() with computed function name"),
            "match.fun" => first
                .filter(|v| !matches!(v.kind(), "string" | "identifier"))
                .map(|_| "match.fun() with computed function name"),
            _ => first
                .filter(|v| v.kind() != "string")
                .and_then(|_| computed_lookup(short)),
        }
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        let function = child_by_field(n, "function")?;
        let arguments = || argument_nodes(n);
        let pos = pos_of(n);
        match last_segment(trimmed(cx, function), &["::"]) {
            "stopifnot" => {
                let arguments = arguments();
                Some(Assertion {
                    pos,
                    has_message: arguments
                        .iter()
                        .any(|a| child_by_field(*a, "name").is_some()),
                    comparison: false,
                    shape: stopifnot_shape(cx, &arguments),
                    in_typecheck: false,
                    value_is_bool_literal: false,
                })
            }
            "expect_true" | "expect_false" => {
                let arguments = arguments();
                Some(Assertion {
                    pos,
                    has_message: has_message(cx, &arguments, 1),
                    comparison: first_is_comparison(cx, &arguments),
                    shape: AssertShape::Other,
                    in_typecheck: false,
                    value_is_bool_literal: false,
                })
            }
            "expect_equal" | "expect_identical" => {
                let arguments = arguments();
                Some(Assertion {
                    pos,
                    has_message: has_message(cx, &arguments, 2),
                    comparison: true,
                    shape: AssertShape::Other,
                    in_typecheck: false,
                    value_is_bool_literal: false,
                })
            }
            _ => None,
        }
    }
}

fn operator_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    child_by_field(n, "operator").map_or("", |op| trimmed(cx, op))
}

fn bound_function<'s, 'a>(cx: &Cx<'a, 's>, func: Node<'a>) -> Option<(&'s str, Node<'a>)> {
    let binary = cx.parent().filter(|p| p.kind() == "binary_operator")?;
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

fn nested_function(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors()
        .any(|ancestor| ancestor.kind() == "function_definition")
}

fn stopifnot_shape(cx: &Cx<'_, '_>, arguments: &[Node<'_>]) -> AssertShape {
    if arguments.len() != 1 || child_by_field(arguments[0], "name").is_some() {
        return AssertShape::Other;
    }
    let Some(value) = child_by_field(arguments[0], "value") else {
        return AssertShape::Other;
    };
    if value.kind() == "identifier" {
        return AssertShape::BareName;
    }
    if value.kind() == "binary_operator" && matches!(operator_text(cx, value), "==" | "!=") {
        return AssertShape::TopEqNe;
    }
    AssertShape::Other
}

fn argument_nodes(call: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(call, "arguments").map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|c| c.kind() == "argument")
            .collect()
    })
}

fn string_content<'s>(cx: &Cx<'_, 's>, string: Node<'_>) -> Option<&'s str> {
    child_by_field(string, "content").map(|content| text(cx, content))
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
    let only = only_named_child_matching(body, |c| c.kind() != "comment")?;
    (only.kind() == "call").then_some(only.id())
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

fn str_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let ArgShape::Str { longest_static } = string_arg_shape(cx, n, &["string_content"]) else {
        return 0;
    };
    longest_static
}

fn concat_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut longest = 0;
    let mut current = n;
    while let (Some(left), Some(right)) = (current.child(0), current.child(2)) {
        if right.kind() == "string" {
            longest = longest.max(str_static(cx, right));
        }
        if left.kind() == "string" {
            return longest.max(str_static(cx, left));
        }
        if left.kind() != current.kind() {
            break;
        }
        current = left;
    }
    longest
}

fn shape_of<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "string" => string_arg_shape(cx, value, &["string_content"]),
        "binary_operator" if binary_concat(cx, value, "+", &["string"]) => ArgShape::Concat {
            longest_static: concat_static(cx, value),
        },
        "function_definition" | "braced_expression" => ArgShape::CodeBody,
        "identifier" => ArgShape::Ident(trimmed(cx, value)),
        _ => ArgShape::Other,
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "extract_operator" => child_by_field(current, "lhs")?,
            "subset" | "subset2" => child_by_field(current, "function")?,
            "call" => first_argument_value(current)?,
            _ => return None,
        };
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

fn first_argument_value(call: Node<'_>) -> Option<Node<'_>> {
    let list = child_by_field(call, "arguments")?;
    find_named_child(list, |c| {
        c.kind() == "argument" && child_by_field(c, "value").is_some()
    })
    .and_then(|argument| child_by_field(argument, "value"))
}

fn single_argument_value(call: Node<'_>) -> Option<Node<'_>> {
    let list = child_by_field(call, "arguments")?;
    only_named_child_matching(list, |c| {
        c.kind() == "argument" && child_by_field(c, "value").is_some()
    })
    .and_then(|argument| child_by_field(argument, "value"))
}

fn is_rethrow(cx: &Cx<'_, '_>, call: Node<'_>, binding: Option<&str>) -> bool {
    let (Some(binding), Some(function)) = (binding, child_by_field(call, "function")) else {
        return false;
    };
    last_segment(trimmed(cx, function), &["::"]) == "stop"
        && single_argument_value(call)
            .is_some_and(|only| only.kind() == "identifier" && trimmed(cx, only) == binding)
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
