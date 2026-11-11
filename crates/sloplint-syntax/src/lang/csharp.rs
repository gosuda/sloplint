use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, any_named_child, child_by_field,
    named_children, text, trimmed,
};
use crate::spec::Role;

mod arms;
mod calls;
mod decls;
mod funcs;
mod items;
mod shadow;

pub(crate) static TABLE: crate::lang::Table = &[
    ("method_declaration", Role::Function),
    ("constructor_declaration", Role::Function),
    ("local_function_statement", Role::Function),
    ("accessor_declaration", Role::Function),
    ("operator_declaration", Role::Function),
    ("conversion_operator_declaration", Role::Function),
    ("destructor_declaration", Role::Function),
    ("lambda_expression", Role::Closure),
    ("anonymous_method_expression", Role::Closure),
    ("if_statement", Role::If),
    ("switch_statement", Role::Match),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("foreach_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("return_statement", Role::Return),
    ("yield_statement", Role::Yield),
    ("conditional_expression", Role::Ternary),
    ("invocation_expression", Role::Call),
    ("object_creation_expression", Role::NewCall),
    ("local_declaration_statement", Role::Decl),
    ("declaration_expression", Role::Decl),
    ("declaration_pattern", Role::Decl),
    ("var_pattern", Role::Decl),
    ("recursive_pattern", Role::Decl),
    ("assignment_expression", Role::Write),
    ("prefix_unary_expression", Role::Write),
    ("postfix_unary_expression", Role::Write),
    ("using_directive", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("struct_declaration", Role::TypeDef),
    ("record_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("switch_section", Role::MatchArm),
    ("switch_expression_arm", Role::MatchArm),
    ("initializer_expression", Role::StructLit),
    ("field_declaration", Role::StaticItem),
];
pub(crate) static HOOKS: CSharpHooks = CSharpHooks;
pub(crate) struct CSharpHooks;
pub(crate) const TEST_ATTRS: &[&str] = &[
    "Test",
    "TestMethod",
    "TestCase",
    "TestCaseSource",
    "DataTestMethod",
    "Fact",
    "Theory",
    "SetUp",
    "TearDown",
];
pub(crate) const EQUALITY_ASSERTS: &[&str] = &[
    "Equal",
    "NotEqual",
    "AreEqual",
    "AreNotEqual",
    "Same",
    "NotSame",
    "Contains",
];
pub(crate) const STRING_KINDS: &[&str] = &[
    "string_literal",
    "verbatim_string_literal",
    "raw_string_literal",
    "interpolated_string_expression",
];
pub(crate) fn has_mod(cx: &Cx<'_, '_>, n: Node<'_>, word: &str) -> bool {
    any_named_child(n, |c| c.kind() == "modifier" && trimmed(cx, c) == word)
}
pub(crate) fn attr_hit(cx: &Cx<'_, '_>, n: Node<'_>, wants: &[&str]) -> bool {
    any_named_child(n, |c| {
        c.kind() == "attribute_list"
            && any_named_child(c, |a| {
                a.kind() == "attribute"
                    && child_by_field(a, "name")
                        .is_some_and(|nm| wants.contains(&seg_last(trimmed(cx, nm))))
            })
    })
}
pub(crate) fn cut_generics(t: &str) -> &str {
    match t.find('<') {
        Some(i) => &t[..i],
        None => t,
    }
}
pub(crate) fn seg_last(t: &str) -> &str {
    let base = cut_generics(t);
    let bytes = base.as_bytes();
    let mut end = base.len();
    while end > 0 && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    &base[start..end]
}
fn pattern_scope(depth: usize, cx: &Cx<'_, '_>) -> usize {
    let mut current = depth.saturating_sub(1);
    for ancestor in cx.ancestors() {
        if matches!(
            ancestor.kind(),
            "block"
                | "switch_body"
                | "for_statement"
                | "foreach_statement"
                | "while_statement"
                | "using_statement"
        ) {
            return current;
        }
        current = current.saturating_sub(1);
    }
    depth.saturating_sub(1)
}
fn switch_decl_parent(depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
    if cx
        .parent_of(node)
        .is_some_and(|parent| parent.kind() == "switch_section")
    {
        depth.saturating_sub(2)
    } else {
        depth.saturating_sub(1)
    }
}
fn using_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    for init in named_children(n) {
        if init.kind() != "variable_declaration" {
            continue;
        }
        for declarator in named_children(init) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name) = child_by_field(declarator, "name") else {
                continue;
            };
            if name.kind() != "identifier" {
                continue;
            }
            out.push(text(cx, name));
        }
    }
}
impl LangHooks for CSharpHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }
    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        match node.kind() {
            "for_statement" | "foreach_statement" | "using_statement" => depth,
            "if_statement" => pattern_scope(depth, cx),
            "local_declaration_statement" => switch_decl_parent(depth, node, cx),
            "declaration_expression"
            | "declaration_pattern"
            | "var_pattern"
            | "recursive_pattern" => pattern_scope(depth, cx),
            _ => depth.saturating_sub(1),
        }
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn binds_flow_targets(&self, kind: &str) -> bool {
        kind == "for_statement"
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(
            kind,
            "foreach_statement"
                | "using_statement"
                | "if_statement"
                | "declaration_expression"
                | "declaration_pattern"
                | "var_pattern"
                | "recursive_pattern"
                | "catch_declaration"
        )
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        if n.kind() == "if_statement" {
            shadow::if_condition_shadow_names(cx, n, out);
        } else if n.kind() == "using_statement" {
            using_shadow_names(cx, n, out);
        } else {
            shadow::shadow_decl_names(cx, n, out);
        }
    }
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }
    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        funcs::closure_head(cx, n)
    }
    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        if n.kind() != "if_statement" {
            return false;
        }
        let Some(p) = parent else { return false };
        if p.kind() != "if_statement" {
            return false;
        }
        p.child_by_field_name("alternative")
            .is_some_and(|a| a.id() == n.id())
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        decls::decl_names(cx, n, out);
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_roots(cx, n, out);
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(cx, n)
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        items::push_imports(cx, n, out);
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(cx, n)
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        arms::match_arm(cx, n)
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::handler_site(cx, n)
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        items::struct_copy(cx, n)
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        _in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        items::ambient_sites(cx, n, out);
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::dynamic_site(cx, n)
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::assertion_site(cx, n)
    }
}
