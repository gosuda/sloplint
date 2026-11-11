//! JavaScript facts for sloplint.

use sloplint_core::facts::{
    AmbientKind, Assertion, Barrel, FieldCopy, Handler, Import, Pos, WildcardArm,
};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field, find_named_child,
    text,
};
use crate::spec::Role;

pub(crate) mod ambient;
pub(crate) mod calls;
pub(crate) mod dynamic;
pub(crate) mod funcs;
pub(crate) mod imports;
pub(crate) mod items;

use ambient::{
    ambient_sites, decl_names_for, function_body_depth, shadow_kind_for, shadow_names_for,
    write_roots_for,
};
use calls::call_head;
pub(crate) use calls::test_call_marker;
use funcs::{closure_head, function_head};
use imports::{exported_names_of, import_facts};
pub(crate) use imports::{external_root_of, is_test_path};
use items::{
    assertion_site, barrel_of, dynamic_site, handler_site, match_arm_default, struct_lit_of,
    typedef_head,
};

macro_rules! js_table {
    ($($extra:expr),* $(,)?) => {
        &[
            ("function_declaration", Role::Function),
            ("generator_function_declaration", Role::Function),
            ("method_definition", Role::Function),
            ("arrow_function", Role::Closure),
            ("function_expression", Role::Closure),
            ("generator_function", Role::Closure),
            ("identifier", Role::Global),
            ("if_statement", Role::If),
            ("else_clause", Role::ElseClause),
            ("switch_statement", Role::Match),
            ("for_statement", Role::Loop),
            ("for_in_statement", Role::Loop),
            ("while_statement", Role::Loop),
            ("do_statement", Role::Loop),
            ("catch_clause", Role::Catch),
            ("ternary_expression", Role::Ternary),
            ("call_expression", Role::Call),
            ("new_expression", Role::NewCall),
            ("lexical_declaration", Role::Decl),
            ("variable_declaration", Role::Bind),
            ("using_declaration", Role::Decl),
            ("assignment_expression", Role::Write),
            ("augmented_assignment_expression", Role::Write),
            ("update_expression", Role::Write),
            ("import_statement", Role::Import),
            ("export_statement", Role::Export),
            ("class_declaration", Role::TypeDef),
            ("comment", Role::Comment),
            ("switch_default", Role::MatchArm),
            ("return_statement", Role::Return),
            ("object", Role::StructLit),
            ("member_expression", Role::Member),
            ("subscript_expression", Role::Subscript),
            $($extra,)*
        ]
    };
}
pub(crate) use js_table;

pub(crate) static TABLE: &[(&str, Role)] = js_table!();

pub(crate) static HOOKS: JavaScriptHooks = JavaScriptHooks;

pub(crate) struct JavaScriptHooks;

pub(crate) const COMMENTS: &[&str] = &["comment"];

pub(crate) fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| child.kind() != "comment")
}

pub(crate) fn parent_kind_is(cx: &Cx<'_, '_>, n: Node<'_>, kind: &str) -> bool {
    cx.parent_of(n).is_some_and(|parent| parent.kind() == kind)
}

pub(crate) fn header_of<'t>(cx: &Cx<'t, '_>, n: Node<'t>) -> Node<'t> {
    match cx.parent_of(n) {
        Some(parent) if parent.kind() == "export_statement" => parent,
        _ => n,
    }
}

pub(crate) fn top_level(cx: &Cx<'_, '_>) -> bool {
    matches!(
        cx.parent().map(|parent| parent.kind()),
        Some("program" | "export_statement")
    )
}

impl LangHooks for JavaScriptHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn call_marker(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        test_call_marker(cx, n)
    }
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        function_head(n, cx)
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        closure_head(n, parent, cx)
    }

    fn is_ternary_alternate(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        n.kind() == "ternary_expression"
            && parent.is_some_and(|p| {
                p.kind() == "ternary_expression"
                    && p.child_by_field_name("alternative")
                        .is_some_and(|a| a.id() == n.id())
            })
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        decl_names_for(n, cx, out);
    }
    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        if node.kind() == "catch_clause" {
            return depth;
        }
        if node.kind() == "for_in_statement" {
            if child_by_field(node, "kind").is_some_and(|kind| text(cx, kind) == "var") {
                return function_body_depth(depth, cx);
            }
            return depth;
        }
        if node.kind() == "lexical_declaration"
            && cx.parent_of(node).is_some_and(|parent| {
                parent.kind() == "switch_case" || parent.kind() == "switch_default"
            })
        {
            return depth.saturating_sub(2);
        }
        if node.kind() == "variable_declaration"
            || matches!(
                node.kind(),
                "function_declaration" | "generator_function_declaration"
            )
        {
            return function_body_depth(depth, cx);
        }
        depth.saturating_sub(1)
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn binds_flow_targets(&self, kind: &str) -> bool {
        kind == "for_in_statement"
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        shadow_kind_for(kind)
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        shadow_names_for(n, cx, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        write_roots_for(n, cx, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        import_facts(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        typedef_head(cx, n)
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        match_arm_default(cx, n)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        handler_site(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        struct_lit_of(cx, n)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        ambient_sites(cx, n, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        dynamic_site(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        assertion_site(cx, n)
    }

    fn exported_names<'s>(&self, root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        exported_names_of(root, cx, out);
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        barrel_of(root, cx)
    }
}
