use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Param, Pos, Stmt,
    TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    first_of_kind, has_child_kind, last_segment, named_children, narrow, pos_of,
    preceding_comments, slice, string_arg_shape, strip_quotes, text, trimmed,
};
use crate::spec::Role;

/// Role table mapping Ruby node kinds to walker roles.
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

/// Ruby language hooks.
pub(crate) static HOOKS: RubyHooks = RubyHooks;

/// Ruby adapter.
pub(crate) struct RubyHooks;

fn node_id(n: Node<'_>) -> usize {
    n.id()
}
fn field_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> Option<&'s str> {
    child_by_field(n, field).map(|v| text(cx, v))
}
fn bare_marker_call<'s>(cx: &Cx<'_, 's>, sib: Node<'_>) -> Option<bool> {
    let method = field_text(cx, sib, "method")?;
    if child_by_field(sib, "receiver").is_some() {
        return None;
    }
    if let Some(list) = first_of_kind(sib, &["argument_list"])
        && !named_children(list).is_empty()
    {
        return None;
    }
    match method {
        "private" | "protected" | "module_function" => Some(false),
        "public" => Some(true),
        _ => None,
    }
}
fn wrapped_by_marker<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    if parent.kind() != "argument_list" {
        return false;
    }
    let Some(grand) = parent.parent() else {
        return false;
    };
    if grand.kind() != "call" {
        return false;
    }
    field_text(cx, grand, "method")
        .is_some_and(|m| matches!(m, "private" | "protected" | "module_function"))
}
fn ruby_public<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    if n.kind() == "singleton_method" {
        return true;
    }
    if wrapped_by_marker(cx, n) {
        return false;
    }
    let mut current = n.prev_sibling();
    while let Some(sib) = current {
        if sib.kind() == "identifier" {
            let t = text(cx, sib);
            if t == "private" || t == "protected" {
                return false;
            }
            if t == "public" {
                return true;
            }
        } else if sib.kind() == "call"
            && let Some(public) = bare_marker_call(cx, sib)
        {
            return public;
        }
        current = sib.prev_sibling();
    }
    true
}
fn is_protocol_method(name: &str) -> bool {
    matches!(
        name,
        "initialize"
            | "to_s"
            | "to_str"
            | "to_i"
            | "to_a"
            | "to_h"
            | "inspect"
            | "hash"
            | "eql?"
            | "=="
            | "!="
            | "<=>"
            | "<"
            | ">"
            | "<="
            | ">="
            | "each"
            | "call"
            | "[]"
            | "[]="
            | "<<"
            | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "succ"
            | "next"
            | "length"
            | "size"
            | "empty?"
            | "coerce"
            | "method_missing"
            | "respond_to_missing?"
    )
}
fn ruby_in_test<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    if cx.is_test_file {
        return true;
    }
    let mut current = n.parent();
    while let Some(parent) = current {
        if parent.kind() == "superclass" {
            return matches!(last_segment(text(cx, parent), &["::"]), "Test" | "TestCase");
        }
        current = parent.parent();
    }
    false
}
fn push_plain_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, out: &mut Vec<Param<'s>>) {
    let name = field_text(cx, child, "name").unwrap_or_else(|| text(cx, child));
    out.push(Param {
        name,
        ty: None,
        bool_default: false,
    });
}
fn ruby_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let params = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "method_parameters");
    let Some(params) = params else {
        return out;
    };
    for child in named_children(params) {
        match child.kind() {
            "identifier" => out.push(Param {
                name: text(cx, child),
                ty: None,
                bool_default: false,
            }),
            "optional_parameter" | "keyword_parameter" => {
                let name = field_text(cx, child, "name").unwrap_or("");
                let default = child_by_field(child, "value")
                    .map(|v| trimmed(cx, v))
                    .unwrap_or("");
                out.push(Param {
                    name,
                    ty: None,
                    bool_default: default == "true" || default == "false",
                });
            }
            "destructured_parameter" | "forward_parameter" => {
                out.push(Param {
                    name: text(cx, child),
                    ty: None,
                    bool_default: false,
                });
            }
            _ => push_plain_param(cx, child, &mut out),
        }
    }
    out
}
fn ruby_sole_call(n: Node<'_>) -> Option<usize> {
    let body = child_by_field(n, "body")?;
    if body.kind() == "rescue_modifier" {
        return None;
    }
    let stmts: Vec<Node<'_>> = if body.kind() == "body_statement" {
        named_children(body)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect()
    } else {
        vec![body]
    };
    if stmts.len() != 1 {
        return None;
    }
    let mut stmt = *stmts.first()?;
    if stmt.kind() == "return" {
        stmt = match named_children(stmt).as_slice() {
            [one] => *one,
            _ => return None,
        };
    }
    if stmt.kind() != "call" {
        return None;
    }
    if has_child_kind(stmt, "block") || has_child_kind(stmt, "do_block") {
        return None;
    }
    Some(node_id(stmt))
}
fn ruby_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    match (child_by_field(n, "receiver"), child_by_field(n, "method")) {
        (Some(recv), Some(method)) => slice(cx.src, recv.start_byte(), method.end_byte()),
        (None, Some(method)) => text(cx, method),
        _ => "",
    }
}
fn ruby_call_kind<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> CallKind {
    let method = field_text(cx, n, "method").unwrap_or("");
    if method == "new" && child_by_field(n, "receiver").is_some() {
        CallKind::New
    } else {
        CallKind::Plain
    }
}
fn ruby_receiver_root<'s>(cx: &Cx<'_, 's>, recv: Node<'_>) -> &'s str {
    let mut current = recv;
    loop {
        if current.kind() != "call" {
            return text(cx, current);
        }
        match child_by_field(current, "receiver") {
            Some(next) => current = next,
            None => return text(cx, current),
        }
    }
}
fn call_args(n: Node<'_>) -> Vec<Node<'_>> {
    match first_of_kind(n, &["argument_list"]) {
        Some(list) => named_children(list)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect(),
        None => Vec::new(),
    }
}
fn ruby_arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "heredoc_body" => string_arg_shape(cx, n, &["heredoc_content"]),
        "chained_string" => ArgShape::Concat,
        "binary" => {
            if binary_concat(cx, n, "+", &["string", "heredoc_body", "chained_string"]) {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}
fn ruby_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    call_args(n)
        .into_iter()
        .map(|arg| Arg {
            shape: ruby_arg_shape(cx, arg),
            start_line: narrow(arg.start_position().row) + 1,
            end_line: narrow(arg.end_position().row) + 1,
        })
        .collect()
}
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
                });
            }
        }
    }
}
fn has_constant_pattern(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        let kind = current.kind();
        if kind == "constant" || kind == "scope_resolution" {
            return true;
        }
        stack.extend(named_children(current));
    }
    false
}
fn sibling_has_constant(n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    for sib in named_children(parent) {
        if sib.id() == n.id() || (sib.kind() != "when" && sib.kind() != "in_clause") {
            continue;
        }
        if let Some(pattern) = child_by_field(sib, "pattern") {
            if has_constant_pattern(pattern) {
                return true;
            }
            continue;
        }
        for child in named_children(sib) {
            if child.kind() == "pattern" && has_constant_pattern(child) {
                return true;
            }
        }
    }
    false
}
fn strip_arrow(t: &str) -> &str {
    t.trim()
        .strip_prefix("=>")
        .map(str::trim)
        .unwrap_or(t.trim())
}
fn ruby_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: &str) -> Stmt<'s> {
    if stmt.kind() == "identifier" && text(cx, stmt) == "raise" {
        return Stmt::Rethrow;
    }
    if stmt.kind() != "call" {
        return Stmt::Other;
    }
    let method = field_text(cx, stmt, "method").unwrap_or("");
    if method != "raise" {
        return Stmt::Call {
            callee: ruby_callee(cx, stmt),
            kind: ruby_call_kind(cx, stmt),
        };
    }
    match first_of_kind(stmt, &["argument_list"]).map(call_args) {
        None => Stmt::Rethrow,
        Some(args) if args.is_empty() => Stmt::Rethrow,
        Some(args) => {
            let single = matches!(args.as_slice(), [one] if text(cx, *one) == binding);
            if !binding.is_empty() && single {
                Stmt::Rethrow
            } else {
                Stmt::Call {
                    callee: ruby_callee(cx, stmt),
                    kind: CallKind::Plain,
                }
            }
        }
    }
}
fn pair_key<'s>(cx: &Cx<'_, 's>, key: Node<'_>) -> Option<&'s str> {
    match key.kind() {
        "hash_key_symbol" => Some(text(cx, key)),
        "simple_symbol" => text(cx, key).strip_prefix(':'),
        "string" => Some(strip_quotes(text(cx, key))),
        _ => None,
    }
}
fn pair_source<'s>(cx: &Cx<'_, 's>, value: Node<'_>, key: &str) -> Option<&'s str> {
    if value.kind() != "call" {
        return None;
    }
    let recv = child_by_field(value, "receiver")?;
    if recv.kind() != "identifier" || field_text(cx, value, "method") != Some(key) {
        return None;
    }
    if let Some(list) = first_of_kind(value, &["argument_list"])
        && !named_children(list).is_empty()
    {
        return None;
    }
    if has_child_kind(value, "block") || has_child_kind(value, "do_block") {
        return None;
    }
    Some(text(cx, recv))
}
fn hash_copy<'s>(cx: &Cx<'_, 's>, scope: Node<'_>, pairs: &[Node<'_>]) -> Option<FieldCopy<'s>> {
    if pairs.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    for pair in pairs {
        let kids = named_children(*pair);
        let [key_node, val_node] = kids.as_slice() else {
            return None;
        };
        let found = pair_source(cx, *val_node, pair_key(cx, *key_node)?)?;
        if source.is_some_and(|s| s != found) {
            return None;
        }
        source = Some(found);
    }
    Some(FieldCopy {
        pos: pos_of(scope),
        fields: narrow(pairs.len()),
        source,
    })
}
fn is_assign_left(n: Node<'_>) -> bool {
    n.parent()
        .is_some_and(|p| p.kind() == "assignment" || p.kind() == "operator_assignment")
}
fn ruby_env_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    for child in named_children(n) {
        if matches!(
            child.kind(),
            "string" | "simple_symbol" | "identifier" | "constant"
        ) {
            return text(cx, child);
        }
    }
    "ENV"
}
fn minitest_arity(method: &str) -> Option<usize> {
    let rest = method
        .strip_prefix("assert")
        .or_else(|| method.strip_prefix("refute"))?;
    if !rest.is_empty() && !rest.starts_with('_') {
        return None;
    }
    let base = method
        .strip_prefix("assert_")
        .or_else(|| method.strip_prefix("refute_"))
        .unwrap_or(method);
    Some(match base {
        "operator" | "compare" => 3,
        "equal" | "not_equal" | "same" | "not_same" | "includes" | "not_includes" | "match"
        | "no_match" | "in_delta" | "in_epsilon" => 2,
        _ => 1,
    })
}
fn rspec_assertion<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Assertion> {
    let recv = child_by_field(n, "receiver")?;
    if recv.kind() != "call" || field_text(cx, recv, "method") != Some("expect") {
        return None;
    }
    let args = call_args(n);
    let first = args.first()?;
    let matcher = if first.kind() == "call" {
        field_text(cx, *first, "method").unwrap_or("")
    } else {
        ""
    };
    let last_str = args
        .last()
        .is_some_and(|a| a.kind() == "string" || a.kind() == "heredoc_body");
    Some(Assertion {
        pos: pos_of(n),
        has_message: args.len() >= 2 && last_str,
        comparison: matches!(
            matcher,
            "eq" | "eql"
                | "equal"
                | "be"
                | "be_within"
                | "include"
                | "match"
                | "contain_exactly"
                | "start_with"
                | "end_with"
        ),
    })
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
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public: ruby_public(cx, n),
            interface_bound: is_protocol_method(name),
            in_test: ruby_in_test(cx, n),
            params: ruby_params(cx, n),
            return_type: None,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: ruby_sole_call(n),
        })
    }
    fn is_else_if(&self, _n: Node<'_>) -> bool {
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
                args: ruby_args(cx, n),
            });
        }
        Some(CallHead {
            callee: ruby_callee(cx, n),
            kind: CallKind::Plain,
            callee_root: receiver.map(|r| ruby_receiver_root(cx, r)),
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
        if let Some(sup) = child_by_field(n, "superclass") {
            impls.push(ImplRef {
                name: last_segment(text(cx, sup), &["::"]),
                pos: pos_of(sup),
            });
        }
        collect_mixins(cx, n, &mut impls);
        Some(TypeHead {
            def: Some(TypeDef {
                name: last_segment(text(cx, name_node), &["::"]),
                pos: pos_of(name_node),
                public: true,
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
                Some(WildcardArm {
                    pos: pos_of(n),
                    enum_like: has_constant_pattern(pattern),
                })
            }
            "else" => {
                let wild = n
                    .parent()
                    .is_some_and(|p| p.kind() == "case" || p.kind() == "case_match");
                if !wild {
                    return None;
                }
                Some(WildcardArm {
                    pos: pos_of(n),
                    enum_like: sibling_has_constant(n),
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
        if !is_assign_left(n) {
            return;
        }
        let kind = if in_function {
            AmbientKind::GlobalWrite
        } else {
            AmbientKind::GlobalMutable
        };
        out.push((kind, text(cx, n), pos_of(n)));
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
        let args = call_args(n);
        let last_str = args
            .last()
            .is_some_and(|a| a.kind() == "string" || a.kind() == "heredoc_body");
        Some(Assertion {
            pos: pos_of(n),
            has_message: args.len() > arity && last_str,
            comparison: arity > 1,
        })
    }
}
