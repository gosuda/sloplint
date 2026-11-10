mod arms;
mod calls;
mod funcs;
mod stmts;

use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, TypeHead};
use crate::spec::Role;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_statement", Role::Function),
    ("class_method_definition", Role::Function),
    ("script_block_expression", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("elseif_clause", Role::ElseIf),
    ("switch_statement", Role::Match),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("for_statement", Role::Loop),
    ("foreach_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("command", Role::Call),
    ("invokation_expression", Role::Call),
    ("assignment_expression", Role::Bind),
    ("pre_increment_expression", Role::Write),
    ("post_increment_expression", Role::Write),
    ("pre_decrement_expression", Role::Write),
    ("post_decrement_expression", Role::Write),
    ("class_statement", Role::TypeDef),
    ("enum_statement", Role::TypeDef),
    ("comment", Role::Comment),
    ("switch_clause", Role::MatchArm),
    ("hash_literal_expression", Role::StructLit),
    ("variable", Role::Global),
];

pub(crate) static HOOKS: PowerShellHooks = PowerShellHooks;

pub(crate) struct PowerShellHooks;

impl LangHooks for PowerShellHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::ps_function_head(cx, n)
    }
    fn is_else_if(&self, _n: Node<'_>, _parent: Option<Node<'_>>) -> bool {
        false
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        stmts::ps_decl_names(n, cx, out);
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        stmts::ps_write_roots(n, cx, out);
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::ps_call(cx, n)
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        funcs::ps_typedef(cx, n)
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        arms::ps_match_arm(n, cx)
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        arms::ps_handler(cx, n)
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        arms::ps_struct_lit(cx, n)
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        stmts::ps_ambient(n, cx, in_function, out);
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::ps_dynamic(n, cx)
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        arms::ps_assertion(n, cx)
    }
}
