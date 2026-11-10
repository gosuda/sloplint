use sloplint_core::facts::{
    AmbientKind, Assertion, Barrel, FieldCopy, Handler, Import, Pos, Visibility, WildcardArm,
};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use crate::hooks::{CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead};
use crate::spec::Role;

mod ambient;
mod calls;
mod funcs;
mod items;
mod params;

pub(crate) static TABLE: crate::lang::Table = &[
    ("let_binding", Role::Function),
    ("method_definition", Role::Function),
    ("value_specification", Role::Function),
    ("fun_expression", Role::Closure),
    ("function_expression", Role::Closure),
    ("if_expression", Role::If),
    ("else_clause", Role::ElseClause),
    ("match_expression", Role::Match),
    ("while_expression", Role::Loop),
    ("for_expression", Role::Loop),
    ("try_expression", Role::Catch),
    ("application_expression", Role::Call),
    ("new_expression", Role::NewCall),
    ("value_definition", Role::Decl),
    ("infix_expression", Role::Write),
    ("open_module", Role::Import),
    ("include_module", Role::Import),
    ("module_binding", Role::Import),
    ("type_binding", Role::TypeDef),
    ("module_type_definition", Role::TypeDef),
    ("structure", Role::Impl),
    ("comment", Role::Comment),
    ("match_case", Role::MatchArm),
    ("record_expression", Role::StructLit),
    ("assert_expression", Role::Assert),
];

pub(crate) static HOOKS: OcamlHooks = OcamlHooks { shadowed: false };

pub(crate) static SHADOWED_HOOKS: OcamlHooks = OcamlHooks { shadowed: true };

pub(crate) struct OcamlHooks {
    shadowed: bool,
}

impl OcamlHooks {
    pub(crate) fn visibility_at(&self, cx: &Cx<'_, '_>, n: Node<'_>) -> Visibility {
        if cx.lang != Lang::Ocaml || self.shadowed {
            return Visibility::Private;
        }
        let Some(parent) = cx.parent_of(n) else {
            return Visibility::Private;
        };
        if parent.kind() == "compilation_unit" || parent.kind() == "structure" {
            return Visibility::Exported;
        }
        if cx
            .parent_of(parent)
            .is_some_and(|g| g.kind() == "compilation_unit" || g.kind() == "structure")
        {
            return Visibility::Exported;
        }
        Visibility::Private
    }
}

impl LangHooks for OcamlHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(kind, "let_expression" | "match_case")
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        ambient::shadow_names_head(n, cx, out);
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        if matches!(node.kind(), "let_expression" | "match_case") {
            return depth;
        }
        let _ = (node, cx);
        depth.saturating_sub(1)
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(self, n, cx)
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        funcs::closure_head(self, n, cx)
    }

    fn is_else_if(&self, _n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|p| p.kind() == "else_clause")
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        ambient::decl_names_head(n, cx, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        calls::write_roots_head(n, cx, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(n, cx)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        items::imports_head(n, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(self, n, cx)
    }

    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        items::container_head(n, cx)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm_head(n, cx)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::handler_head(n, cx)
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
        ambient::ambient_head(n, cx, in_function, out);
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::assertion_head(n, cx)
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        items::barrel_head(root, cx)
    }
}
