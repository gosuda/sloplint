//! Types, handlers, arms, literals, and misc facts for JavaScript.

use sloplint_core::facts::{
    AssertShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, ScrutineeKind, Stmt,
    TypeDef, WildcardArm,
};
use tree_sitter::Node;

use super::dynamic::closed_name;
use super::funcs::visibility_of;
use super::imports::import_module;
use crate::hooks::{
    Container, Cx, TypeHead, child_by_field, code_children, count_named_child, find_named_child,
    narrow, pos_of, text,
};

pub(crate) const EQ_NE_OPS: &[&str] = &["==", "===", "!=", "!=="];
pub(crate) const PLAIN_ASSERTS: &[&str] = &["assert", "assert.ok", "console.assert"];
pub(crate) const EQUALITY_ASSERTS: &[&str] = &[
    "assert.equal",
    "assert.notEqual",
    "assert.strictEqual",
    "assert.notStrictEqual",
    "assert.deepEqual",
    "assert.deepStrictEqual",
    "assert.notDeepEqual",
    "assert.notDeepStrictEqual",
];

const JS_EXTENSIONS: &[&str] = &[".js", ".jsx", ".mjs", ".cjs"];

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if n.kind() != "class_declaration" {
        return None;
    }
    let name = child_by_field(n, "name")?;
    let name_text = text(cx, name);
    let public = cx
        .parent_of(n)
        .is_some_and(|parent| parent.kind() == "export_statement")
        || cx.is_exported(name_text);
    let heritage = code_children(n)
        .into_iter()
        .find(|child| child.kind() == "class_heritage")
        .and_then(first_code);
    let impls: Vec<ImplRef<'s>> = heritage
        .and_then(|base| heritage_ref(cx, base))
        .into_iter()
        .collect();
    Some(TypeHead {
        def: Some(TypeDef {
            name: name_text,
            pos: pos_of(name),
            visibility: visibility_of(public),
            abstraction: false,
        }),
        impls,
        container: Some(Container::Class {
            exported: public,
            implements: false,
        }),
    })
}

fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| child.kind() != "comment")
}

fn heritage_ref<'s>(cx: &Cx<'_, 's>, base: Node<'_>) -> Option<ImplRef<'s>> {
    let name = match base.kind() {
        "identifier" => base,
        "member_expression" => child_by_field(base, "property")?,
        _ => return None,
    };
    Some(ImplRef {
        name: text(cx, name),
        pos: pos_of(name),
        is_generic: false,
        child_is_abstract: false,
    })
}

fn join_normalized(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in dir.split('/').chain(rel.split('/')) {
        match segment {
            "" | "." => {}
            ".." => parts.pop().map(|_| ())?,
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn re_export_candidates(dir: &str, source: &str) -> Vec<String> {
    if !(source.starts_with("./") || source.starts_with("../")) {
        return Vec::new();
    }
    let Some(path) = join_normalized(dir, source) else {
        return Vec::new();
    };
    if JS_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
        return vec![path];
    }
    [
        ".js",
        ".jsx",
        ".mjs",
        ".cjs",
        "/index.js",
        "/index.jsx",
        ".ts",
        ".tsx",
        "/index.ts",
        "/index.tsx",
    ]
    .iter()
    .map(|suffix| format!("{path}{suffix}"))
    .collect()
}

pub(crate) fn barrel_of(root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
    if root.kind() != "program" {
        return None;
    }
    let dir = cx.path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut targets = Vec::new();
    for statement in code_children(root) {
        if statement.kind() == "hash_bang_line" {
            continue;
        }
        if statement.kind() != "export_statement" {
            return None;
        }
        let source = import_module(cx, statement)?;
        targets.push(re_export_candidates(dir, source));
    }
    (!targets.is_empty()).then_some(Barrel { targets })
}

pub(crate) fn switch_scrutinee(cx: &Cx<'_, '_>, _n: Node<'_>) -> ScrutineeKind {
    let value = cx
        .ancestors()
        .nth(1)
        .and_then(|stmt| child_by_field(stmt, "value"));
    match scrutinee_root(value).map(|node| node.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("member_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}

fn scrutinee_root(mut value: Option<Node<'_>>) -> Option<Node<'_>> {
    loop {
        let node = value?;
        if node.kind() == "parenthesized_expression" {
            value = find_named_child(node, |child| child.kind() != "comment");
        } else {
            return Some(node);
        }
    }
}

pub(crate) fn match_arm_default(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "switch_default" {
        return None;
    }
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: false,
        scrutinee: switch_scrutinee(cx, n),
        variant_count: 0,
        has_nullable_ctor: false,
    })
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    let Some(inner) = first_code(stmt) else {
        return Stmt::Other;
    };
    if stmt.kind() == "expression_statement" && inner.kind() == "call_expression" {
        return call_stmt(cx, inner);
    }
    if stmt.kind() == "throw_statement"
        && inner.kind() == "identifier"
        && binding == Some(text(cx, inner))
    {
        return Stmt::Rethrow;
    }
    Stmt::Other
}

fn call_stmt<'s>(cx: &Cx<'_, 's>, inner: Node<'_>) -> Stmt<'s> {
    match child_by_field(inner, "function") {
        Some(function) => Stmt::Call {
            callee: text(cx, function),
            kind: CallKind::Plain,
        },
        None => Stmt::Other,
    }
}

fn try_imports_only(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let body = cx
        .parent_of(n)
        .filter(|parent| parent.kind() == "try_statement")
        .and_then(|parent| child_by_field(parent, "body"));
    let Some(body) = body else {
        return false;
    };
    let mut seen = false;
    for stmt in code_children(body) {
        if stmt.kind() != "import_statement" {
            return false;
        }
        seen = true;
    }
    seen
}

pub(crate) fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let binding = child_by_field(n, "parameter")
        .filter(|parameter| parameter.kind() == "identifier")
        .map(|parameter| text(cx, parameter));
    let body = child_by_field(n, "body")?;
    let stmts = code_children(body)
        .into_iter()
        .map(|stmt| handler_stmt(cx, stmt, binding))
        .collect();
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: try_imports_only(cx, n),
        in_cleanup: false,
    })
}

fn pair_copy<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    let key = child_by_field(pair, "key").filter(|key| key.kind() == "property_identifier")?;
    let value =
        child_by_field(pair, "value").filter(|value| value.kind() == "member_expression")?;
    let object = child_by_field(value, "object").filter(|object| object.kind() == "identifier")?;
    let property = child_by_field(value, "property")?;
    (text(cx, property) == text(cx, key)).then(|| text(cx, object))
}

fn object_source<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut found: Option<&'s str> = None;
    for child in code_children(n) {
        if child.kind() == "spread_element" || child.kind() == "comment" {
            continue;
        }
        if child.kind() != "pair" {
            return None;
        }
        let object = pair_copy(cx, child)?;
        if found.is_some_and(|known| known != object) {
            return None;
        }
        found = Some(object);
    }
    found
}

pub(crate) fn struct_lit_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "object" {
        return None;
    }
    let fields = count_named_child(n, |child| child.kind() == "pair");
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(fields),
        source: object_source(cx, n),
    })
}

const EXPRESSION_WRAPPERS: &[&str] = &[
    "parenthesized_expression",
    "non_null_expression",
    "as_expression",
    "satisfies_expression",
];

const UNWRAP_CAP: usize = 16;

fn wrapped_expression(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "sequence_expression" => code_children(node).pop(),
        kind if EXPRESSION_WRAPPERS.contains(&kind) => first_code(node),
        _ => None,
    }
}

pub(super) fn unwrap_expression(mut node: Node<'_>) -> Option<Node<'_>> {
    for _ in 0..UNWRAP_CAP {
        let Some(inner) = wrapped_expression(node) else {
            return Some(node);
        };
        node = inner;
    }
    None
}

fn subscript_call(cx: &Cx<'_, '_>, function: Node<'_>) -> Option<&'static str> {
    let Some(index) = unwrap_expression(child_by_field(function, "index")?) else {
        return Some("computed method call");
    };
    (!matches!(index.kind(), "string" | "number") && !closed_name(cx, index))
        .then_some("computed method call")
}

fn call_dynamic(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    let Some(function) = unwrap_expression(child_by_field(n, "function")?) else {
        return Some("computed method call");
    };
    match function.kind() {
        "subscript_expression" => subscript_call(cx, function),
        "identifier" => (text(cx, function) == "eval").then_some("eval"),
        "member_expression" => host_member(cx, function, "eval").then_some("eval"),
        "import" => {
            let first = child_by_field(n, "arguments").and_then(first_code)?;
            (first.kind() != "string" && !closed_name(cx, first)).then_some("dynamic import")
        }
        _ => None,
    }
}

pub(crate) fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    match n.kind() {
        "call_expression" => call_dynamic(cx, n),
        "new_expression" => {
            let constructor = child_by_field(n, "constructor")?;
            function_constructor(cx, constructor).then_some("Function constructor")
        }
        _ => None,
    }
}

const GLOBAL_HOSTS: &[&str] = &["globalThis", "window", "self", "global"];

fn host_member(cx: &Cx<'_, '_>, member: Node<'_>, name: &str) -> bool {
    child_by_field(member, "object")
        .and_then(unwrap_expression)
        .is_some_and(|object| GLOBAL_HOSTS.contains(&text(cx, object)))
        && child_by_field(member, "property").is_some_and(|property| text(cx, property) == name)
}

fn function_constructor(cx: &Cx<'_, '_>, constructor: Node<'_>) -> bool {
    match constructor.kind() {
        "identifier" => text(cx, constructor) == "Function",
        "member_expression" => host_member(cx, constructor, "Function"),
        _ => false,
    }
}

fn comparison_arg(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "binary_expression"
        && child_by_field(n, "operator")
            .is_some_and(|operator| EQ_NE_OPS.contains(&text(cx, operator)))
}

fn bare_or_other(args: &[Node<'_>]) -> AssertShape {
    if args
        .first()
        .is_some_and(|first| first.kind() == "identifier")
    {
        return AssertShape::BareName;
    }
    AssertShape::Other
}

fn bool_literal_arg(args: &[Node<'_>]) -> bool {
    args.first()
        .is_some_and(|first| matches!(first.kind(), "true" | "false"))
}

pub(crate) fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = text(cx, child_by_field(n, "function")?);
    if !PLAIN_ASSERTS.contains(&callee) && !EQUALITY_ASSERTS.contains(&callee) {
        return None;
    }
    let args: Vec<Node<'_>> = child_by_field(n, "arguments")
        .filter(|arguments| arguments.kind() == "arguments")
        .map(code_children)
        .unwrap_or_default();
    if PLAIN_ASSERTS.contains(&callee) {
        return Some(plain_assertion(cx, n, &args));
    }
    EQUALITY_ASSERTS
        .contains(&callee)
        .then(|| equality_assertion(n, &args))
}

fn plain_assertion<'s>(cx: &Cx<'_, 's>, n: Node<'_>, args: &[Node<'_>]) -> Assertion {
    let comparison = args.first().is_some_and(|first| comparison_arg(cx, *first));
    Assertion {
        pos: pos_of(n),
        has_message: args.len() >= 2,
        comparison,
        shape: if comparison {
            AssertShape::TopEqNe
        } else {
            bare_or_other(args)
        },
        in_typecheck: false,
        value_is_bool_literal: bool_literal_arg(args),
    }
}

fn equality_assertion(n: Node<'_>, args: &[Node<'_>]) -> Assertion {
    Assertion {
        pos: pos_of(n),
        has_message: args.len() >= 3,
        comparison: true,
        shape: AssertShape::TopEqNe,
        in_typecheck: false,
        value_is_bool_literal: bool_literal_arg(args),
    }
}
