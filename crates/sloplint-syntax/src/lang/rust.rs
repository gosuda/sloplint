use sloplint_core::facts::{
    AmbientKind, Assertion, Barrel, CallKind, FieldCopy, Handler, Import, Pos, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, child_by_field,
    first_named_child_matching, last_segment, text,
};
use crate::spec::Role;
use sloplint_core::role::CodeRole;
mod ambient;
mod arms;
mod asserts;
mod attrs;
mod calls;
mod errors;
mod imports;
mod items;
mod params;

use ambient::ambient;
use arms::wildcard_arm;
use asserts::assertion;
use calls::{call_args, callee_root, initializes_binding, is_simple_path, macro_args};
use imports::{push_use, use_facts};
use items::{
    barrel, call_marker, closure_head, container, function_head, handler, module_leading,
    module_region, struct_lit, typedef,
};
use params::decl_names;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_item", Role::Function),
    ("closure_expression", Role::Closure),
    ("if_expression", Role::If),
    ("else_clause", Role::ElseClause),
    ("match_expression", Role::Match),
    ("for_expression", Role::Loop),
    ("while_expression", Role::Loop),
    ("loop_expression", Role::Loop),
    ("call_expression", Role::Call),
    ("macro_invocation", Role::MacroCall),
    ("let_declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("compound_assignment_expr", Role::Write),
    ("reference_expression", Role::Write),
    ("use_declaration", Role::Import),
    ("struct_item", Role::TypeDef),
    ("enum_item", Role::TypeDef),
    ("union_item", Role::TypeDef),
    ("type_item", Role::TypeDef),
    ("trait_item", Role::TypeDef),
    ("impl_item", Role::Impl),
    ("mod_item", Role::Module),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("match_arm", Role::MatchArm),
    ("struct_expression", Role::StructLit),
    ("static_item", Role::StaticItem),
];

pub(crate) struct RustHooks;

pub(crate) static HOOKS: RustHooks = RustHooks;

impl LangHooks for RustHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        function_head(cx, n)
    }
    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        closure_head(cx, n)
    }
    fn is_else_if(&self, _n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|p| p.kind() == "else_clause")
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        decl_names(n, cx, out);
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        ambient::write_roots(n, cx, out);
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "macro_invocation" {
            let mac = child_by_field(n, "macro")?;
            let tree = first_named_child_matching(n, &["token_tree"]);
            let args = tree.map_or(Vec::new(), |tt| macro_args(cx, tt));
            return Some(CallHead {
                callee: text(cx, mac),
                kind: CallKind::Macro,
                callee_root: callee_root(cx, mac),
                is_simple_path: is_simple_path(mac),
                initializes_binding: initializes_binding(cx, n),
                args,
            });
        }
        let func = child_by_field(n, "function")?;
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, func),
            is_simple_path: is_simple_path(func),
            initializes_binding: initializes_binding(cx, n),
            args: call_args(cx, n),
        })
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if let Some(arg) = child_by_field(n, "argument") {
            let facts = use_facts(cx, n);
            let mut parts = Vec::new();
            push_use(cx, arg, &mut parts, &facts, out);
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        typedef(cx, n)
    }
    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        container(cx, n)
    }
    fn module_region(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        module_region(n, cx)
    }
    fn call_marker(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        call_marker(n, cx)
    }
    fn module_leading<'t>(&self, n: Node<'t>, _cx: &Cx<'_, '_>) -> Option<Node<'t>> {
        module_leading(n, _cx)
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        wildcard_arm(cx, n)
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        handler(cx, n)
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        struct_lit(cx, n)
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        ambient(n, cx, in_function, out);
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "macro_invocation" {
            return None;
        }
        let last = child_by_field(n, "macro").map_or("", |m| last_segment(text(cx, m), &["::"]));
        (last == "paste" || last == "concat_idents").then_some("token-pasting macro")
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        assertion(cx, n)
    }
    fn barrel(&self, root: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Barrel> {
        barrel(root)
    }
}
