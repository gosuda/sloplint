use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, AssertShape, Assertion, CallKind, FieldCopy, Handler, ImplRef,
    Import, ImportKind, ImportScope, Pos, ScrutineeKind, Stmt, TypeDef, Visibility, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    child_by_field, first_named_child, first_named_child_matching, has_child_kind, named_children,
    narrow, pos_of, preceding_comments, rfind_named_child, string_arg_shape, strip_quotes, text,
    trimmed,
};
use crate::lang::c_family::{
    self, CPP_DECLARATORS, angles, bin_op, collect_fragments, first_arg, flat, in_function,
    init_value, inner, pair_source, plain_assert, root_of, sole_call, span, word,
};
use crate::spec::Role;

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
pub(crate) struct CppHooks;
pub(crate) static HOOKS: CppHooks = CppHooks;
impl LangHooks for CppHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let (fd, name_node) = c_family::func_parts(
            n,
            &CPP_DECLARATORS,
            &[
                "identifier",
                "field_identifier",
                "destructor_name",
                "operator_name",
            ],
        )?;
        let header = cx
            .parent()
            .filter(|p| p.kind() == "template_declaration")
            .unwrap_or(n);
        let (doc, doc_pos) = preceding_comments(cx, header, &["comment"], &[]);
        let public = is_public(cx, n, member_list(cx, n));
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            signature_forced: name_node.kind() == "operator_name" || is_bound(n, fd),
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: None,
            params: c_family::params(
                cx,
                fd,
                &CPP_DECLARATORS,
                &["parameter_declaration", "optional_parameter_declaration"],
            ),
            return_type: return_type(cx, n, fd),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
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
        out.extend(c_family::decl_info(cx, n, CPP_DECLARATORS.wrap).0);
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
                    is_simple_path: func.kind() == "identifier",
                    initializes_binding: init_value(cx, n),
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
                    is_simple_path: true,
                    initializes_binding: init_value(cx, n),
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
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if child_by_field(n, "value").is_some() {
            return None;
        }
        let mut enum_like = false;
        if let Some(parent) = cx.parent() {
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
            scrutinee: ScrutineeKind::Other,
            variant_count: 0,
            has_nullable_ctor: false,
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
            has_else: false,
            try_imports_only: false,
            in_cleanup: false,
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
                runtime_symbol_lookup(text(cx, func))
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
                shape: if condition.is_some() {
                    AssertShape::TopEqNe
                } else {
                    AssertShape::Other
                },
                in_typecheck: false,
                value_is_bool_literal: false,
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
fn member_list<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Option<Node<'t>> {
    let parent = cx.parent_of(n)?;
    if parent.kind() == "field_declaration_list" {
        return Some(parent);
    }
    if parent.kind() == "template_declaration" {
        return cx
            .parent_of(parent)
            .filter(|g| g.kind() == "field_declaration_list");
    }
    None
}
fn is_public<'s>(cx: &Cx<'_, 's>, n: Node<'_>, list: Option<Node<'_>>) -> bool {
    if in_anonymous(cx) {
        return false;
    }
    let Some(items) = list else {
        return !word(cx, n, &["storage_class_specifier"], &["static"]);
    };
    let anchor = cx
        .parent()
        .filter(|p| p.kind() == "template_declaration")
        .unwrap_or(n);
    cx.members
        .state_before(items, anchor, |sib| access_marker(cx, sib))
        .unwrap_or_else(|| {
            cx.parent_of(items)
                .is_some_and(|g| g.kind() == "struct_specifier")
        })
}
fn access_marker(cx: &Cx<'_, '_>, sib: Node<'_>) -> Option<bool> {
    (sib.kind() == "access_specifier")
        .then(|| trimmed(cx, sib))
        .map(|label| label.starts_with("public") || label.starts_with("protected"))
}
fn in_anonymous(cx: &Cx<'_, '_>) -> bool {
    let mut current = cx.parent();
    while let Some(found) = current {
        if found.kind() == "namespace_definition" && child_by_field(found, "name").is_none() {
            return true;
        }
        current = cx.parent_of(found);
    }
    false
}
fn is_bound(n: Node<'_>, fd: Node<'_>) -> bool {
    has_child_kind(fd, "virtual_specifier") || has_child_kind(n, "virtual")
}
fn return_type<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    if let Some(trailing) = child_by_field(n, "trailing_return_type") {
        return child_by_field(trailing, "type_descriptor").map(|t| text(cx, t));
    }
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
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
fn import_fact<'s>(
    pos: Pos,
    module: Cow<'s, str>,
    name: &'s str,
    alias: Option<&'s str>,
    kind: ImportKind,
) -> Import<'s> {
    Import {
        pos,
        module,
        name,
        alias,
        kind,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    c_family::call_args(cx, args_node, arg_shape)
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
        "initializer_list" => ArgShape::Collection,
        "lambda_expression" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}
fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if any_named_child(n, |c| c.kind() == "identifier") {
        return ArgShape::Concat;
    }
    let mut segments = Vec::new();
    for c in named_children(n) {
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
fn push_include<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(path) = child_by_field(n, "path") else {
        return;
    };
    let module = match path.kind() {
        "string_literal" => strip_quotes(text(cx, path)),
        "system_lib_string" => angles(text(cx, path)),
        _ => text(cx, path),
    };
    out.push(import_fact(
        pos_of(n),
        Cow::Borrowed(module),
        "",
        None,
        ImportKind::SideEffect,
    ));
}
fn scope_of(path: &str) -> (&str, &str) {
    match path.rfind("::") {
        Some(at) => (&path[..at], &path[at + 2..]),
        None => ("", path),
    }
}
fn push_using<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(target) = first_named_child_matching(n, &["identifier", "qualified_identifier"])
    else {
        return;
    };
    if flat(text(cx, n)).starts_with("usingnamespace") {
        out.push(import_fact(
            pos_of(n),
            Cow::Borrowed(text(cx, target)),
            "",
            None,
            ImportKind::Wildcard,
        ));
        return;
    }
    if (0..n.child_count()).any(|i| n.child(i).is_some_and(|c| c.kind() == "enum")) {
        out.push(import_fact(
            pos_of(n),
            Cow::Borrowed(""),
            text(cx, target),
            None,
            ImportKind::Named,
        ));
        return;
    }
    let (module, name) = scope_of(text(cx, target));
    out.push(import_fact(
        pos_of(n),
        Cow::Borrowed(module),
        name,
        None,
        ImportKind::Named,
    ));
}
fn push_alias<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) -> Option<()> {
    let alias_node = child_by_field(n, "name")?;
    let target = rfind_named_child(n, |c| {
        c.kind() == "namespace_identifier" || c.kind() == "nested_namespace_specifier"
    })?;
    let (module, name) = scope_of(text(cx, target));
    out.push(import_fact(
        pos_of(n),
        Cow::Borrowed(module),
        name,
        Some(text(cx, alias_node)),
        ImportKind::Named,
    ));
    Some(())
}
fn class_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let body = child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    let anon = in_anonymous(cx);
    let base = child_by_field(n, "base_class_clause");
    let mut impls = Vec::new();
    if let Some(clause) = base {
        for c in named_children(clause) {
            if let Some(found) = base_ref(cx, c) {
                impls.push(found);
            }
        }
    }
    let public = !anon && !in_function(cx);
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
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
        is_generic: plain != full,
        child_is_abstract: false,
    })
}
fn is_abstract<'s>(cx: &Cx<'_, 's>, body: Node<'_>) -> bool {
    any_named_child(body, |c| {
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
    let public = !in_anonymous(cx) && !in_function(cx);
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
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
            let public = !in_anonymous(cx) && !in_function(cx);
            return Some(TypeHead {
                def: Some(TypeDef {
                    name: text(cx, current),
                    pos: pos_of(current),
                    visibility: if public {
                        Visibility::Exported
                    } else {
                        Visibility::Private
                    },
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
    let decl_node = first_named_child_matching(params, &["parameter_declaration"])?;
    let found = c_family::unwrap(
        child_by_field(decl_node, "declarator")?,
        CPP_DECLARATORS.wrap,
    );
    (found.kind() == "identifier").then_some(text(cx, found))
}
fn catch_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, bound: Option<&'s str>) -> Stmt<'s> {
    if stmt.kind() == "expression_statement" {
        let expr = child_by_field(stmt, "expression").or_else(|| first_named_child(stmt));
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
        let thrown = first_named_child(stmt);
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
    let (names, func) = c_family::decl_info(cx, n, CPP_DECLARATORS.wrap);
    if func {
        return;
    }
    if let Some((name, pos)) = names.into_iter().next() {
        out.push((AmbientKind::GlobalMutable, name, pos));
    }
}
fn env_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if callee_holds_a_paren(cx, n) {
        return;
    }
    c_family::env_ambient(cx, n, out, CPP_DECLARATORS.envs);
}

fn callee_holds_a_paren(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "function")
        .is_some_and(|f| text(cx, f).bytes().any(|b| b == b'(' || b == b')'))
}

fn runtime_symbol_lookup(callee: &str) -> Option<&'static str> {
    if !callee.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    match flat(callee).as_str() {
        "dlsym" | "dlvsym" => Some("runtime symbol lookup (dlsym)"),
        "GetProcAddress" => Some("runtime symbol lookup (GetProcAddress)"),
        _ => None,
    }
}
fn gtest_assert<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> Assertion {
    let mut has_message = false;
    if let Some(parent) = cx.parent()
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
        shape: if comparison {
            AssertShape::TopEqNe
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
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
        shape: if comparison {
            AssertShape::TopEqNe
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}
