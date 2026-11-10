use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead};
use crate::spec::Role;

mod calls;
mod funcs;
mod items;
mod syntax;

#[rustfmt::skip]
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function), ("init_declaration", Role::Function),
    ("lambda_literal", Role::Closure), ("if_statement", Role::If),
    ("guard_statement", Role::If), ("switch_statement", Role::Match),
    ("for_statement", Role::Loop), ("while_statement", Role::Loop),
    ("repeat_while_statement", Role::Loop), ("catch_block", Role::Catch),
    ("ternary_expression", Role::Ternary), ("call_expression", Role::Call),
    ("property_declaration", Role::Decl), ("assignment", Role::Write),
    ("import_declaration", Role::Import), ("class_declaration", Role::TypeDef),
    ("protocol_declaration", Role::TypeDef), ("typealias_declaration", Role::TypeDef),
    ("comment", Role::Comment), ("multiline_comment", Role::Comment),
    ("switch_entry", Role::MatchArm), ("navigation_expression", Role::Member),
];

pub(crate) static HOOKS: SwiftHooks = SwiftHooks;

pub(crate) struct SwiftHooks;

impl LangHooks for SwiftHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }

    fn untyped_throws(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        funcs::untyped_throws(n, cx)
    }

    fn is_else_if(&self, _n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        funcs::is_else_if(_n, parent)
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
        items::imports(cx, n, out);
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
}
