use std::borrow::Cow;

use sloplint_core::facts::{
    AssertShape, Assertion, CallKind, FieldCopy, Import, ImportKind, ImportScope, Param, Pos,
    PrimitiveKind, Visibility,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, child_by_field, count_named_child, find_named_child,
    first_named_child_matching, ident_root, named_children, narrow, pos_of, preceding_comments,
    text, trimmed,
};
use crate::spec::Role;

mod shapes;

use shapes::{argument_nodes, call_args, is_static_name, name_from_literal_loop};

pub(crate) struct StarlarkHooks;

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
            shadow_params: Vec::new(),
            name,
            name_pos: pos_of(name_node),
            visibility: starlark_visibility(name),
            signature_forced: false,
            entry_point: false,
            in_test: is_starlark_test_path(cx.path),
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: None,
            receiver: None,
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
            callee: starlark_callee(cx, callee),
            kind: CallKind::Plain,
            callee_root: starlark_callee_root(cx, callee),
            is_simple_path: starlark_is_simple_path(callee),
            initializes_binding: starlark_initializes_binding(cx, n),
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
                scope: ImportScope::Module,
                in_test: false,
                is_reexport: false,
                external_root: starlark_external_root(module),
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
        if !matches!(callee, "getattr" | "hasattr") {
            return None;
        }
        let arguments = child_by_field(n, "arguments")?;
        let mut seen = 0;
        let mut cursor = arguments.walk();
        let mut second = None;
        for child in arguments.children(&mut cursor) {
            if !child.is_named() || is_comment(child.kind()) {
                continue;
            }
            if seen == 1 {
                second = Some(child);
                break;
            }
            seen += 1;
        }
        let second = second?;
        if is_static_name(second) {
            return None;
        }
        if second.kind() == "identifier" && name_from_literal_loop(cx, second) {
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
        let first = find_named_child(n, |c| !matches!(c.kind(), "assert_keyword" | "comment"));
        let comparison = first.is_some_and(|found| found.kind() == "comparison_operator");
        let shape = if comparison {
            if first.is_some_and(has_equality_operator) {
                AssertShape::TopEqNe
            } else {
                AssertShape::Other
            }
        } else if first.is_some_and(|found| found.kind() == "identifier") {
            AssertShape::BareName
        } else {
            AssertShape::Other
        };
        Some(Assertion {
            pos: pos_of(n),
            has_message: count_named_child(n, |c| {
                !matches!(c.kind(), "assert_keyword" | "comment")
            }) >= 2,
            comparison,
            shape,
            in_typecheck: false,
            value_is_bool_literal: false,
        })
    }
}

fn has_equality_operator(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| matches!(child.kind(), "==" | "!="))
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
        let mut out = Vec::new();
        let mut keyword_only = false;
        for parameter in named_children(list) {
            if parameter.kind() == "keyword_separator" {
                keyword_only = true;
                continue;
            }
            if parameter.kind() == "positional_separator" {
                keyword_only = false;
                continue;
            }
            if let Some(param) = param_of(cx, parameter, keyword_only) {
                let star = is_star_variadic(parameter);
                out.push(param);
                if star {
                    keyword_only = true;
                }
            }
        }
        out
    })
}

fn is_star_variadic(parameter: Node<'_>) -> bool {
    if parameter.kind() == "list_splat_pattern" {
        return true;
    }
    if parameter.kind() == "typed_parameter" {
        return find_named_child(parameter, |c| c.kind() == "list_splat_pattern").is_some();
    }
    false
}

fn param_of<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>, keyword_only: bool) -> Option<Param<'s>> {
    if parameter.kind() == "identifier" {
        return Some(Param {
            name: trimmed(cx, parameter),
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only,
            is_variadic: false,
        });
    }
    if parameter.kind() == "typed_parameter"
        && let Some(pat) = find_named_child(parameter, |c| {
            c.kind() == "list_splat_pattern" || c.kind() == "dictionary_splat_pattern"
        })
    {
        return Some(Param {
            name: first_identifier(cx, pat).unwrap_or(""),
            ty: type_text(cx, parameter),
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only,
            is_variadic: true,
        });
    }
    let (name, ty, value, is_variadic) = match parameter.kind() {
        "default_parameter" => (
            field_text(cx, parameter, "name"),
            None,
            child_by_field(parameter, "value"),
            false,
        ),
        "typed_parameter" => (
            first_identifier(cx, parameter)?,
            type_text(cx, parameter),
            None,
            false,
        ),
        "typed_default_parameter" => (
            field_text(cx, parameter, "name"),
            type_text(cx, parameter),
            child_by_field(parameter, "value"),
            false,
        ),
        "list_splat_pattern" | "dictionary_splat_pattern" => {
            (first_identifier(cx, parameter)?, None, None, true)
        }
        _ => return None,
    };
    Some(Param {
        name,
        ty,
        primitive_kind: PrimitiveKind::Other,
        has_default: value.is_some(),
        bool_default: value.is_some_and(|v| matches!(v.kind(), "true" | "false")),
        keyword_only,
        is_variadic,
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

fn string_content<'s>(cx: &Cx<'_, 's>, string: Node<'_>) -> Option<&'s str> {
    first_named_child_matching(string, &["string_content"]).map(|content| text(cx, content))
}

fn is_top_level_load(cx: &Cx<'_, '_>, call: Node<'_>) -> bool {
    child_by_field(call, "function").is_some_and(|function| trimmed(cx, function) == "load")
        && cx
            .parent()
            .filter(|p| p.kind() == "expression_statement")
            .is_some_and(|statement| {
                cx.parent_of(statement)
                    .is_some_and(|module| module.kind() == "module")
            })
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
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: starlark_external_root(module),
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
fn starlark_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Exported
    }
}

fn starlark_is_simple_path(callee: Node<'_>) -> bool {
    let mut current = callee;
    loop {
        match current.kind() {
            "identifier" => return true,
            "attribute" => match child_by_field(current, "object") {
                Some(next) => current = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

fn starlark_callee<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> &'s str {
    if starlark_is_simple_path(callee) {
        return trimmed(cx, callee);
    }
    if callee.kind() == "attribute" {
        return child_by_field(callee, "attribute").map_or("", |name| trimmed(cx, name));
    }
    ""
}

fn starlark_callee_root<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> Option<&'s str> {
    if !starlark_is_simple_path(callee) {
        return None;
    }
    ident_root(cx, callee, &["object"]).map(|(name, _)| name)
}
fn starlark_external_root(module: &str) -> Option<&str> {
    let rest = module
        .strip_prefix("@@")
        .or_else(|| module.strip_prefix('@'))?;
    let end = rest.find('/').unwrap_or(rest.len());
    let repo = &rest[..end];
    (!repo.is_empty()).then_some(repo)
}

fn is_starlark_test_path(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment == "test" || segment == "tests" || segment == "__tests__")
        || path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .ends_with("_test.bzl")
}

fn starlark_initializes_binding(cx: &Cx<'_, '_>, call: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "assignment" {
        return false;
    }
    if child_by_field(parent, "right").is_none_or(|right| right.id() != call.id()) {
        return false;
    }
    child_by_field(parent, "left").is_some_and(|left| left.kind() == "identifier")
}
