use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead};
use crate::spec::Role;
use sloplint_core::role::CodeRole;

mod calls;
mod funcs;
mod imports;
mod items;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_expression", Role::If),
    ("match_expression", Role::Match),
    ("while_expression", Role::Loop),
    ("do_while_expression", Role::Loop),
    ("for_expression", Role::Loop),
    ("catch_clause", Role::Catch),
    ("call_expression", Role::Call),
    ("split_call_condition", Role::Call),
    ("instance_expression", Role::NewCall),
    ("var_definition", Role::Decl),
    ("val_definition", Role::Decl),
    ("assignment_expression", Role::Write),
    ("infix_expression", Role::Write),
    ("import_declaration", Role::Import),
    ("export_declaration", Role::Export),
    ("class_definition", Role::TypeDef),
    ("object_definition", Role::TypeDef),
    ("trait_definition", Role::TypeDef),
    ("enum_definition", Role::TypeDef),
    ("template_body", Role::Module),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("case_clause", Role::MatchArm),
    ("field_expression", Role::Member),
    ("return_expression", Role::Return),
];

pub(crate) static HOOKS: ScalaHooks = ScalaHooks;

pub(crate) struct ScalaHooks;

impl LangHooks for ScalaHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        if n.kind() != "if_expression" {
            return false;
        }
        let Some(p) = parent else {
            return false;
        };
        if p.kind() != "if_expression" {
            return false;
        }
        p.child_by_field_name("alternative")
            .is_some_and(|a| a.id() == n.id())
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::decl_names(cx, n, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_roots(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(cx, n)
    }

    fn module_region(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        funcs::module_region(n, cx)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        imports::push_imports(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm(cx, n)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::handler_head(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        items::struct_lit(cx, n)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        items::ambient_head(cx, n, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::dynamic_head(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::assertion_head(cx, n)
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        let _ = cx;
        if items::shadow_decl_kind(node.kind()) {
            depth
        } else {
            depth.saturating_sub(1)
        }
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        items::shadow_decl_kind(kind)
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        items::shadow_names(cx, n, out);
    }
}
