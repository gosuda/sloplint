use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, text};
use crate::spec::Role;

mod calls;
mod funcs;
mod items;

pub(crate) static TABLE: crate::lang::Table = &[
    ("procedure", Role::Closure),
    ("if_statement", Role::If),
    ("when_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("else_if_clause", Role::ElseIf),
    ("else_when_clause", Role::ElseIf),
    ("switch_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("ternary_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("var_declaration", Role::Decl),
    ("variable_declaration", Role::Decl),
    ("assignment_statement", Role::Bind),
    ("update_statement", Role::Write),
    ("import_declaration", Role::Import),
    ("struct_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("union_declaration", Role::TypeDef),
    ("attribute", Role::Module),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("switch_case", Role::MatchArm),
    ("struct", Role::StructLit),
];

pub(crate) static HOOKS: OdinHooks = OdinHooks;

pub(crate) struct OdinHooks;

pub(crate) fn is_anon_text(cx: &Cx<'_, '_>, c: Node<'_>, want: &str) -> bool {
    !c.is_named() && text(cx, c) == want
}

pub(crate) fn has_operator(n: Node<'_>, cx: &Cx<'_, '_>, op: &str) -> bool {
    (0..n.child_count()).any(|i| n.child(i).is_some_and(|c| is_anon_text(cx, c, op)))
}

pub(crate) fn is_test_ident(cx: &Cx<'_, '_>, c: Node<'_>) -> bool {
    c.kind() == "identifier" && text(cx, c) == "test"
}

impl LangHooks for OdinHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn ambient_bind(&self) -> bool {
        true
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let _ = (n, cx);
        None
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        funcs::closure_head(n, _parent, cx)
    }

    fn is_else_if(&self, _n: Node<'_>, _parent: Option<Node<'_>>) -> bool {
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::decl_names_head(n, cx, out);
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        if node.kind() == "for_statement" {
            return depth;
        }
        let _ = (node, cx);
        depth.saturating_sub(1)
    }

    fn block_scoped_binds(&self) -> bool {
        true
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(kind, "for_statement")
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        items::shadow_names_head(n, cx, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_roots_head(n, cx, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(n, cx)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        items::imports_head(n, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(n, cx)
    }

    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        items::container_head(n, cx)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm_head(n, cx)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        items::struct_lit_head(n, cx)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        items::ambient_head(n, cx, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::dynamic_head(n, cx)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::assertion_head(n, cx)
    }
}
