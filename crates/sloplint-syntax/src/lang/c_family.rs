use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, AssertShape, Assertion, Param, Pos, PrimitiveKind,
};
use tree_sitter::Node;

use crate::hooks::{
    Cx, any_named_child, child_by_field, find_named_child, first_named_child,
    first_named_child_matching, named_children, narrow, only_named_child_matching, pos_of, slice,
    text,
};

pub(crate) fn flat(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
pub(crate) fn angles(t: &str) -> &str {
    t.strip_prefix('<')
        .unwrap_or(t)
        .strip_suffix('>')
        .unwrap_or(t)
}
pub(crate) fn word<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kinds: &[&str], words: &[&str]) -> bool {
    any_named_child(n, |c| {
        kinds.contains(&c.kind()) && words.contains(&text(cx, c))
    })
}
pub(crate) fn inner(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "declarator").or_else(|| first_named_child(n))
}
pub(crate) fn unwrap<'n>(n: Node<'n>, wrap: &[&str]) -> Node<'n> {
    let mut current = n;
    loop {
        if !wrap.contains(&current.kind()) {
            break;
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    current
}
pub(crate) fn leaf<'n>(n: Node<'n>, wrap: &[&str]) -> Node<'n> {
    unwrap(
        match n.kind() {
            "init_declarator" => child_by_field(n, "declarator").unwrap_or(n),
            _ => n,
        },
        wrap,
    )
}
pub(crate) fn span<'s>(cx: &Cx<'_, 's>, ty: Node<'_>, end: Node<'_>) -> &'s str {
    let shown = slice(cx.src, ty.start_byte(), end.start_byte()).trim();
    if !shown.is_empty() {
        shown
    } else {
        text(cx, ty)
    }
}
pub(crate) fn param_target<'n>(d: Node<'n>, wrap: &[&str]) -> (Option<Node<'n>>, bool) {
    let mut current = d;
    let mut callable = false;
    loop {
        if current.kind() == "function_declarator" {
            callable = true;
        } else if !wrap.contains(&current.kind()) {
            break;
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    (
        (current.kind() == "identifier").then_some(current),
        callable,
    )
}

pub(crate) struct CDeclarators {
    pub(crate) wrap: &'static [&'static str],
    pub(crate) envs: &'static [&'static str],
}

pub(crate) static C_DECLARATORS: CDeclarators = CDeclarators {
    wrap: &[
        "pointer_declarator",
        "array_declarator",
        "parenthesized_declarator",
        "attributed_declarator",
    ],
    envs: &["getenv", "secure_getenv", "_dupenv_s"],
};

pub(crate) static CPP_DECLARATORS: CDeclarators = CDeclarators {
    wrap: &[
        "reference_declarator",
        "pointer_declarator",
        "array_declarator",
        "parenthesized_declarator",
        "attributed_declarator",
    ],
    envs: &[
        "getenv",
        "std::getenv",
        "::getenv",
        "secure_getenv",
        "_dupenv_s",
    ],
};

pub(crate) fn func_parts<'n>(
    n: Node<'n>,
    decls: &CDeclarators,
    name_kinds: &[&str],
) -> Option<(Node<'n>, Node<'n>)> {
    let fd = unwrap(child_by_field(n, "declarator")?, decls.wrap);
    if fd.kind() != "function_declarator" {
        return None;
    }
    let name = unwrap(child_by_field(fd, "declarator")?, decls.wrap);
    let kind = name.kind();
    if name_kinds.contains(&kind) {
        return Some((fd, name));
    }
    match kind {
        "qualified_identifier" | "template_function" | "template_method" => {
            Some((fd, child_by_field(name, "name")?))
        }
        _ => None,
    }
}

pub(crate) fn params<'s>(
    cx: &Cx<'_, 's>,
    fd: Node<'_>,
    decls: &CDeclarators,
    decl_kinds: &[&str],
) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let Some(list) = child_by_field(fd, "parameters") else {
        return out;
    };
    for c in named_children(list) {
        if !decl_kinds.contains(&c.kind()) {
            continue;
        }
        if let Some(found) = param(cx, c, decls) {
            out.push(found);
        }
    }
    out
}

pub(crate) fn init_value(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    cx.parent()
        .filter(|parent| parent.kind() == "init_declarator")
        .and_then(|parent| child_by_field(parent, "value"))
        .is_some_and(|value| value.id() == n.id())
}

pub(crate) fn primitive_kind_of(ty: Option<&str>) -> PrimitiveKind {
    let text = ty.unwrap_or("");
    if text.bytes().any(|b| b == b'<') {
        return PrimitiveKind::Other;
    }
    let stars = text.bytes().filter(|&b| b == b'*').count();
    let pointer = stars > 0;
    let mut base = "";
    let mut signed = false;
    for tok in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if tok.is_empty() {
            continue;
        }
        if tok == "unsigned" || tok == "signed" {
            signed = true;
        }
        base = tok;
    }
    match base {
        "bool" | "_Bool" | "BOOL" if !pointer => PrimitiveKind::Bool,
        "string" => PrimitiveKind::Str,
        "NSString" if stars <= 1 => PrimitiveKind::Str,
        "char" if pointer && stars == 1 && !signed => PrimitiveKind::Str,
        "short" | "int" | "long" | "signed" | "unsigned" | "size_t" | "ssize_t" | "NSInteger"
        | "NSUInteger" | "int8_t" | "int16_t" | "int32_t" | "int64_t" | "uint8_t" | "uint16_t"
        | "uint32_t" | "uint64_t"
            if !pointer =>
        {
            PrimitiveKind::Int
        }
        "float" | "double" | "CGFloat" if !pointer => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

pub(crate) fn is_simple_callee(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" => return true,
            "field_expression" => match child_by_field(current, "argument") {
                Some(argument) => current = argument,
                None => return false,
            },
            _ => return false,
        }
    }
}

pub(crate) fn defaulted_param<'s>(
    cx: &Cx<'_, 's>,
    p: Node<'_>,
    name: &'s str,
    ty: Option<&'s str>,
) -> Param<'s> {
    Param {
        name,
        ty,
        primitive_kind: primitive_kind_of(ty),
        has_default: child_by_field(p, "default_value").is_some(),
        bool_default: child_by_field(p, "default_value")
            .is_some_and(|d| text(cx, d) == "true" || text(cx, d) == "false"),
        keyword_only: false,
        is_variadic: false,
    }
}

pub(crate) fn param<'s>(cx: &Cx<'_, 's>, p: Node<'_>, decls: &CDeclarators) -> Option<Param<'s>> {
    let ty_node = child_by_field(p, "type")?;
    let declarator = child_by_field(p, "declarator");
    let (named, callable) = declarator.map_or((None, false), |d| param_target(d, decls.wrap));
    let name = named.map(|l| text(cx, l)).unwrap_or("");
    let ty = match (named, declarator) {
        (Some(found), _) if !callable => span(cx, ty_node, found),
        (_, Some(d)) => slice(cx.src, p.start_byte(), d.end_byte()),
        (_, None) => text(cx, ty_node),
    };
    if name.is_empty() && ty == "void" {
        return None;
    }
    Some(defaulted_param(cx, p, name, Some(ty)))
}

pub(crate) fn const_pointer(cx: &Cx<'_, '_>, d: Node<'_>, wrap: &[&str]) -> bool {
    let mut current = match d.kind() {
        "init_declarator" => child_by_field(d, "declarator").unwrap_or(d),
        _ => d,
    };
    let mut innermost_const = false;
    while wrap.contains(&current.kind()) {
        if current.kind() == "pointer_declarator" {
            innermost_const = any_named_child(current, |c| {
                c.kind() == "type_qualifier" && text(cx, c) == "const"
            });
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    innermost_const
}
pub(crate) fn decl_info<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    wrap: &[&str],
) -> (Vec<(&'s str, Pos)>, bool) {
    let mut cursor = n.walk();
    let ds: Vec<Node<'_>> = n
        .children_by_field_name("declarator", &mut cursor)
        .collect();
    let mut out = Vec::new();
    let mut func = false;
    for d in ds {
        let found = leaf(d, wrap);
        if found.kind() == "function_declarator" {
            func = true;
        } else if found.kind() == "identifier" && !const_pointer(cx, d, wrap) {
            out.push((text(cx, found), pos_of(found)));
        }
    }
    (out, func)
}
pub(crate) fn root_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "field_expression" | "subscript_expression" | "pointer_expression" => {
                let next = child_by_field(current, "argument")?;
                current = next
            }
            "parenthesized_expression" => {
                let next = first_named_child(current)?;
                current = next
            }
            _ => return None,
        }
    }
}
pub(crate) fn env_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    envs: &[&str],
) {
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let raw = text(cx, func);
    if raw.bytes().any(|b| b == b'(' || b == b')') {
        return;
    }
    if envs.contains(&raw)
        || raw.bytes().any(|b| b.is_ascii_whitespace()) && envs.contains(&flat(raw).as_str())
    {
        out.push((AmbientKind::EnvRead, raw, pos_of(n)));
    }
}
pub(crate) fn bin_op<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if n.child_count() != 3 {
        return None;
    }
    n.child(1).map(|op| text(cx, op))
}
pub(crate) fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|a| find_named_child(a, |c| c.kind() != "comment"))
}
pub(crate) fn plain_assert(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let first = first_arg(n);
    let mut has_message = false;
    let mut comparison = false;
    if let Some(found) = first.filter(|a| a.kind() == "binary_expression") {
        let op = bin_op(cx, found).unwrap_or("");
        if op == "&&" {
            has_message =
                child_by_field(found, "right").is_some_and(|r| r.kind() == "string_literal");
        }
        comparison = op == "==" || op == "!=";
    }
    let shape = if comparison {
        AssertShape::TopEqNe
    } else if first.is_some_and(|a| a.kind() == "identifier") {
        AssertShape::BareName
    } else {
        AssertShape::Other
    };
    Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}
pub(crate) fn pair_source<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    if pair.kind() != "initializer_pair" {
        return None;
    }
    let value = child_by_field(pair, "value")?;
    if value.kind() != "field_expression" {
        return None;
    }
    let designator = designator_of(cx, pair)?;
    let field = child_by_field(value, "field")?;
    if designator != text(cx, field) {
        return None;
    }
    let arg = child_by_field(value, "argument")?;
    if arg.kind() != "identifier" {
        return None;
    }
    Some(text(cx, arg))
}
pub(crate) fn designator_of<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    for c in named_children(pair) {
        if c.kind() == "field_designator" {
            let found = first_named_child_matching(c, &["field_identifier"])?;
            return Some(text(cx, found));
        }
        if c.kind() == "field_identifier" {
            return Some(text(cx, c));
        }
    }
    None
}
pub(crate) fn in_function(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors().any(|a| a.kind() == "function_definition")
}
pub(crate) fn collect_fragments<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    kinds: &[&str],
    out: &mut Vec<&'s str>,
) {
    for i in 0..n.child_count() {
        if let Some(found) = n.child(i)
            && kinds.contains(&found.kind())
        {
            out.push(text(cx, found));
        }
    }
}
pub(crate) fn sole_call(body: Node<'_>) -> Option<usize> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let stmt = only_named_child_matching(body, |c| c.kind() != "comment")?;
    if stmt.kind() != "expression_statement" && stmt.kind() != "return_statement" {
        return None;
    }
    let expr = child_by_field(stmt, "expression").or_else(|| first_named_child(stmt))?;
    (expr.kind() == "call_expression").then_some(expr.id())
}
pub(crate) fn call_args<'s>(
    cx: &Cx<'_, 's>,
    args_node: Node<'_>,
    shape: fn(&Cx<'_, 's>, Node<'_>) -> ArgShape<'s>,
) -> Vec<Arg<'s>> {
    let mut args = Vec::new();
    for c in named_children(args_node) {
        if c.kind() == "comment" {
            continue;
        }
        args.push(Arg {
            shape: shape(cx, c),
            start_line: narrow(c.start_position().row) + 1,
            end_line: narrow(c.end_position().row) + 1,
        });
    }
    args
}
