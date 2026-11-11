use sloplint_core::facts::{AmbientKind, Assertion, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field,
    find_named_child, first_of_kind, has_child_kind, last_segment, named_children, text,
};
use crate::spec::Role;

mod ambient;
mod calls;
mod funcs;
mod items;

pub(crate) static TABLE: &[(&str, Role)] = &[
    ("method_declaration", Role::Function),
    ("constructor_declaration", Role::Function),
    ("compact_constructor_declaration", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_statement", Role::If),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("enhanced_for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("ternary_expression", Role::Ternary),
    ("method_invocation", Role::Call),
    ("object_creation_expression", Role::NewCall),
    ("local_variable_declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("import_declaration", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("record_declaration", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("switch_label", Role::MatchArm),
    ("assert_statement", Role::Assert),
    ("field_declaration", Role::StaticItem),
];

pub(crate) static HOOKS: JavaHooks = JavaHooks;

pub(crate) struct JavaHooks;

pub(crate) const COMMENTS: &[&str] = &["line_comment", "block_comment"];
pub(crate) const TEST_ANNOTATIONS: &[&str] = &[
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];
pub(crate) const REFLECTION: &[&str] = &[
    "getMethod",
    "getDeclaredMethod",
    "getField",
    "getDeclaredField",
    "getConstructor",
    "getDeclaredConstructor",
];

pub(crate) fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| child.kind() != "comment")
        .collect()
}

pub(crate) fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| child.kind() != "comment")
}

pub(crate) fn modifiers_of(n: Node<'_>) -> Option<Node<'_>> {
    first_of_kind(n, &["modifiers"])
}

pub(crate) fn has_keyword(n: Node<'_>, keyword: &str) -> bool {
    modifiers_of(n).is_some_and(|modifiers| has_child_kind(modifiers, keyword))
}

pub(crate) fn annotation_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<&'s str> {
    let Some(modifiers) = modifiers_of(n) else {
        return Vec::new();
    };
    named_children(modifiers)
        .into_iter()
        .filter(|child| matches!(child.kind(), "marker_annotation" | "annotation"))
        .filter_map(|child| child_by_field(child, "name"))
        .map(|name| last_segment(text(cx, name), &["."]))
        .collect()
}

pub(crate) fn container_exports(cx: &Cx<'_, '_>) -> bool {
    !matches!(
        cx.innermost_container(),
        Some(Container::Class {
            exported: false,
            ..
        }) | Some(Container::Trait { public: false, .. })
    )
}

impl LangHooks for JavaHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        items::is_else_if(n, parent)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::local_declared(cx, n, out);
    }
    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        if node.kind() == "local_variable_declaration"
            && cx
                .parent_of(node)
                .is_some_and(|parent| parent.kind() == "switch_block_statement_group")
        {
            return depth.saturating_sub(2);
        }
        match node.kind() {
            "enhanced_for_statement"
            | "if_statement"
            | "while_statement"
            | "for_statement"
            | "switch_block_statement_group"
            | "switch_rule" => depth,
            "resource" => depth.saturating_sub(2),
            _ => depth.saturating_sub(1),
        }
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        funcs::lambda_head(cx, n)
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(
            kind,
            "enhanced_for_statement"
                | "catch_formal_parameter"
                | "local_variable_declaration"
                | "if_statement"
                | "while_statement"
                | "for_statement"
                | "switch_block_statement_group"
                | "switch_rule"
                | "resource"
        )
    }
    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        items::shadow_statement_names(cx, n, out);
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_targets(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import_declaration" {
            out.extend(items::import_fact(cx, n));
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::switch_label_arm(cx, n)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::handler_site(cx, n)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        _in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        ambient::ambient_sites(cx, n, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        calls::dynamic_site(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        calls::assertion_site(cx, n)
    }
}
