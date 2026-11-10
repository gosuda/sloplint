use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, TypeHead};
use crate::spec::Role;

mod calls;
mod funcs;
mod imports;
mod items;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("method_declaration", Role::Function),
    ("anonymous_function", Role::Closure),
    ("arrow_function", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("else_if_clause", Role::ElseIf),
    ("switch_statement", Role::Match),
    ("match_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("foreach_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("function_call_expression", Role::Call),
    ("member_call_expression", Role::Call),
    ("scoped_call_expression", Role::Call),
    ("nullsafe_member_call_expression", Role::Call),
    ("object_creation_expression", Role::NewCall),
    ("assignment_expression", Role::Bind),
    ("augmented_assignment_expression", Role::Bind),
    ("reference_assignment_expression", Role::Bind),
    ("update_expression", Role::Write),
    ("namespace_use_declaration", Role::Import),
    ("include_expression", Role::Import),
    ("include_once_expression", Role::Import),
    ("require_expression", Role::Import),
    ("require_once_expression", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("trait_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("case_statement", Role::MatchArm),
    ("default_statement", Role::MatchArm),
    ("match_conditional_expression", Role::MatchArm),
    ("match_default_expression", Role::MatchArm),
    ("array_creation_expression", Role::StructLit),
    ("subscript_expression", Role::Subscript),
    ("dynamic_variable_name", Role::Global),
    ("global_declaration", Role::Global),
    ("return_statement", Role::Return),
    ("function_static_declaration", Role::StaticItem),
    ("property_declaration", Role::StaticItem),
];

pub(crate) static HOOKS: PhpHooks = PhpHooks;

pub(crate) struct PhpHooks;

impl LangHooks for PhpHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }
    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        n.kind() == "if_statement" && parent.is_some_and(|p| p.kind() == "else_clause")
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
        imports::push_imports(cx, n, out);
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(cx, n)
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm(n, cx)
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
}
