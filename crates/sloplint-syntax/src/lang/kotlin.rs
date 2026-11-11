use sloplint_core::facts::{
    AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, Visibility, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, any_named_child,
    first_of_kind, named_children, text,
};
use crate::spec::Role;

mod calls;
mod funcs;
mod items;

pub(crate) static TABLE: &[(&str, Role)] = &[
    ("function_declaration", Role::Function),
    ("secondary_constructor", Role::Function),
    ("primary_constructor", Role::Function),
    ("lambda_literal", Role::Closure),
    ("anonymous_function", Role::Closure),
    ("if_expression", Role::If),
    ("when_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_while_statement", Role::Loop),
    ("catch_block", Role::Catch),
    ("call_expression", Role::Call),
    ("property_declaration", Role::Decl),
    ("assignment", Role::Write),
    ("unary_expression", Role::Write),
    ("import", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("object_declaration", Role::TypeDef),
    ("object_literal", Role::Impl),
    ("type_alias", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("when_entry", Role::MatchArm),
];

pub(crate) static HOOKS: KotlinHooks = KotlinHooks;

pub(crate) struct KotlinHooks;

pub(crate) const COMMENTS: &[&str] = &["line_comment", "block_comment"];
pub(crate) const TEST_ANNOTATIONS: &[&str] = &[
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];
pub(crate) const TYPE_KINDS: &[&str] = &[
    "user_type",
    "nullable_type",
    "function_type",
    "non_nullable_type",
    "parenthesized_type",
    "dynamic",
];
pub(crate) const REFLECTION: &[&str] = &[
    "getMethod",
    "getDeclaredMethod",
    "getField",
    "getDeclaredField",
    "getConstructor",
    "getDeclaredConstructor",
];

fn children_of(n: Node<'_>) -> Vec<Node<'_>> {
    (0..n.child_count()).filter_map(|i| n.child(i)).collect()
}

pub(crate) fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| child.kind() != "line_comment" && child.kind() != "block_comment")
        .collect()
}

pub(crate) fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    crate::hooks::find_named_child(n, |child| {
        child.kind() != "line_comment" && child.kind() != "block_comment"
    })
}

pub(crate) fn modifiers_of(n: Node<'_>) -> Option<Node<'_>> {
    first_of_kind(n, &["modifiers"])
}

pub(crate) fn has_modifier_word(cx: &Cx<'_, '_>, n: Node<'_>, word: &str) -> bool {
    modifiers_of(n).is_some_and(|modifiers| {
        (0..modifiers.child_count())
            .filter_map(|i| modifiers.child(i))
            .any(|child| text(cx, child) == word)
    })
}

pub(crate) fn annotation_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<&'s str> {
    let Some(modifiers) = modifiers_of(n) else {
        return Vec::new();
    };
    named_children(modifiers)
        .into_iter()
        .filter(|child| child.kind() == "annotation")
        .map(|child| {
            let bare = text(cx, child).trim_start_matches('@');
            crate::hooks::last_segment(bare.split('(').next().unwrap_or(""), &["."])
        })
        .collect()
}

pub(crate) fn type_after_colon(n: Node<'_>) -> Option<Node<'_>> {
    let mut seen_colon = false;
    for child in children_of(n) {
        if child.kind() == ":" {
            seen_colon = true;
        } else if seen_colon && TYPE_KINDS.contains(&child.kind()) {
            return Some(child);
        }
    }
    None
}

pub(crate) fn is_comment(n: Node<'_>) -> bool {
    matches!(n.kind(), "line_comment" | "block_comment")
}

pub(crate) fn previous_code_sibling(n: Node<'_>) -> Option<Node<'_>> {
    let mut previous = n.prev_sibling();
    while let Some(sibling) = previous {
        if !matches!(sibling.kind(), ";" | "line_comment" | "block_comment") {
            return Some(sibling);
        }
        previous = sibling.prev_sibling();
    }
    None
}

fn has_visibility_word(cx: &Cx<'_, '_>, n: Node<'_>, words: &[&str]) -> bool {
    modifiers_of(n).is_some_and(|modifiers| {
        any_named_child(modifiers, |child| {
            child.kind() == "visibility_modifier" && words.contains(&text(cx, child))
        })
    })
}

pub(crate) fn type_visibility(cx: &Cx<'_, '_>, n: Node<'_>) -> Visibility {
    if has_visibility_word(cx, n, &["private"]) {
        Visibility::Private
    } else if has_visibility_word(cx, n, &["internal"]) {
        Visibility::Restricted
    } else {
        Visibility::Exported
    }
}

pub(crate) fn stricter_visibility(first: Visibility, second: Visibility) -> Visibility {
    match (first, second) {
        (Visibility::Private, _) | (_, Visibility::Private) => Visibility::Private,
        (Visibility::Restricted, _) | (_, Visibility::Restricted) => Visibility::Restricted,
        _ => Visibility::Exported,
    }
}

impl LangHooks for KotlinHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        let _ = parent;
        funcs::closure_head(cx, n)
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        let _ = cx;
        if matches!(node.kind(), "for_statement" | "when_expression") {
            return depth;
        }
        depth.saturating_sub(1)
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        matches!(kind, "for_statement" | "when_expression")
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        items::shadow_names(cx, n, out);
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        items::is_else_if(n, parent)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::decl_names(cx, n, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_targets(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import" {
            out.extend(items::import_fact(cx, n));
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(cx, n)
    }

    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        (n.kind() == "object_literal").then_some(Container::Class {
            exported: false,
            implements: false,
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm(cx, n)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::handler_site(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        calls::struct_copy(cx, n)
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
