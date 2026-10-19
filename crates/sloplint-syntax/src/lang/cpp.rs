use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    has_child_kind, named_children, narrow, pos_of, preceding_comments, slice, string_arg_shape,
    strip_quotes, text, trimmed,
};
use crate::spec::Role;

/// Role table for C++.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("switch_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("for_range_loop", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("new_expression", Role::NewCall),
    ("declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("preproc_include", Role::Import),
    ("using_declaration", Role::Import),
    ("namespace_alias_definition", Role::Import),
    ("class_specifier", Role::TypeDef),
    ("struct_specifier", Role::TypeDef),
    ("union_specifier", Role::TypeDef),
    ("enum_specifier", Role::TypeDef),
    ("type_definition", Role::TypeDef),
    ("alias_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("case_statement", Role::MatchArm),
    ("initializer_list", Role::StructLit),
    ("field_expression", Role::Member),
    ("subscript_expression", Role::Subscript),
    ("preproc_def", Role::Global),
    ("preproc_function_def", Role::Global),
    ("static_assert_declaration", Role::Assert),
];
/// Hooks for C++.
pub(crate) struct CppHooks;
/// Hooks instance for C++.
pub(crate) static HOOKS: CppHooks = CppHooks;
const WRAP: &[&str] = &[
    "reference_declarator",
    "pointer_declarator",
    "array_declarator",
    "parenthesized_declarator",
    "attributed_declarator",
];
const ENVS: &[&str] = &[
    "getenv",
    "std::getenv",
    "::getenv",
    "secure_getenv",
    "_dupenv_s",
];
impl LangHooks for CppHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let (fd, name_node) = func_parts(n)?;
        let header = n
            .parent()
            .filter(|p| p.kind() == "template_declaration")
            .unwrap_or(n);
        let (doc, doc_pos) = preceding_comments(cx, header, &["comment"], &[]);
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public: is_public(cx, n, member_list(n)),
            interface_bound: is_bound(cx, n, fd),
            in_test: cx.is_test_file,
            params: params(cx, fd),
            return_type: return_type(cx, n, fd),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
    }
    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent().is_some_and(|p| p.kind() == "else_clause")
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "declaration" {
            return;
        }
        if word(
            cx,
            n,
            &["type_qualifier", "storage_class_specifier"],
            &["const", "constexpr", "extern"],
        ) {
            return;
        }
        out.extend(decl_info(cx, n).0);
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let target = match n.kind() {
            "assignment_expression" => child_by_field(n, "left"),
            "update_expression" => child_by_field(n, "argument"),
            _ => None,
        };
        if let Some(root) = target.and_then(|t| root_of(cx, t)) {
            out.push(root);
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        match n.kind() {
            "call_expression" => {
                let func = child_by_field(n, "function")?;
                let args_node = child_by_field(n, "arguments")?;
                Some(CallHead {
                    callee: text(cx, func),
                    kind: CallKind::Plain,
                    callee_root: callee_root(cx, func),
                    args: call_args(cx, args_node),
                })
            }
            "new_expression" => {
                let ty = child_by_field(n, "type")?;
                let args = child_by_field(n, "arguments")
                    .map(|a| call_args(cx, a))
                    .unwrap_or_default();
                Some(CallHead {
                    callee: text(cx, ty),
                    kind: CallKind::New,
                    callee_root: None,
                    args,
                })
            }
            _ => None,
        }
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        match n.kind() {
            "preproc_include" => push_include(cx, n, out),
            "using_declaration" => push_using(cx, n, out),
            "namespace_alias_definition" => {
                let _ = push_alias(cx, n, out);
            }
            _ => {}
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "class_specifier" | "struct_specifier" => class_typedef(cx, n),
            "union_specifier" | "enum_specifier" => named_typedef(cx, n),
            "type_definition" => alias_typedef(cx, n),
            "alias_declaration" => named_typedef(cx, n),
            _ => None,
        }
    }
    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if child_by_field(n, "value").is_some() {
            return None;
        }
        let mut enum_like = false;
        if let Some(parent) = n.parent() {
            for sib in named_children(parent) {
                if sib.kind() == "case_statement"
                    && child_by_field(sib, "value").is_some_and(|v| {
                        v.kind() == "identifier" || v.kind() == "qualified_identifier"
                    })
                {
                    enum_like = true;
                    break;
                }
            }
        }
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like,
        })
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let bound = catch_binding(cx, n);
        let body = child_by_field(n, "body")?;
        let mut stmts = Vec::new();
        for stmt in named_children(body) {
            if stmt.kind() == "comment" {
                continue;
            }
            stmts.push(catch_stmt(cx, stmt, bound));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "initializer_list" {
            return None;
        }
        let kids: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect();
        if kids.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        let mut consistent = true;
        for c in &kids {
            match pair_source(cx, *c) {
                Some(found) if source.is_none() => source = Some(found),
                Some(found) if source.is_some_and(|prev| prev == found) => {}
                _ => {
                    consistent = false;
                    break;
                }
            }
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(kids.len()),
            source: consistent.then_some(source).flatten(),
        })
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        match n.kind() {
            "declaration" if !in_function => global_ambient(cx, n, out),
            "call_expression" => env_ambient(cx, n, out),
            _ => {}
        }
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        match n.kind() {
            "preproc_def" | "preproc_function_def" => {
                let value = child_by_field(n, "value")?;
                text(cx, value)
                    .contains("##")
                    .then_some("token-pasting macro")
            }
            "call_expression" => {
                let func = child_by_field(n, "function")?;
                match flat(text(cx, func)).as_str() {
                    "dlsym" | "dlvsym" => Some("runtime symbol lookup (dlsym)"),
                    "GetProcAddress" => Some("runtime symbol lookup (GetProcAddress)"),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() == "static_assert_declaration" {
            let has_message = child_by_field(n, "message").is_some();
            let condition = child_by_field(n, "condition").filter(|c| {
                c.kind() == "binary_expression"
                    && bin_op(cx, *c).is_some_and(|op| op == "==" || op == "!=")
            });
            return Some(Assertion {
                pos: pos_of(n),
                has_message,
                comparison: condition.is_some(),
            });
        }
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let callee = text(cx, func);
        if callee == "assert" {
            return Some(plain_assert(cx, n));
        }
        if callee.starts_with("ASSERT_") || callee.starts_with("EXPECT_") {
            return Some(gtest_assert(cx, n, callee));
        }
        matches!(
            callee,
            "REQUIRE" | "CHECK" | "REQUIRE_FALSE" | "CHECK_FALSE"
        )
        .then(|| catch2_assert(cx, n))
    }
}
fn flat(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn angles(t: &str) -> &str {
    t.strip_prefix('<')
        .unwrap_or(t)
        .strip_suffix('>')
        .unwrap_or(t)
}
fn word<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kinds: &[&str], words: &[&str]) -> bool {
    named_children(n)
        .iter()
        .any(|c| kinds.contains(&c.kind()) && words.contains(&text(cx, *c)))
}
fn inner(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "declarator").or_else(|| named_children(n).into_iter().next())
}
fn unwrap(n: Node<'_>) -> Node<'_> {
    let mut current = n;
    loop {
        if !WRAP.contains(&current.kind()) {
            break;
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    current
}
fn leaf(n: Node<'_>) -> Node<'_> {
    unwrap(match n.kind() {
        "init_declarator" => child_by_field(n, "declarator").unwrap_or(n),
        _ => n,
    })
}
fn func_parts(n: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    let fd = unwrap(child_by_field(n, "declarator")?);
    if fd.kind() != "function_declarator" {
        return None;
    }
    let inner_node = child_by_field(fd, "declarator")?;
    let name = match inner_node.kind() {
        "identifier" | "field_identifier" | "destructor_name" => inner_node,
        "qualified_identifier" | "template_function" | "template_method" => {
            child_by_field(inner_node, "name")?
        }
        _ => return None,
    };
    Some((fd, name))
}
fn member_list(n: Node<'_>) -> Option<Node<'_>> {
    let parent = n.parent()?;
    if parent.kind() == "field_declaration_list" {
        return Some(parent);
    }
    if parent.kind() == "template_declaration" {
        return parent
            .parent()
            .filter(|g| g.kind() == "field_declaration_list");
    }
    None
}
fn is_public<'s>(cx: &Cx<'_, 's>, n: Node<'_>, list: Option<Node<'_>>) -> bool {
    if in_anonymous(n) {
        return false;
    }
    let Some(items) = list else {
        return !word(cx, n, &["storage_class_specifier"], &["static"]);
    };
    let anchor = n
        .parent()
        .filter(|p| p.kind() == "template_declaration")
        .unwrap_or(n);
    let mut access: Option<&str> = None;
    let mut sib = anchor.prev_sibling();
    while let Some(found) = sib {
        if found.kind() == "access_specifier" {
            access = Some(trimmed(cx, found));
            break;
        }
        sib = found.prev_sibling();
    }
    match access {
        Some(spec) => spec.starts_with("public"),
        None => items
            .parent()
            .is_some_and(|g| g.kind() == "struct_specifier"),
    }
}
fn in_anonymous(n: Node<'_>) -> bool {
    let mut parent = n.parent();
    while let Some(found) = parent {
        if found.kind() == "namespace_definition" && child_by_field(found, "name").is_none() {
            return true;
        }
        parent = found.parent();
    }
    false
}
fn in_function(n: Node<'_>) -> bool {
    let mut parent = n.parent();
    while let Some(found) = parent {
        if found.kind() == "function_definition" {
            return true;
        }
        parent = found.parent();
    }
    false
}
fn is_bound<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> bool {
    if has_child_kind(fd, "virtual_specifier") {
        return true;
    }
    word(cx, n, &["storage_class_specifier"], &["virtual"])
        || word(cx, fd, &["storage_class_specifier"], &["virtual"])
}
fn span<'s>(cx: &Cx<'_, 's>, ty: Node<'_>, end: Node<'_>) -> &'s str {
    let shown = slice(cx.src, ty.start_byte(), end.start_byte()).trim();
    if !shown.is_empty() {
        shown
    } else {
        text(cx, ty)
    }
}
fn params<'s>(cx: &Cx<'_, 's>, fd: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let Some(list) = child_by_field(fd, "parameters") else {
        return out;
    };
    for c in named_children(list) {
        if c.kind() != "parameter_declaration" && c.kind() != "optional_parameter_declaration" {
            continue;
        }
        if let Some(found) = param(cx, c) {
            out.push(found);
        }
    }
    out
}
fn param_target(d: Node<'_>) -> (Option<Node<'_>>, bool) {
    let mut current = d;
    let mut callable = false;
    loop {
        if current.kind() == "function_declarator" {
            callable = true;
        } else if !WRAP.contains(&current.kind()) {
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
fn param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let ty_node = child_by_field(p, "type")?;
    let declarator = child_by_field(p, "declarator");
    let (named, callable) = declarator.map_or((None, false), param_target);
    let name = named.map(|l| text(cx, l)).unwrap_or("");
    let ty = match (named, declarator) {
        (Some(found), _) if !callable => span(cx, ty_node, found),
        (_, Some(d)) => slice(cx.src, p.start_byte(), d.end_byte()),
        (_, None) => text(cx, ty_node),
    };
    if name.is_empty() && ty == "void" {
        return None;
    }
    let default = child_by_field(p, "default_value")
        .is_some_and(|d| text(cx, d) == "true" || text(cx, d) == "false");
    Some(Param {
        name,
        ty: Some(ty),
        bool_default: default,
    })
}
fn return_type<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    if let Some(trailing) = child_by_field(n, "trailing_return_type") {
        return child_by_field(trailing, "type_descriptor").map(|t| text(cx, t));
    }
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
}
fn sole_call(body: Node<'_>) -> Option<usize> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let mut stmts = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment");
    let stmt = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    if stmt.kind() != "expression_statement" && stmt.kind() != "return_statement" {
        return None;
    }
    let expr =
        child_by_field(stmt, "expression").or_else(|| named_children(stmt).into_iter().next())?;
    (expr.kind() == "call_expression").then_some(expr.id())
}
fn const_pointer(cx: &Cx<'_, '_>, d: Node<'_>) -> bool {
    let mut current = match d.kind() {
        "init_declarator" => child_by_field(d, "declarator").unwrap_or(d),
        _ => d,
    };
    let mut innermost_const = false;
    while WRAP.contains(&current.kind()) {
        if current.kind() == "pointer_declarator" {
            innermost_const = named_children(current)
                .iter()
                .any(|c| c.kind() == "type_qualifier" && text(cx, *c) == "const");
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    innermost_const
}
fn decl_info<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<(&'s str, Pos)>, bool) {
    let mut cursor = n.walk();
    let ds: Vec<Node<'_>> = n
        .children_by_field_name("declarator", &mut cursor)
        .collect();
    let mut out = Vec::new();
    let mut func = false;
    for d in ds {
        let found = leaf(d);
        if found.kind() == "function_declarator" {
            func = true;
        } else if found.kind() == "identifier" && !const_pointer(cx, d) {
            out.push((text(cx, found), pos_of(found)));
        }
    }
    (out, func)
}
fn root_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "field_expression" | "subscript_expression" | "pointer_expression" => {
                let next = child_by_field(current, "argument")?;
                current = next
            }
            "parenthesized_expression" => {
                let next = named_children(current).into_iter().next()?;
                current = next
            }
            _ => return None,
        }
    }
}
fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "this" | "base" => return Some(text(cx, current)),
            "field_expression" => {
                let next = child_by_field(current, "argument")?;
                current = next
            }
            "member_access_expression" => {
                let next = child_by_field(current, "expression")?;
                current = next
            }
            _ => return None,
        }
    }
}
fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    let mut args = Vec::new();
    for c in named_children(args_node) {
        if c.kind() == "comment" {
            continue;
        }
        args.push(Arg {
            shape: arg_shape(cx, c),
            start_line: narrow(c.start_position().row) + 1,
            end_line: narrow(c.end_position().row) + 1,
        });
    }
    args
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "raw_string_literal" => {
            let mut segments = Vec::new();
            collect_fragments(cx, n, &["raw_string_content"], &mut segments);
            ArgShape::Str {
                longest_static: longest_static_chunk(&segments),
            }
        }
        "concatenated_string" => concat_shape(cx, n),
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}
fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let kids = named_children(n);
    if kids.iter().any(|c| c.kind() == "identifier") {
        return ArgShape::Concat;
    }
    let mut segments = Vec::new();
    for c in kids {
        if c.kind() != "string_literal" && c.kind() != "raw_string_literal" {
            return ArgShape::Other;
        }
        collect_fragments(
            cx,
            c,
            &["string_content", "raw_string_content"],
            &mut segments,
        );
    }
    ArgShape::Str {
        longest_static: longest_static_chunk(&segments),
    }
}
fn collect_fragments<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kinds: &[&str], out: &mut Vec<&'s str>) {
    for i in 0..n.child_count() {
        if let Some(found) = n.child(i)
            && kinds.contains(&found.kind())
        {
            out.push(text(cx, found));
        }
    }
}
fn push_include<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(path) = child_by_field(n, "path") else {
        return;
    };
    let module = match path.kind() {
        "string_literal" => strip_quotes(text(cx, path)),
        "system_lib_string" => angles(text(cx, path)),
        _ => text(cx, path),
    };
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name: "",
        alias: None,
        kind: ImportKind::SideEffect,
        in_test: cx.is_test_file,
    });
}
fn scope_of(path: &str) -> (&str, &str) {
    match path.rfind("::") {
        Some(at) => (&path[..at], &path[at + 2..]),
        None => ("", path),
    }
}
fn push_using<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(target) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "identifier" || c.kind() == "qualified_identifier")
    else {
        return;
    };
    if flat(text(cx, n)).starts_with("usingnamespace") {
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(text(cx, target)),
            name: "",
            alias: None,
            kind: ImportKind::Wildcard,
            in_test: cx.is_test_file,
        });
        return;
    }
    if (0..n.child_count()).any(|i| n.child(i).is_some_and(|c| c.kind() == "enum")) {
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(""),
            name: text(cx, target),
            alias: None,
            kind: ImportKind::Named,
            in_test: cx.is_test_file,
        });
        return;
    }
    let (module, name) = scope_of(text(cx, target));
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name,
        alias: None,
        kind: ImportKind::Named,
        in_test: cx.is_test_file,
    });
}
fn push_alias<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) -> Option<()> {
    let alias_node = child_by_field(n, "name")?;
    let target = named_children(n).into_iter().rfind(|c| {
        c.kind() == "namespace_identifier" || c.kind() == "nested_namespace_specifier"
    })?;
    let (module, name) = scope_of(text(cx, target));
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name,
        alias: Some(text(cx, alias_node)),
        kind: ImportKind::Named,
        in_test: cx.is_test_file,
    });
    Some(())
}
fn class_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let body = child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    let anon = in_anonymous(n);
    let base = child_by_field(n, "base_class_clause");
    let mut impls = Vec::new();
    if let Some(clause) = base {
        for c in named_children(clause) {
            if let Some(found) = base_ref(cx, c) {
                impls.push(found);
            }
        }
    }
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            public: !anon && !in_function(n),
            abstraction: is_abstract(cx, body),
        }),
        impls,
        container: Some(Container::Class {
            exported: !anon,
            implements: base.is_some(),
        }),
    })
}
fn base_ref<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<ImplRef<'s>> {
    if n.kind() != "type_identifier"
        && n.kind() != "qualified_identifier"
        && n.kind() != "template_type"
    {
        return None;
    }
    let full = text(cx, n);
    let plain = match full.find('<') {
        Some(at) => &full[..at],
        None => full,
    };
    Some(ImplRef {
        name: scope_of(plain).1,
        pos: pos_of(n),
    })
}
fn is_abstract<'s>(cx: &Cx<'_, 's>, body: Node<'_>) -> bool {
    named_children(body).into_iter().any(|c| {
        c.kind() == "field_declaration" && field_pure(cx, c)
            || c.kind() == "function_definition" && has_child_kind(c, "pure_virtual_clause")
    })
}
fn field_pure<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> bool {
    let mut cursor = field.walk();
    let ds: Vec<Node<'_>> = field
        .children_by_field_name("declarator", &mut cursor)
        .collect();
    ds.into_iter().any(|d| {
        let found = child_by_field(d, "declarator").unwrap_or(d);
        found.kind() == "function_declarator"
            && child_by_field(found, "default_value").is_some_and(|v| text(cx, v).trim() == "0")
    })
}
fn named_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if n.kind() == "union_specifier" || n.kind() == "enum_specifier" {
        child_by_field(n, "body")?;
    }
    let name_node = child_by_field(n, "name")?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            public: !in_anonymous(n) && !in_function(n),
            abstraction: false,
        }),
        impls: Vec::new(),
        container: None,
    })
}
fn alias_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let mut current = child_by_field(n, "declarator")?;
    loop {
        if current.kind() == "type_identifier" || current.kind() == "identifier" {
            return Some(TypeHead {
                def: Some(TypeDef {
                    name: text(cx, current),
                    pos: pos_of(current),
                    public: !in_anonymous(n) && !in_function(n),
                    abstraction: false,
                }),
                impls: Vec::new(),
                container: None,
            });
        }
        {
            let next = inner(current)?;
            current = next
        }
    }
}
fn catch_binding<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let params = child_by_field(n, "parameters")?;
    let decl_node = named_children(params)
        .into_iter()
        .find(|c| c.kind() == "parameter_declaration")?;
    let found = unwrap(child_by_field(decl_node, "declarator")?);
    (found.kind() == "identifier").then_some(text(cx, found))
}
fn catch_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, bound: Option<&'s str>) -> Stmt<'s> {
    if stmt.kind() == "expression_statement" {
        let expr =
            child_by_field(stmt, "expression").or_else(|| named_children(stmt).into_iter().next());
        if expr.is_some_and(|e| e.kind() == "call_expression") {
            let callee = expr
                .and_then(|e| child_by_field(e, "function"))
                .map(|f| text(cx, f))
                .unwrap_or("");
            return Stmt::Call {
                callee,
                kind: CallKind::Plain,
            };
        }
        return Stmt::Other;
    }
    if stmt.kind() == "throw_statement" {
        let thrown = named_children(stmt).into_iter().next();
        match thrown {
            None => return Stmt::Rethrow,
            Some(found) if found.kind() == "identifier" && Some(text(cx, found)) == bound => {
                return Stmt::Rethrow;
            }
            _ => return Stmt::Other,
        }
    }
    Stmt::Other
}
fn global_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if word(
        cx,
        n,
        &["type_qualifier", "storage_class_specifier"],
        &["const", "constexpr", "extern"],
    ) {
        return;
    }
    let (names, func) = decl_info(cx, n);
    if func {
        return;
    }
    if let Some((name, pos)) = names.into_iter().next() {
        out.push((AmbientKind::GlobalMutable, name, pos));
    }
}
fn env_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let raw = text(cx, func);
    if ENVS.contains(&flat(raw).as_str()) {
        out.push((AmbientKind::EnvRead, raw, pos_of(n)));
    }
}
fn bin_op<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if n.child_count() != 3 {
        return None;
    }
    n.child(1).map(|op| text(cx, op))
}
fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|a| {
        named_children(a)
            .into_iter()
            .find(|c| c.kind() != "comment")
    })
}
fn plain_assert(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let arg = first_arg(n).filter(|a| a.kind() == "binary_expression");
    let mut has_message = false;
    let mut comparison = false;
    if let Some(found) = arg {
        let op = bin_op(cx, found).unwrap_or("");
        if op == "&&" {
            has_message =
                child_by_field(found, "right").is_some_and(|r| r.kind() == "string_literal");
        }
        comparison = op == "==" || op == "!=";
    }
    Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
    }
}
fn gtest_assert<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> Assertion {
    let mut has_message = false;
    if let Some(parent) = n.parent()
        && parent.kind() == "binary_expression"
        && child_by_field(parent, "left").is_some_and(|l| l == n)
        && bin_op(cx, parent).is_some_and(|op| op == "<<")
    {
        has_message = true;
    }
    let suffix = &callee["ASSERT_".len()..];
    let comparison = matches!(
        suffix,
        "EQ" | "NE" | "LT" | "LE" | "GT" | "GE" | "STREQ" | "STRNE"
    );
    Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
    }
}
fn catch2_assert<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Assertion {
    let arg = first_arg(n).filter(|a| a.kind() == "binary_expression");
    let comparison = arg.is_some_and(|a| {
        bin_op(cx, a).is_some_and(|op| {
            op == "==" || op == "!=" || op == "<" || op == "<=" || op == ">" || op == ">="
        })
    });
    Assertion {
        pos: pos_of(n),
        has_message: false,
        comparison,
    }
}
fn pair_source<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
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
fn designator_of<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    for c in named_children(pair) {
        if c.kind() == "field_designator" {
            let found = named_children(c)
                .into_iter()
                .find(|d| d.kind() == "field_identifier")?;
            return Some(text(cx, found));
        }
        if c.kind() == "field_identifier" {
            return Some(text(cx, c));
        }
    }
    None
}
