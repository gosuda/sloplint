use sloplint_core::facts::{
    AmbientKind, Assertion, CallKind, FieldCopy, Handler, Import, Pos, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field, text,
};
use crate::spec::Role;

mod calls;
mod funcs;
mod imports;
mod stmts;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function),
    ("method_declaration", Role::Function),
    ("func_literal", Role::Closure),
    ("if_statement", Role::If),
    ("expression_switch_statement", Role::Match),
    ("type_switch_statement", Role::Match),
    ("select_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("call_expression", Role::Call),
    ("var_declaration", Role::Decl),
    ("short_var_declaration", Role::Bind),
    ("assignment_statement", Role::Write),
    ("inc_statement", Role::Write),
    ("return_statement", Role::Return),
    ("dec_statement", Role::Write),
    ("import_declaration", Role::Import),
    ("type_spec", Role::TypeDef),
    ("comment", Role::Comment),
    ("default_case", Role::MatchArm),
    ("composite_literal", Role::StructLit),
];

pub(crate) struct GoHooks;

pub(crate) static HOOKS: GoHooks = GoHooks;

impl LangHooks for GoHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let name_node = child_by_field(n, "name")?;
        let mut params = Vec::new();
        if let Some(list) = child_by_field(n, "parameters") {
            funcs::go_params(cx, list, &mut params);
        }
        let marker = (n.kind() == "function_declaration")
            .then(|| funcs::go_marker(text(cx, name_node), &params, cx.path))
            .flatten();
        Some(funcs::go_function_head(cx, n, name_node, params, marker))
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        if n.kind() != "func_literal" {
            return None;
        }
        Some(funcs::go_closure_head(cx, n))
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent
            .and_then(|p| p.child_by_field_name("alternative"))
            .is_some_and(|a| a.id() == n.id())
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        stmts::go_decl_names(cx, n, out);
    }

    fn decl_visible_in_initializer(&self) -> bool {
        false
    }

    fn decl_piece(&self, kind: &str) -> bool {
        kind == "var_spec"
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        if node.kind() == "type_switch_statement" {
            return depth;
        }
        if cx.parent().is_some_and(|p| p.kind() == "for_clause") {
            depth.saturating_sub(2)
        } else {
            depth.saturating_sub(1)
        }
    }

    fn block_scoped_binds(&self) -> bool {
        true
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(
            kind,
            "range_clause" | "type_switch_statement" | "receive_statement"
        )
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        stmts::go_shadow_names(cx, n, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        stmts::go_write_roots(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let args = calls::go_call_args(cx, n);
        Some(CallHead {
            callee: calls::go_callee(cx, func),
            kind: CallKind::Plain,
            callee_root: calls::go_callee_root(cx, func),
            is_simple_path: calls::go_is_simple_path(func),
            initializes_binding: calls::go_initializes_binding(cx, n),
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        imports::go_imports(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        funcs::go_typedef(cx, n)
    }

    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        (n.kind() == "method_declaration").then_some(Container::Impl {
            interface_bound: false,
        })
    }

    fn match_arm(&self, _n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        None
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        stmts::go_handler(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        stmts::go_struct_lit(cx, n)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        stmts::go_ambient(cx, n, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::go_dynamic(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::go_assertion(cx, n)
    }
}
