use sloplint_core::facts::{
    AmbientKind, AssertShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Pos, Stmt, TypeDef,
    Visibility, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, count_named_child,
    first_of_kind, last_segment, named_children, pos_of, preceding_comments, region_of,
    rfind_named_child, text,
};
use crate::spec::Role;
use sloplint_core::role::CodeRole;

mod calls;
mod funcs;
mod items;

use calls::{
    call_args, ruby_args, ruby_call_kind, ruby_callee, ruby_is_simple_path, ruby_receiver_root,
};
use funcs::{
    is_protocol_method, ruby_in_test_class, ruby_params, ruby_public, ruby_sole_call,
    superclass_name,
};
use items::{
    bare_assert_shape, case_scrutinee_kind, has_constant_pattern, hash_copy, is_assign_left,
    minitest_arity, rspec_assertion, ruby_env_name, ruby_stmt, sibling_has_constant,
    sibling_variant_count, strip_arrow,
};
fn field_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> Option<&'s str> {
    child_by_field(n, field).map(|v| text(cx, v))
}

pub(crate) static TABLE: crate::lang::Table = &[
    ("method", Role::Function),
    ("singleton_method", Role::Function),
    ("block", Role::Closure),
    ("do_block", Role::Closure),
    ("if", Role::If),
    ("unless", Role::If),
    ("if_modifier", Role::If),
    ("unless_modifier", Role::If),
    ("elsif", Role::ElseIf),
    ("case", Role::Match),
    ("case_match", Role::Match),
    ("while", Role::Loop),
    ("until", Role::Loop),
    ("for", Role::Loop),
    ("while_modifier", Role::Loop),
    ("until_modifier", Role::Loop),
    ("rescue", Role::Catch),
    ("rescue_modifier", Role::Catch),
    ("conditional", Role::Ternary),
    ("call", Role::Call),
    ("assignment", Role::Bind),
    ("operator_assignment", Role::Bind),
    ("class", Role::TypeDef),
    ("module", Role::TypeDef),
    ("comment", Role::Comment),
    ("when", Role::MatchArm),
    ("in_clause", Role::MatchArm),
    ("else", Role::MatchArm),
    ("hash", Role::StructLit),
    ("element_reference", Role::Subscript),
    ("global_variable", Role::Global),
    ("class_variable", Role::Global),
];

pub(crate) static HOOKS: RubyHooks = RubyHooks;

pub(crate) struct RubyHooks;

fn push_ruby_target<'s>(cx: &Cx<'_, 's>, node: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if matches!(
        node.kind(),
        "identifier" | "instance_variable" | "global_variable" | "class_variable"
    ) {
        out.push((text(cx, node), pos_of(node)));
    }
}
fn collect_mixins<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<ImplRef<'s>>) {
    let Some(body) = child_by_field(n, "body") else {
        return;
    };
    let stmts: Vec<Node<'_>> = if body.kind() == "body_statement" {
        named_children(body)
    } else {
        vec![body]
    };
    for stmt in stmts {
        if stmt.kind() != "call" {
            continue;
        }
        let method = field_text(cx, stmt, "method").unwrap_or("");
        if method != "include" && method != "extend" && method != "prepend" {
            continue;
        }
        if child_by_field(stmt, "receiver").is_some() {
            continue;
        }
        for arg in call_args(stmt) {
            if arg.kind() == "constant" {
                out.push(ImplRef {
                    name: last_segment(text(cx, arg), &["::"]),
                    pos: pos_of(arg),
                    is_generic: false,
                    child_is_abstract: false,
                });
            }
        }
    }
}
impl LangHooks for RubyHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let kind = n.kind();
        if kind != "method" && kind != "singleton_method" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        let tested = ruby_in_test_class(cx, n);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            visibility: if ruby_public(cx, n) {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            signature_forced: is_protocol_method(name),
            entry_point: false,
            in_test: tested,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: tested.then(|| region_of(n, CodeRole::Test)),
            params: ruby_params(cx, n),
            return_type: None,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: ruby_sole_call(n),
        })
    }
    fn is_else_if(&self, _n: Node<'_>, _parent: Option<Node<'_>>) -> bool {
        false
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let kind = n.kind();
        if kind != "assignment" && kind != "operator_assignment" {
            return;
        }
        let Some(left) = child_by_field(n, "left") else {
            return;
        };
        if left.kind() == "left_assignment_list" {
            for child in named_children(left) {
                push_ruby_target(cx, child, out);
            }
        } else {
            push_ruby_target(cx, left, out);
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "call" {
            return None;
        }
        let receiver = child_by_field(n, "receiver");
        let method = field_text(cx, n, "method").unwrap_or("");
        if method == "new"
            && let Some(recv) = receiver
        {
            return Some(CallHead {
                callee: text(cx, recv),
                kind: CallKind::New,
                callee_root: Some(ruby_receiver_root(cx, recv)),
                is_simple_path: false,
                initializes_binding: false,
                args: ruby_args(cx, n),
            });
        }
        let root = receiver
            .map(|r| ruby_receiver_root(cx, r))
            .filter(|root| !root.is_empty());
        Some(CallHead {
            callee: ruby_callee(cx, n),
            kind: CallKind::Plain,
            callee_root: root,
            is_simple_path: ruby_is_simple_path(cx, n),
            initializes_binding: false,
            args: ruby_args(cx, n),
        })
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        if kind != "class" && kind != "module" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let mut impls = Vec::new();
        if let Some((name, pos)) = superclass_name(cx, n) {
            impls.push(ImplRef {
                name,
                pos,
                is_generic: false,
                child_is_abstract: false,
            });
        }
        collect_mixins(cx, n, &mut impls);
        Some(TypeHead {
            def: Some(TypeDef {
                name: last_segment(text(cx, name_node), &["::"]),
                pos: pos_of(name_node),
                visibility: Visibility::Exported,
                abstraction: kind == "module",
            }),
            impls,
            container: Some(Container::Class {
                exported: true,
                implements: false,
            }),
        })
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        match n.kind() {
            "when" => None,
            "in_clause" => {
                let pattern = child_by_field(n, "pattern")?;
                if text(cx, pattern) != "_" || child_by_field(n, "guard").is_some() {
                    return None;
                }
                let parent = cx.parent()?;
                Some(WildcardArm {
                    pos: pos_of(n),
                    enum_like: has_constant_pattern(pattern),
                    scrutinee: case_scrutinee_kind(parent),
                    variant_count: sibling_variant_count(cx, n),
                    has_nullable_ctor: false,
                })
            }
            "else" => {
                let parent = cx
                    .parent()
                    .filter(|p| p.kind() == "case" || p.kind() == "case_match")?;
                Some(WildcardArm {
                    pos: pos_of(n),
                    enum_like: sibling_has_constant(cx, n),
                    scrutinee: case_scrutinee_kind(parent),
                    variant_count: sibling_variant_count(cx, n),
                    has_nullable_ctor: false,
                })
            }
            _ => None,
        }
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        match n.kind() {
            "rescue" => {
                let binding = child_by_field(n, "variable")
                    .map(|v| strip_arrow(text(cx, v)))
                    .unwrap_or("");
                let Some(body) = child_by_field(n, "body") else {
                    return Some(Handler {
                        pos: pos_of(n),
                        stmts: Vec::new(),
                        has_else: false,
                        try_imports_only: false,
                        in_cleanup: false,
                    });
                };
                let then_body = body.kind() == "then" || body.kind() == "body_statement";
                let kids: Vec<Node<'_>> = if then_body {
                    named_children(body)
                } else {
                    vec![body]
                };
                let mut stmts = Vec::new();
                for stmt in kids {
                    if stmt.kind() == "comment" {
                        continue;
                    }
                    stmts.push(ruby_stmt(cx, stmt, binding));
                }
                Some(Handler {
                    pos: pos_of(n),
                    stmts,
                    has_else: false,
                    try_imports_only: false,
                    in_cleanup: false,
                })
            }
            "rescue_modifier" => {
                let body = child_by_field(n, "handler")?;
                let then_body = body.kind() == "then" || body.kind() == "body_statement";
                let kids: Vec<Node<'_>> = if then_body {
                    named_children(body)
                } else {
                    vec![body]
                };
                let mut stmts = Vec::new();
                for stmt in kids {
                    if stmt.kind() == "comment" {
                        continue;
                    }
                    if stmt.kind() == "call" {
                        stmts.push(Stmt::Call {
                            callee: ruby_callee(cx, stmt),
                            kind: ruby_call_kind(cx, stmt),
                        });
                    } else {
                        stmts.push(Stmt::Other);
                    }
                }
                Some(Handler {
                    pos: pos_of(n),
                    stmts,
                    has_else: false,
                    try_imports_only: false,
                    in_cleanup: false,
                })
            }
            _ => None,
        }
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() == "hash" {
            let pairs: Vec<Node<'_>> = named_children(n)
                .into_iter()
                .filter(|c| c.kind() == "pair")
                .collect();
            return hash_copy(cx, n, &pairs);
        }
        if n.kind() != "call" {
            return None;
        }
        let mut pairs = Vec::new();
        for arg in call_args(n) {
            if arg.kind() == "pair" {
                pairs.push(arg);
            } else if arg.kind() == "hash" {
                for inner in named_children(arg) {
                    if inner.kind() == "pair" {
                        pairs.push(inner);
                    }
                }
            } else if arg.kind() != "comment" {
                return None;
            }
        }
        hash_copy(cx, n, &pairs)
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "element_reference" {
            let Some(obj) = child_by_field(n, "object") else {
                return;
            };
            if text(cx, obj) != "ENV" {
                return;
            }
            out.push((AmbientKind::EnvRead, ruby_env_name(cx, n), pos_of(n)));
            return;
        }
        if n.kind() == "call" {
            let Some(recv) = child_by_field(n, "receiver") else {
                return;
            };
            if text(cx, recv) != "ENV" {
                return;
            }
            let method = field_text(cx, n, "method").unwrap_or("");
            if matches!(
                method,
                "fetch" | "key?" | "has_key?" | "include?" | "values" | "to_h" | "to_a" | "dig"
            ) {
                out.push((AmbientKind::EnvRead, text(cx, recv), pos_of(n)));
            }
            return;
        }
        if n.kind() != "global_variable" && n.kind() != "class_variable" {
            return;
        }
        if !is_assign_left(cx, n) {
            return;
        }
        let kind = if in_function {
            AmbientKind::GlobalWrite
        } else {
            AmbientKind::GlobalMutable
        };
        out.push((kind, text(cx, n), pos_of(n)));
    }
    fn call_marker(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        if n.kind() != "call" {
            return None;
        }
        if !matches!(
            field_text(cx, n, "method").unwrap_or(""),
            "it" | "specify" | "example" | "describe" | "context"
        ) {
            return None;
        }
        if let Some(recv) = child_by_field(n, "receiver")
            && text(cx, recv) != "RSpec"
        {
            return None;
        }
        Some(CodeRole::Test)
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "call" {
            return None;
        }
        match field_text(cx, n, "method").unwrap_or("") {
            "send" => Some("send dispatch"),
            "public_send" => Some("public_send dispatch"),
            "__send__" => Some("__send__ dispatch"),
            "define_method" => Some("define_method dispatch"),
            "define_singleton_method" => Some("define_singleton_method dispatch"),
            "method_missing" => Some("method_missing dispatch"),
            "const_get" => Some("const_get dispatch"),
            "const_set" => Some("const_set dispatch"),
            "instance_variable_get" => Some("instance_variable_get dispatch"),
            "instance_variable_set" => Some("instance_variable_set dispatch"),
            "class_variable_get" => Some("class_variable_get dispatch"),
            "class_variable_set" => Some("class_variable_set dispatch"),
            "eval" => Some("eval dispatch"),
            "instance_eval" => Some("instance_eval dispatch"),
            "class_eval" => Some("class_eval dispatch"),
            "module_eval" => Some("module_eval dispatch"),
            _ => None,
        }
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call" {
            return None;
        }
        let method = field_text(cx, n, "method").unwrap_or("");
        if method == "to" || method == "not_to" || method == "to_not" {
            return rspec_assertion(cx, n);
        }
        let arity = minitest_arity(method)?;
        let has_message = first_of_kind(n, &["argument_list"]).is_some_and(|list| {
            let last_str = rfind_named_child(list, |c| c.kind() != "comment")
                .is_some_and(|a| a.kind() == "string" || a.kind() == "heredoc_body");
            count_named_child(list, |c| c.kind() != "comment") > arity && last_str
        });
        let shape = if method == "assert" || method == "refute" {
            bare_assert_shape(cx, n)
        } else {
            AssertShape::Other
        };
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison: arity > 1,
            shape,
            in_typecheck: false,
            value_is_bool_literal: false,
        })
    }
}
