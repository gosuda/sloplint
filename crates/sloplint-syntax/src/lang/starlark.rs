use std::borrow::Cow;

use sloplint_core::facts::{
    Arg, ArgShape, Assertion, CallKind, FieldCopy, Import, ImportKind, Param, Pos,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, binary_concat, child_by_field, count_named_child,
    find_named_child, first_named_child_matching, ident_root, named_children, narrow, pos_of,
    preceding_comments, string_arg_shape, text, trimmed,
};
use crate::spec::Role;

/// Starlark (Bazel dialect) adapter hooks: functions, `load` imports, and calls.
pub(crate) struct StarlarkHooks;

/// Starlark node kinds mapped to walker roles.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("lambda", Role::Closure),
    ("if_statement", Role::If),
    ("elif_clause", Role::ElseIf),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("conditional_expression", Role::Ternary),
    ("call", Role::Call),
    ("assignment", Role::Bind),
    ("augmented_assignment", Role::Write),
    ("comment", Role::Comment),
    ("assert_statement", Role::Assert),
];

/// The Starlark adapter instance the grammar table points at.
pub(crate) static HOOKS: StarlarkHooks = StarlarkHooks;

const COMMENT_KINDS: &[&str] = &["comment"];

impl LangHooks for StarlarkHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let name_node = child_by_field(n, "name")?;
        let body = child_by_field(n, "body");
        let (mut doc, comment_pos) = preceding_comments(cx, n, COMMENT_KINDS, &[]);
        let docstring = body
            .and_then(docstring_statement)
            .and_then(|s| s.named_child(0));
        let doc_pos = match (comment_pos, docstring) {
            (Some(comment), Some(string)) => Some(comment.min(pos_of(string))),
            (comment, string) => comment.or(string.map(pos_of)),
        };
        doc.extend(docstring.map(|string| trimmed(cx, string)));
        let name = trimmed(cx, name_node);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public: !name.starts_with('_'),
            interface_bound: false,
            in_test: cx.is_test_file,
            params: params(cx, n),
            return_type: child_by_field(n, "return_type").map(|ty| trimmed(cx, ty)),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
    }

    fn is_else_if(&self, _n: Node<'_>, _parent: Option<Node<'_>>) -> bool {
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if let Some(left) = child_by_field(n, "left") {
            collect_identifiers(cx, left, out);
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        match n.kind() {
            "augmented_assignment" => out.extend(
                child_by_field(n, "left")
                    .and_then(|left| ident_root(cx, left, &["object", "value"])),
            ),
            "assignment" => {
                let targets = child_by_field(n, "left").map_or_else(Vec::new, write_targets);
                out.extend(
                    targets
                        .into_iter()
                        .filter_map(|target| ident_root(cx, target, &["object", "value"])),
                );
            }
            _ => {}
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        let callee = child_by_field(n, "function")?;
        Some(CallHead {
            callee: trimmed(cx, callee),
            kind: CallKind::Plain,
            callee_root: ident_root(cx, callee, &["object"]).map(|(name, _)| name),
            args: call_args(cx, n),
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if !is_top_level_load(cx, n) {
            return;
        }
        let arguments = argument_nodes(n);
        let Some((first, rest)) = arguments.split_first() else {
            return;
        };
        if first.kind() != "string" {
            return;
        }
        let module = string_content(cx, *first).unwrap_or("");
        let pos = pos_of(n);
        let symbols: Vec<Import<'s>> = rest
            .iter()
            .filter_map(|argument| symbol_import(cx, module, *argument, pos))
            .collect();
        if symbols.is_empty() {
            out.push(Import {
                pos,
                module: Cow::Borrowed(module),
                name: module,
                alias: None,
                kind: ImportKind::SideEffect,
                in_test: cx.is_test_file,
            });
        } else {
            out.extend(symbols);
        }
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if trimmed(cx, child_by_field(n, "function")?) != "struct" {
            return None;
        }
        let arguments = argument_nodes(n);
        if arguments.is_empty() || arguments.iter().any(|a| a.kind() != "keyword_argument") {
            return None;
        }
        let sources: Option<Vec<&'s str>> =
            arguments.iter().map(|a| keyword_source(cx, *a)).collect();
        let source = sources.and_then(|list| {
            let first = *list.first()?;
            list.iter().all(|s| *s == first).then_some(first)
        });
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(arguments.len()),
            source,
        })
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        let callee = trimmed(cx, child_by_field(n, "function")?);
        let second = argument_nodes(n).get(1).copied()?;
        if second.kind() == "string" {
            return None;
        }
        match callee {
            "getattr" => Some("getattr() with computed name"),
            "hasattr" => Some("hasattr() with computed name"),
            _ => None,
        }
    }

    fn assertion(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "assert_statement" {
            return None;
        }
        Some(Assertion {
            pos: pos_of(n),
            has_message: count_named_child(n, |c| {
                !matches!(c.kind(), "assert_keyword" | "comment")
            }) >= 2,
            comparison: find_named_child(n, |c| !matches!(c.kind(), "assert_keyword" | "comment"))
                .is_some_and(|first| first.kind() == "comparison_operator"),
        })
    }
}

fn is_comment(kind: &str) -> bool {
    kind == "comment"
}

fn is_docstring_statement(statement: Node<'_>) -> bool {
    statement.kind() == "expression_statement"
        && statement.named_child_count() == 1
        && statement
            .named_child(0)
            .is_some_and(|child| child.kind() == "string")
}

fn docstring_statement(body: Node<'_>) -> Option<Node<'_>> {
    find_named_child(body, |c| !is_comment(c.kind())).filter(|c| is_docstring_statement(*c))
}

fn sole_call(body: Option<Node<'_>>) -> Option<usize> {
    let body = body?;
    let count = u32::try_from(body.named_child_count()).unwrap_or(u32::MAX);
    let mut statements = (0..count)
        .filter_map(|i| body.named_child(i))
        .filter(|c| !is_comment(c.kind()));
    let first = statements.next()?;
    let only = if is_docstring_statement(first) {
        statements.next()?
    } else {
        first
    };
    if statements.next().is_some() {
        return None;
    }
    let call = match only.kind() {
        "return_statement" | "expression_statement" => only.named_child(0)?,
        _ => return None,
    };
    (call.kind() == "call").then(|| call.id())
}

fn params<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Vec<Param<'s>> {
    child_by_field(func, "parameters").map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter_map(|parameter| param_of(cx, parameter))
            .collect()
    })
}

fn param_of<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> Option<Param<'s>> {
    let (name, ty, value) = match parameter.kind() {
        "identifier" => {
            return Some(Param {
                name: trimmed(cx, parameter),
                ty: None,
                bool_default: false,
            });
        }
        "default_parameter" => (
            field_text(cx, parameter, "name"),
            None,
            child_by_field(parameter, "value"),
        ),
        "typed_parameter" => (
            first_identifier(cx, parameter)?,
            type_text(cx, parameter),
            None,
        ),
        "typed_default_parameter" => (
            field_text(cx, parameter, "name"),
            type_text(cx, parameter),
            child_by_field(parameter, "value"),
        ),
        "list_splat_pattern" | "dictionary_splat_pattern" => {
            (first_identifier(cx, parameter)?, None, None)
        }
        _ => return None,
    };
    Some(Param {
        name,
        ty,
        bool_default: value.is_some_and(|v| matches!(v.kind(), "true" | "false")),
    })
}

fn field_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> &'s str {
    child_by_field(n, field).map_or("", |child| trimmed(cx, child))
}

fn type_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    child_by_field(n, "type").map(|ty| trimmed(cx, ty))
}

fn first_identifier<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    first_named_child_matching(n, &["identifier"]).map(|identifier| text(cx, identifier))
}

fn collect_identifiers<'s>(cx: &Cx<'_, 's>, target: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending = vec![target];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "identifier" => out.push((text(cx, node), pos_of(node))),
            "pattern_list" | "tuple_pattern" | "list_pattern" => {
                pending.extend(named_children(node).into_iter().rev());
            }
            _ => {}
        }
    }
}

fn write_targets(left: Node<'_>) -> Vec<Node<'_>> {
    match left.kind() {
        "attribute" | "subscript" => vec![left],
        "pattern_list" | "tuple_pattern" | "list_pattern" => named_children(left)
            .into_iter()
            .filter(|c| matches!(c.kind(), "attribute" | "subscript"))
            .collect(),
        _ => Vec::new(),
    }
}

fn argument_nodes(call: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(call, "arguments").map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|c| !is_comment(c.kind()))
            .collect()
    })
}

fn call_args<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Vec<Arg<'s>> {
    argument_nodes(call)
        .into_iter()
        .map(|argument| {
            let value = if argument.kind() == "keyword_argument" {
                child_by_field(argument, "value")
            } else {
                Some(argument)
            };
            match value {
                Some(value) => arg(shape_of(cx, value), value),
                None => arg(ArgShape::Other, argument),
            }
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
        "concatenated_string" => ArgShape::Concat,
        "binary_operator" if binary_concat(cx, value, "+", &["string", "concatenated_string"]) => {
            ArgShape::Concat
        }
        "identifier" => ArgShape::Ident(trimmed(cx, value)),
        _ => ArgShape::Other,
    }
}

fn string_content<'s>(cx: &Cx<'_, 's>, string: Node<'_>) -> Option<&'s str> {
    first_named_child_matching(string, &["string_content"]).map(|content| text(cx, content))
}

fn is_top_level_load(cx: &Cx<'_, '_>, call: Node<'_>) -> bool {
    child_by_field(call, "function").is_some_and(|function| trimmed(cx, function) == "load")
        && call
            .parent()
            .filter(|p| p.kind() == "expression_statement")
            .and_then(|statement| statement.parent())
            .is_some_and(|module| module.kind() == "module")
}

fn symbol_import<'s>(
    cx: &Cx<'_, 's>,
    module: &'s str,
    argument: Node<'_>,
    pos: Pos,
) -> Option<Import<'s>> {
    let (name, alias) = match argument.kind() {
        "string" => (string_content(cx, argument)?, None),
        "keyword_argument" => {
            let label = trimmed(cx, child_by_field(argument, "name")?);
            let value = child_by_field(argument, "value").filter(|v| v.kind() == "string")?;
            let original = string_content(cx, value)?;
            (original, (label != original).then_some(label))
        }
        _ => return None,
    };
    Some(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias,
        kind: ImportKind::Named,
        in_test: cx.is_test_file,
    })
}

fn keyword_source<'s>(cx: &Cx<'_, 's>, argument: Node<'_>) -> Option<&'s str> {
    let label = trimmed(cx, child_by_field(argument, "name")?);
    let value = child_by_field(argument, "value")?;
    if value.kind() != "attribute" {
        return None;
    }
    let object = child_by_field(value, "object").filter(|o| o.kind() == "identifier")?;
    let member = trimmed(cx, child_by_field(value, "attribute")?);
    (member == label).then(|| text(cx, object))
}
