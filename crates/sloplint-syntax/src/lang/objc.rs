use sloplint_core::facts::{
    AmbientKind, AssertShape, Assertion, CallKind, FieldCopy, Handler, Import, Pos, ScrutineeKind,
    WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field,
    count_named_child, first_named_child, first_named_child_matching, named_children, narrow,
    pos_of, text,
};
use crate::lang::c_family::{
    self, C_DECLARATORS, flat, init_value, pair_source, plain_assert, root_of, word,
};
use crate::spec::Role;

mod calls;
mod funcs;
mod items;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("method_definition", Role::Function),
    ("block_literal", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("switch_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("message_expression", Role::Call),
    ("declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("preproc_include", Role::Import),
    ("module_import", Role::Import),
    ("struct_specifier", Role::TypeDef),
    ("union_specifier", Role::TypeDef),
    ("enum_specifier", Role::TypeDef),
    ("type_definition", Role::TypeDef),
    ("class_interface", Role::TypeDef),
    ("protocol_declaration", Role::TypeDef),
    ("class_implementation", Role::Impl),
    ("comment", Role::Comment),
    ("case_statement", Role::MatchArm),
    ("initializer_list", Role::StructLit),
    ("field_expression", Role::Member),
    ("subscript_expression", Role::Subscript),
    ("preproc_def", Role::Global),
    ("preproc_function_def", Role::Global),
];
pub(crate) struct ObjcHooks;
pub(crate) static HOOKS: ObjcHooks = ObjcHooks;
impl LangHooks for ObjcHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn block_scoped_decls(&self) -> bool {
        true
    }
    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        if node.kind() == "declaration"
            && cx
                .parent_of(node)
                .is_some_and(|parent| parent.kind() == "case_statement")
        {
            depth.saturating_sub(2)
        } else if node.kind() == "for_statement" {
            depth
        } else {
            depth.saturating_sub(1)
        }
    }
    fn shadow_decl_kind(&self, kind: &str) -> bool {
        kind == "for_statement"
    }
    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        match n.kind() {
            "for_statement" => push_for_in_var(cx, n, out),
            "block_literal" => push_block_params(cx, n, out),
            _ => {}
        }
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        match n.kind() {
            "function_definition" => funcs::c_function(cx, n),
            "method_definition" => funcs::method(cx, n),
            _ => None,
        }
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if word(cx, n, &["type_qualifier"], &["const"]) {
            return;
        }
        if word(cx, n, &["storage_class_specifier"], &["extern"]) {
            return;
        }
        out.extend(c_family::decl_info(cx, n, C_DECLARATORS.wrap).0);
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let target = match n.kind() {
            "assignment_expression" => child_by_field(n, "left"),
            "update_expression" => child_by_field(n, "argument"),
            _ => None,
        };
        if let Some(root) = target.and_then(|t| root_of(cx, t)) {
            out.push(root);
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        match n.kind() {
            "call_expression" => {
                let func = child_by_field(n, "function")?;
                let args_node = child_by_field(n, "arguments")?;
                let simple = c_family::is_simple_callee(func);
                Some(CallHead {
                    callee: text(cx, func),
                    kind: CallKind::Plain,
                    callee_root: simple
                        .then(|| root_of(cx, func))
                        .flatten()
                        .map(|(name, _)| name),
                    is_simple_path: simple,
                    initializes_binding: init_value(cx, n),
                    args: calls::call_args(cx, args_node),
                })
            }
            "message_expression" => {
                let callee = calls::first_selector(cx, n)?;
                let receiver = child_by_field(n, "receiver");
                let mut cursor = n.walk();
                let mut selectors = n.children_by_field_name("method", &mut cursor).peekable();
                let mut args = Vec::new();
                for c in named_children(n) {
                    if Some(c) == receiver
                        || c.kind() == "comment"
                        || selectors.next_if_eq(&c).is_some()
                    {
                        continue;
                    }
                    args.push(calls::mk_arg(cx, c));
                }
                let callee_root = receiver.and_then(|r| calls::msg_root(cx, r));
                Some(CallHead {
                    callee,
                    kind: CallKind::Plain,
                    callee_root,
                    is_simple_path: receiver.is_none() || callee_root.is_some(),
                    initializes_binding: init_value(cx, n),
                    args,
                })
            }
            _ => None,
        }
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        match n.kind() {
            "preproc_include" => items::push_include(cx, n, out),
            "module_import" => items::push_module(cx, n, out),
            _ => {}
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "struct_specifier" | "union_specifier" | "enum_specifier" => items::tag_typedef(cx, n),
            "type_definition" => items::alias_typedef(cx, n),
            "class_interface" => items::class_typedef(cx, n),
            "protocol_declaration" => items::protocol_typedef(cx, n),
            _ => None,
        }
    }
    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        (n.kind() == "class_implementation").then_some(Container::Impl {
            interface_bound: true,
        })
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if child_by_field(n, "value").is_some() {
            return None;
        }
        let switch = cx.ancestors().find(|a| a.kind() == "switch_statement")?;
        let body = child_by_field(switch, "body")?;
        let variants = narrow(
            named_children(body)
                .into_iter()
                .filter(|sib| is_variant_label(*sib))
                .count(),
        );
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like: variants >= 2,
            scrutinee: switch_scrutinee(switch),
            variant_count: variants,
            has_nullable_ctor: false,
        })
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let bound = items::catch_binding(cx, n);
        let body = first_named_child_matching(n, &["compound_statement"])?;
        let mut stmts = Vec::new();
        for stmt in named_children(body) {
            if stmt.kind() == "comment" {
                continue;
            }
            stmts.push(items::catch_stmt(cx, stmt, bound));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
            has_else: false,
            try_imports_only: false,
            in_cleanup: false,
        })
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "initializer_list" {
            return None;
        }
        let kids: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect();
        if kids.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        let mut consistent = true;
        for c in &kids {
            match pair_source(cx, *c) {
                Some(found) if source.is_none() => source = Some(found),
                Some(found) if source.is_some_and(|prev| prev == found) => {}
                _ => {
                    consistent = false;
                    break;
                }
            }
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(kids.len()),
            source: consistent.then_some(source).flatten(),
        })
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        match n.kind() {
            "declaration" if !in_function => items::global_ambient(cx, n, out),
            "call_expression" => c_family::env_ambient(cx, n, out, C_DECLARATORS.envs),
            "field_expression" => {
                if let Some((name, pos)) = items::dot_env(cx, n) {
                    out.push((AmbientKind::EnvRead, name, pos));
                }
            }
            "message_expression" => {
                if let Some((name, pos)) = items::msg_env(cx, n) {
                    out.push((AmbientKind::EnvRead, name, pos));
                }
            }
            _ => {}
        }
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        match n.kind() {
            "preproc_def" | "preproc_function_def" => {
                let value = child_by_field(n, "value")?;
                text(cx, value)
                    .contains("##")
                    .then_some("token-pasting macro")
            }
            "call_expression" => {
                let func = child_by_field(n, "function")?;
                let (message, name_index) = match flat(text(cx, func)).as_str() {
                    "dlsym" | "dlvsym" => ("runtime symbol lookup (dlsym)", 1),
                    "NSClassFromString" => ("runtime class lookup (NSClassFromString)", 0),
                    "NSSelectorFromString" => ("runtime selector lookup (NSSelectorFromString)", 0),
                    "objc_getClass" | "objc_getProtocol" => {
                        ("runtime class lookup (objc_getClass)", 0)
                    }
                    _ => return None,
                };
                match c_family::dynamic_name_argument(n, name_index) {
                    Some(found) if c_family::is_plain_dynamic_name(found) => None,
                    _ => Some(message),
                }
            }
            "message_expression" => calls::first_selector(cx, n)
                .is_some_and(|s| s == "performSelector")
                .then_some("runtime selector dispatch (performSelector)"),
            _ => None,
        }
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let callee = text(cx, func);
        if callee == "assert" {
            return Some(plain_assert(cx, n));
        }
        let argc = child_by_field(n, "arguments")
            .map(|a| count_named_child(a, |child| child.kind() != "comment"))
            .unwrap_or(0);
        let (has_message, comparison) = match callee {
            "NSAssert" | "NSCAssert" => (argc >= 2, false),
            "NSParameterAssert" | "NSCParameterAssert" => (false, false),
            "XCTAssert" | "XCTAssertTrue" | "XCTAssertFalse" | "XCTAssertNil"
            | "XCTAssertNotNil" => (argc >= 2, false),
            "XCTAssertEqual"
            | "XCTAssertNotEqual"
            | "XCTAssertEqualObjects"
            | "XCTAssertNotEqualObjects" => (argc >= 3, true),
            "XCTFail" => (argc >= 1, false),
            _ => return None,
        };
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison,
            shape: if comparison {
                AssertShape::TopEqNe
            } else {
                AssertShape::Other
            },
            in_typecheck: false,
            value_is_bool_literal: false,
        })
    }
}

fn is_variant_label(sib: Node<'_>) -> bool {
    sib.kind() == "case_statement"
        && child_by_field(sib, "value").is_some_and(|v| v.kind() == "identifier")
}

fn switch_scrutinee(switch: Node<'_>) -> ScrutineeKind {
    let inner = child_by_field(switch, "condition").and_then(|cond| {
        if cond.kind() == "parenthesized_expression" {
            first_named_child(cond)
        } else {
            Some(cond)
        }
    });
    match inner.map(|n| n.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}

fn push_block_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(list) = first_named_child_matching(n, &["parameter_list"]) else {
        return;
    };
    for child in named_children(list) {
        if child.kind() == "identifier" {
            out.push(text(cx, child));
        } else if child.kind() == "parameter_declaration"
            && let Some(found) = c_family::param(cx, child, &C_DECLARATORS)
            && !found.name.is_empty()
        {
            out.push(found.name);
        }
    }
}

fn push_for_in_var<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if child_by_field(n, "initializer").is_some() {
        return;
    }
    for child in named_children(n) {
        if is_loop_header_field(n, child) {
            continue;
        }
        if child.kind() == "identifier" {
            out.push(text(cx, child));
            return;
        }
        if C_DECLARATORS.wrap.contains(&child.kind()) {
            let found = c_family::leaf(child, C_DECLARATORS.wrap);
            if found.kind() == "identifier" {
                out.push(text(cx, found));
            }
            return;
        }
    }
}

fn is_loop_header_field(n: Node<'_>, child: Node<'_>) -> bool {
    [
        child_by_field(n, "body"),
        child_by_field(n, "condition"),
        child_by_field(n, "update"),
    ]
    .into_iter()
    .any(|found| found.is_some_and(|slot| slot.id() == child.id()))
}
