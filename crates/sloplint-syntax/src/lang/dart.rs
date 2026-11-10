use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead};
use crate::spec::Role;

mod calls;
mod funcs;
mod imports;
mod items;
mod syntax;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function),
    ("method_declaration", Role::Function),
    ("local_function_declaration", Role::Function),
    ("function_expression", Role::Closure),
    ("if_statement", Role::If),
    ("switch_statement", Role::Match),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("new_expression", Role::NewCall),
    ("const_object_expression", Role::NewCall),
    ("local_variable_declaration", Role::Decl),
    ("top_level_variable_declaration", Role::StaticItem),
    ("assignment_expression", Role::Write),
    ("postfix_expression", Role::Write),
    ("unary_expression", Role::Write),
    ("library_import", Role::Import),
    ("library_export", Role::Export),
    ("class_declaration", Role::TypeDef),
    ("mixin_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("extension_type_declaration", Role::TypeDef),
    ("extension_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("documentation_block_comment", Role::Comment),
    ("switch_statement_case", Role::MatchArm),
    ("switch_statement_default", Role::MatchArm),
    ("switch_expression_case", Role::MatchArm),
    ("member_expression", Role::Member),
    ("assert_statement", Role::Assert),
    ("return_statement", Role::Return),
];

pub(crate) static HOOKS: DartHooks = DartHooks;

pub(crate) struct DartHooks;

impl LangHooks for DartHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        funcs::closure_shadow_params(cx, n, out);
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        funcs::is_else_if(n, parent)
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

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        imports::imports(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm(n, cx)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::handler(cx, n)
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
        items::ambient(cx, n, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::dynamic(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::assertion(cx, n)
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<sloplint_core::facts::Barrel> {
        imports::barrel(cx, root)
    }
}
