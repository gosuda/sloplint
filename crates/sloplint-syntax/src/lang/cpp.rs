use sloplint_core::facts::{
    AmbientKind, AssertShape, Assertion, CallKind, FieldCopy, Handler, Import, Pos, Visibility,
    WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field,
    named_children, narrow, pos_of, preceding_comments, region_of, text,
};
use crate::lang::c_family::{
    self, CPP_DECLARATORS, bin_op, init_value, pair_source, plain_assert, root_of, sole_call, word,
};
use crate::spec::Role;
use sloplint_core::role::CodeRole;

mod calls;
mod funcs;
mod handlers;
mod imports;
mod types;

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
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

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
        let scope = funcs::scope_name(cx, fd);
        let name = text(cx, name_node);
        let slot = scope.and_then(|found| funcs::scope_member(cx, found, name));
        let public = funcs::is_public(cx, n, funcs::member_list(cx, n), slot);
        let test_macro = matches!(
            text(cx, name_node),
            "TEST" | "TEST_F" | "TEST_P" | "TEST_CASE"
        );
        let entry = name == "main"
            && name_node.kind() == "identifier"
            && scope.is_none()
            && funcs::member_list(cx, n).is_none()
            && !c_family::in_function(cx)
            && !word(cx, n, &["storage_class_specifier"], &["static"]);
        Some(FunctionHead {
            shadow_params: Vec::new(),
            name,
            name_pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            signature_forced: name_node.kind() == "operator_name"
                || funcs::is_bound(n, fd)
                || slot.is_some_and(|found| found.forced),
            entry_point: entry,
            in_test: test_macro,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: test_macro.then(|| region_of(n, CodeRole::Test)),
            receiver: None,
            params: c_family::params(
                cx,
                fd,
                &CPP_DECLARATORS,
                &["parameter_declaration", "optional_parameter_declaration"],
            ),
            return_type: funcs::return_type(cx, n, fd),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
    }
    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        if n.kind() != "lambda_expression" {
            return None;
        }
        let body = child_by_field(n, "body");
        let mut params = Vec::new();
        if let Some(declarator) = child_by_field(n, "declarator") {
            params = c_family::params(
                cx,
                declarator,
                &CPP_DECLARATORS,
                &["parameter_declaration", "optional_parameter_declaration"],
            );
        }
        Some(FunctionHead {
            shadow_params: Vec::new(),
            name: "",
            name_pos: pos_of(n),
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: None,
            params,
            receiver: None,
            return_type: None,
            missing_annotations: Vec::new(),
            doc: Vec::new(),
            doc_pos: None,
            sole_call: body.and_then(c_family::sole_call),
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
    fn block_scoped_decls(&self) -> bool {
        true
    }
    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, _cx: &Cx<'_, '_>) -> usize {
        match node.kind() {
            "for_range_loop" | "catch_clause" => depth,
            "declaration" => statement_scope(depth, node),
            _ => depth.saturating_sub(1),
        }
    }
    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(kind, "for_range_loop" | "catch_clause")
    }
    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        match n.kind() {
            "for_range_loop" => range_names(cx, n, out),
            "catch_clause" => {
                if let Some(name) = handlers::catch_binding(cx, n) {
                    out.push(name);
                }
            }
            _ => {}
        }
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
                    callee: calls::callee_text(cx, func),
                    kind: CallKind::Plain,
                    callee_root: calls::callee_root(cx, func),
                    is_simple_path: calls::is_simple_path(func),
                    initializes_binding: init_value(cx, n),
                    args: calls::call_args(cx, args_node),
                })
            }
            "new_expression" => {
                let ty = child_by_field(n, "type")?;
                let args = child_by_field(n, "arguments")
                    .map(|a| calls::call_args(cx, a))
                    .unwrap_or_default();
                Some(CallHead {
                    callee: text(cx, ty),
                    kind: CallKind::New,
                    callee_root: None,
                    is_simple_path: calls::is_simple_path(ty),
                    initializes_binding: init_value(cx, n),
                    args,
                })
            }
            _ => None,
        }
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        match n.kind() {
            "preproc_include" => imports::push_include(cx, n, out),
            "using_declaration" => imports::push_using(cx, n, out),
            "namespace_alias_definition" => {
                let _ = imports::push_alias(cx, n, out);
            }
            _ => {}
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "class_specifier" | "struct_specifier" => types::class_typedef(cx, n),
            "union_specifier" | "enum_specifier" => types::named_typedef(cx, n),
            "type_definition" => types::alias_typedef(cx, n),
            "alias_declaration" => types::named_typedef(cx, n),
            _ => None,
        }
    }
    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        if n.kind() != "function_definition" {
            return None;
        }
        let fd = c_family::unwrap(child_by_field(n, "declarator")?, CPP_DECLARATORS.wrap);
        if fd.kind() != "function_declarator" {
            return None;
        }
        let scope = funcs::scope_name(cx, fd)?;
        if !types::declared_names(cx).contains(&scope) {
            return None;
        }
        Some(Container::Class {
            exported: true,
            implements: false,
        })
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if child_by_field(n, "value").is_some() {
            return None;
        }
        let (scrutinee, variant_count, enum_like) = handlers::wildcard_info(cx);
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like,
            scrutinee,
            variant_count,
            has_nullable_ctor: false,
        })
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let bound = handlers::catch_binding(cx, n);
        let body = child_by_field(n, "body")?;
        let mut stmts = Vec::new();
        for stmt in named_children(body) {
            if stmt.kind() == "comment" {
                continue;
            }
            stmts.push(handlers::catch_stmt(cx, stmt, bound));
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
            "declaration" if !in_function => handlers::global_ambient(cx, n, out),
            "call_expression" => handlers::env_ambient(cx, n, out),
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
                calls::runtime_symbol_lookup(text(cx, func))
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
        let callee = calls::callee_text(cx, func);
        if callee == "assert" {
            return Some(plain_assert(cx, n));
        }
        if callee.starts_with("ASSERT_") || callee.starts_with("EXPECT_") {
            return Some(calls::gtest_assert(cx, n, callee));
        }
        matches!(
            callee,
            "REQUIRE" | "CHECK" | "REQUIRE_FALSE" | "CHECK_FALSE"
        )
        .then(|| calls::catch2_assert(cx, n))
    }
}

fn statement_scope(depth: usize, node: Node<'_>) -> usize {
    if node
        .parent()
        .is_some_and(|parent| parent.kind() == "case_statement")
    {
        return depth.saturating_sub(2);
    }
    let mut levels = 1;
    let mut current = node.parent();
    while let Some(outer) = current {
        if matches!(
            outer.kind(),
            "if_statement" | "while_statement" | "switch_statement" | "for_range_loop"
        ) {
            return depth.saturating_sub(levels);
        }
        if !matches!(outer.kind(), "condition_clause" | "init_statement") {
            break;
        }
        levels += 1;
        current = outer.parent();
    }
    depth.saturating_sub(1)
}

fn range_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(declarator) = child_by_field(n, "declarator") else {
        return;
    };
    let found = c_family::leaf(declarator, CPP_DECLARATORS.wrap);
    if found.kind() == "identifier" {
        out.push(text(cx, found));
    } else if found.kind() == "structured_binding_declarator" {
        for id in named_children(found) {
            if id.kind() == "identifier" {
                out.push(text(cx, id));
            }
        }
    }
}
