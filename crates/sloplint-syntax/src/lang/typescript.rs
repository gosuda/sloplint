//! TypeScript facts for sloplint.

use sloplint_core::facts::{
    AmbientKind, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import, Pos,
    ScrutineeKind, TypeDef, WildcardArm,
};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, code_children,
    find_named_child, first_named_child_matching, named_children, pos_of, strip_quotes, text,
};
use crate::lang::javascript::{calls as js_calls, items as js_items, js_table, test_call_marker};
use crate::spec::Role;
pub(crate) mod ambient;
pub(crate) mod calls;
pub(crate) mod funcs;
pub(crate) mod imports;
use ambient::{ambient_sites, decl_names_for, write_roots_for};
use calls::{callee_root, ts_args};
use funcs::{
    TsHead, closure_params, doc_anchor, exported as ts_exported, function_head, has_override,
    head_for, member_public, visibility_of,
};
use imports::{export_stmt, heritage_impls, import_stmt, parent_dir, ts_targets};
pub(crate) static TABLE: crate::lang::Table = js_table!(
    ("abstract_class_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("type_alias_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("function_signature", Role::Function),
    ("return_statement", Role::Return),
);

pub(crate) struct TypeScriptHooks;

pub(crate) static HOOKS: TypeScriptHooks = TypeScriptHooks;

fn file_enums<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for stmt in code_children(root) {
        let decl = if stmt.kind() == "export_statement" {
            find_enum(stmt)
        } else if stmt.kind() == "enum_declaration" {
            Some(stmt)
        } else {
            None
        };
        out.extend(
            decl.and_then(|d| child_by_field(d, "name"))
                .map(|name| text(cx, name)),
        );
    }
    out
}

fn find_enum(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| child.kind() == "enum_declaration")
}

fn switch_enum_info<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    enums: &[&'s str],
) -> (bool, u32, ScrutineeKind) {
    let scrutinee = js_items::switch_scrutinee(cx, n);
    let Some(body) = cx.parent() else {
        return (false, 0, scrutinee);
    };
    let mut count = 0u32;
    let mut cursor = body.walk();
    for sibling in body.children(&mut cursor) {
        if !sibling.is_named() || sibling.kind() != "switch_case" || sibling.id() == n.id() {
            continue;
        }
        if case_enum_object(cx, sibling, enums) {
            count += 1;
        }
    }
    (count > 0, count, scrutinee)
}

fn case_enum_object<'s>(cx: &Cx<'_, 's>, case: Node<'_>, enums: &[&'s str]) -> bool {
    let value = child_by_field(case, "value").filter(|v| v.kind() == "member_expression");
    let Some(value) = value else {
        return false;
    };
    let object = child_by_field(value, "object").filter(|o| o.kind() == "identifier");
    object.is_some_and(|name| enums.contains(&text(cx, name)))
}

impl LangHooks for TypeScriptHooks {
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
        closure_from_parent(n, parent, cx)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        decl_names_for(n, cx, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        write_roots_for(n, cx, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "new_expression" {
            return ts_new_call(cx, n);
        }
        let func = child_by_field(n, "function")?;
        let args = child_by_field(n, "arguments").map_or(Vec::new(), |a| ts_args(cx, a));
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, func),
            is_simple_path: js_calls::simple_path(func),
            initializes_binding: js_calls::initializes_binding(cx, n),
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import_statement" {
            import_stmt(cx, n, out);
            return;
        }
        if n.kind() == "export_statement" {
            export_stmt(cx, n, out);
        }
    }

    fn exported_names<'s>(&self, root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        crate::lang::javascript::imports::exported_names_of(root, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        ts_typedef(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "switch_default" {
            return None;
        }
        let enums = file_enums(cx, n);
        let (enum_like, variant_count, scrutinee) = switch_enum_info(cx, n, &enums);
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like,
            scrutinee,
            variant_count,
            has_nullable_ctor: false,
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        js_items::handler_site(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        js_items::struct_lit_of(cx, n)
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
        js_items::dynamic_site(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        js_items::assertion_site(cx, n)
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        ts_barrel(root, cx)
    }
}

fn ts_new_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let ctor = child_by_field(n, "constructor")?;
    let args = child_by_field(n, "arguments").map_or(Vec::new(), |a| ts_args(cx, a));
    Some(CallHead {
        callee: text(cx, ctor),
        kind: CallKind::New,
        callee_root: None,
        is_simple_path: js_calls::simple_path(ctor),
        initializes_binding: js_calls::initializes_binding(cx, n),
        args,
    })
}

fn closure_from_parent<'s>(
    n: Node<'_>,
    parent: Option<Node<'_>>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    let parent = parent?;
    if parent.kind() == "variable_declarator" {
        return declarator_closure(n, parent, cx);
    }
    if parent.kind() == "public_field_definition" {
        return field_closure(n, parent, cx);
    }
    None
}

fn declarator_closure<'s>(
    n: Node<'_>,
    parent: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    if child_by_field(parent, "value").is_some_and(|v| v.id() != n.id()) {
        return None;
    }
    let decl = cx.parent_of(parent)?;
    let grand = cx.parent_of(decl)?;
    if grand.kind() != "program" && grand.kind() != "export_statement" {
        return None;
    }
    let name_node = child_by_field(parent, "name")?;
    if name_node.kind() != "identifier" {
        return None;
    }
    let public = ts_exported(cx, decl, text(cx, name_node));
    let mut params = Vec::new();
    closure_params(cx, n, &mut params);
    let site = TsHead {
        name: name_node,
        node: n,
        header: closure_header(cx, decl),
        visibility: visibility_of(public),
        forced: false,
        overload: false,
        body: child_by_field(n, "body"),
    };
    Some(head_for(cx, site, params))
}

fn closure_header<'t>(cx: &Cx<'t, '_>, decl: Node<'t>) -> Node<'t> {
    cx.parent_of(decl)
        .filter(|p| p.kind() == "export_statement")
        .unwrap_or(decl)
}

fn field_closure<'s>(n: Node<'_>, parent: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
    if child_by_field(parent, "value").is_some_and(|v| v.id() != n.id()) {
        return None;
    }
    let name_node = child_by_field(parent, "name")?;
    let mut params = Vec::new();
    closure_params(cx, n, &mut params);
    let site = TsHead {
        name: name_node,
        node: n,
        header: doc_anchor(cx, parent),
        visibility: visibility_of(member_public(cx, parent, name_node.kind())),
        forced: has_override(parent),
        overload: false,
        body: child_by_field(n, "body"),
    };
    Some(head_for(cx, site, params))
}

fn ts_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let kind = n.kind();
    let public = cx
        .parent_of(n)
        .is_some_and(|p| p.kind() == "export_statement");
    let abstraction = kind == "interface_declaration" || kind == "abstract_class_declaration";
    let name_node = child_by_field(n, "name")?;
    let mut impls: Vec<ImplRef<'s>> = Vec::new();
    let mut container = None;
    if kind == "class_declaration" || kind == "abstract_class_declaration" {
        let mut has_impl = false;
        if let Some(heritage) = first_named_child_matching(n, &["class_heritage"]) {
            has_impl = heritage_impls(cx, heritage, abstraction, &mut impls);
        }
        container = Some(Container::Class {
            exported: ts_exported(cx, n, text(cx, name_node)),
            implements: has_impl,
        });
    }
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: visibility_of(public),
            abstraction,
        }),
        impls,
        container,
    })
}

fn ts_barrel(root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
    if root.kind() != "program" {
        return None;
    }
    let mut sources = Vec::new();
    for child in named_children(root) {
        if child.kind() == "comment" {
            continue;
        }
        if child.kind() == "export_statement" && child_by_field(child, "source").is_some() {
            sources.push(child_by_field(child, "source").map_or("", |s| strip_quotes(text(cx, s))));
            continue;
        }
        return None;
    }
    if sources.is_empty() {
        return None;
    }
    let dir = parent_dir(cx.path);
    let mut targets = Vec::new();
    for src in sources {
        targets.push(ts_targets(dir, src));
    }
    Some(Barrel { targets })
}
