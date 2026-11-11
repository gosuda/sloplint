use sloplint_core::facts::{AmbientKind, Assertion, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field,
    first_named_child, has_child_kind, pos_of, text,
};
use crate::spec::Role;
use sloplint_core::role::CodeRole;

mod calls;
mod funcs;
mod imports;
mod items;
mod payloads;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function),
    ("if_statement", Role::If),
    ("if_expression", Role::If),
    ("else_clause", Role::ElseClause),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("for_expression", Role::Loop),
    ("while_statement", Role::Loop),
    ("while_expression", Role::Loop),
    ("catch_expression", Role::Catch),
    ("call_expression", Role::Call),
    ("builtin_function", Role::MacroCall),
    ("variable_declaration", Role::Bind),
    ("assignment_expression", Role::Write),
    ("string", Role::Import),
    ("using_namespace_declaration", Role::Import),
    ("struct_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("union_declaration", Role::TypeDef),
    ("opaque_declaration", Role::TypeDef),
    ("test_declaration", Role::Module),
    ("comment", Role::Comment),
    ("switch_case", Role::MatchArm),
    ("struct_initializer", Role::StructLit),
    ("anonymous_struct_initializer", Role::StructLit),
];

pub(crate) static HOOKS: ZigHooks = ZigHooks;

pub(crate) struct ZigHooks;

pub(crate) fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        let kind = current.kind();
        if kind == "identifier" {
            return Some((text(cx, current), pos_of(current)));
        }
        if kind == "field_expression" || kind == "index_expression" {
            current = child_by_field(current, "object")?;
            continue;
        }
        if kind == "dereference_expression" {
            current = crate::hooks::first_named_child(current)?;
            continue;
        }
        if kind == "parenthesized_expression" {
            current = first_named_child(current)?;
            continue;
        }
        return None;
    }
}

impl LangHooks for ZigHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn ambient_bind(&self) -> bool {
        true
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::zig_function(n, cx)
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        if parent.is_some_and(|p| p.kind() == "else_clause") {
            return true;
        }
        let mut current = n.prev_sibling();
        while let Some(sibling) = current {
            if sibling.kind() == "payload" {
                current = sibling.prev_sibling();
            } else {
                return sibling.kind() == "else";
            }
        }
        false
    }

    fn block_scoped_binds(&self) -> bool {
        true
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, _cx: &Cx<'_, '_>) -> usize {
        if payloads::shadow_decl_kind(node.kind()) {
            depth
        } else {
            depth.saturating_sub(1)
        }
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        kind == "variable_declaration" || payloads::shadow_decl_kind(kind)
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        if n.kind() != "variable_declaration" {
            payloads::shadow_names(n, cx, out);
            return;
        }
        if has_child_kind(n, "var") {
            return;
        }
        let mut headed = false;
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if !child.is_named() {
                headed = child.kind() == "const";
                continue;
            }
            if headed && child.kind() == "identifier" {
                out.push(text(cx, child));
            }
        }
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "variable_declaration" {
            return;
        }
        let mut headed = false;
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if !child.is_named() {
                headed = child.kind() == "var";
                continue;
            }
            if headed && child.kind() == "identifier" {
                out.push((text(cx, child), pos_of(child)));
            }
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "assignment_expression" {
            if let Some(left) = child_by_field(n, "left")
                && let Some(root) = write_root(cx, left)
            {
                out.push(root);
            }
            return;
        }
        if n.kind() != "variable_declaration" {
            return;
        }
        let mut open = false;
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if !child.is_named() {
                if child.kind() == "=" {
                    return;
                }
                open = child.kind() == "var" || child.kind() == "const";
                continue;
            }
            if open {
                continue;
            }
            if let Some(root) = write_root(cx, child)
                && root.0 != "_"
            {
                out.push(root);
            }
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::zig_call(n, cx)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        imports::zig_imports(n, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::zig_typedef(n, cx)
    }

    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        (n.kind() == "test_declaration").then_some(Container::TestModule)
    }

    fn module_region(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<CodeRole> {
        (n.kind() == "test_declaration").then_some(CodeRole::Test)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::zig_match_arm(n, cx)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        items::zig_handler(n, cx)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        items::zig_struct_lit(n, cx)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        items::zig_ambient(n, cx, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        items::zig_dynamic(n, cx)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        items::zig_assertion(n, cx)
    }
}
