use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, Param, Pos, Stmt, TypeDef,
    WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    find_named_child, first_named_child, first_named_child_matching, first_of_kind,
    last_named_child, named_children, narrow, only_named_child_matching, pos_of,
    preceding_comments, slice, strip_quotes, text, trimmed,
};
use crate::spec::Role;

/// Role table mapping PowerShell node kinds to walker roles.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_statement", Role::Function),
    ("class_method_definition", Role::Function),
    ("script_block_expression", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("elseif_clause", Role::ElseIf),
    ("switch_statement", Role::Match),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("for_statement", Role::Loop),
    ("foreach_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("command", Role::Call),
    ("invokation_expression", Role::Call),
    ("assignment_expression", Role::Bind),
    ("pre_increment_expression", Role::Write),
    ("post_increment_expression", Role::Write),
    ("pre_decrement_expression", Role::Write),
    ("post_decrement_expression", Role::Write),
    ("class_statement", Role::TypeDef),
    ("enum_statement", Role::TypeDef),
    ("comment", Role::Comment),
    ("switch_clause", Role::MatchArm),
    ("hash_literal_expression", Role::StructLit),
    ("variable", Role::Global),
];

/// PowerShell language hooks.
pub(crate) static HOOKS: PowerShellHooks = PowerShellHooks;

/// PowerShell adapter.
pub(crate) struct PowerShellHooks;

fn node_id(n: Node<'_>) -> usize {
    n.id()
}
fn ps_var_name(t: &str) -> &str {
    let s = t.strip_prefix('$').unwrap_or(t);
    let s = s
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .unwrap_or(s);
    match s.rfind(':') {
        Some(at) => &s[at + 1..],
        None => s,
    }
}
fn ps_param_ty<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<&'s str> {
    if let Some(found) = first_of_kind(p, &["type_literal"]) {
        return Some(text(cx, found));
    }
    let var = first_of_kind(p, &["variable"])?;
    let full = text(cx, p);
    let end = var
        .start_byte()
        .saturating_sub(p.start_byte())
        .min(full.len());
    let before = &full[..end];
    let close = before.rfind(']')?;
    let open = before[..close].rfind('[')?;
    Some(&before[open..=close])
}
fn push_ps_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>, out: &mut Vec<Param<'s>>) {
    let Some(var) = first_of_kind(p, &["variable"]) else {
        return;
    };
    let name = ps_var_name(text(cx, var));
    let ty = ps_param_ty(cx, p);
    let bool_default = first_of_kind(p, &["script_parameter_default"]).is_some_and(|d| {
        let t = trimmed(cx, d);
        t == "$true" || t == "$false"
    });
    out.push(Param {
        name,
        ty,
        bool_default,
    });
}
fn collect_script_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        if child.kind() == "script_parameter" {
            push_ps_param(cx, child, out);
        }
    }
}
fn ps_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        match child.kind() {
            "script_parameter" => push_ps_param(cx, child, &mut out),
            "script_block" => {
                for sub in named_children(child) {
                    if sub.kind() == "param_block" {
                        collect_script_params(cx, sub, &mut out);
                    }
                }
            }
            "function_parameter_declaration" => {
                if let Some(list) = first_of_kind(child, &["parameter_list"]) {
                    collect_script_params(cx, list, &mut out);
                }
            }
            "function_name" | "simple_name" | "comment" | "class_attribute" => {}
            _ => {
                if !matches!(
                    child.kind(),
                    "function_statement" | "class_method_definition" | "script_block_expression"
                ) {
                    collect_script_params(cx, child, &mut out);
                }
            }
        }
    }
    out
}
fn enclosing_class<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let mut current = n.parent();
    while let Some(parent) = current {
        if parent.kind() == "class_statement" {
            return first_of_kind(parent, &["simple_name"])
                .map(|v| text(cx, v))
                .unwrap_or("");
        }
        current = parent.parent();
    }
    ""
}
fn single_named(n: Node<'_>) -> Option<Node<'_>> {
    only_named_child_matching(n, |c| c.kind() != "comment")
}
fn ps_sole_call(n: Node<'_>) -> Option<usize> {
    let body = first_of_kind(n, &["script_block"])?;
    let inner = first_of_kind(body, &["script_block_body"]).unwrap_or(body);
    let list = first_of_kind(inner, &["statement_list"]).unwrap_or(inner);
    let mut stmt = single_named(list)?;
    loop {
        let kind = stmt.kind();
        if kind != "pipeline" && kind != "pipeline_chain" {
            break;
        }
        stmt = single_named(stmt)?;
    }
    (stmt.kind() == "command" || stmt.kind() == "invokation_expression").then(|| node_id(stmt))
}
fn ps_string_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if any_named_child(n, |c| {
        c.kind() == "variable" || c.kind() == "sub_expression"
    }) {
        let parts: Vec<&'s str> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() != "variable" && c.kind() != "sub_expression")
            .map(|c| text(cx, c))
            .collect();
        ArgShape::Str {
            longest_static: longest_static_chunk(&parts),
        }
    } else {
        ArgShape::Str {
            longest_static: narrow(text(cx, n).chars().count().saturating_sub(2)),
        }
    }
}
fn ps_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => ps_string_shape(cx, n),
        "generic_token" => ArgShape::Str {
            longest_static: narrow(text(cx, n).chars().count()),
        },
        "variable" => ArgShape::Ident(ps_var_name(text(cx, n))),
        "command_parameter" => ArgShape::Other,
        "additive_expression" => {
            if binary_concat(cx, n, "+", &["string_literal"]) {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        _ => ArgShape::Other,
    }
}
fn ps_span_arg<'s>(cx: &Cx<'_, 's>, shape_node: Node<'_>, span_node: Node<'_>) -> Arg<'s> {
    Arg {
        shape: ps_shape(cx, shape_node),
        start_line: narrow(span_node.start_position().row) + 1,
        end_line: narrow(span_node.end_position().row) + 1,
    }
}
fn ps_arg<'s>(cx: &Cx<'_, 's>, arg: Node<'_>) -> Arg<'s> {
    let value = unwrap_operand(arg);
    let shaped = if value.kind() == "variable" {
        value
    } else {
        arg
    };
    ps_span_arg(cx, shaped, arg)
}
fn pipeline_operand(n: Node<'_>) -> Option<Node<'_>> {
    let parent = n.parent()?;
    if parent.kind() != "pipeline_chain" {
        return None;
    }
    let mut prev = n.prev_named_sibling()?;
    while matches!(prev.kind(), "comment" | "redirections") {
        prev = prev.prev_named_sibling()?;
    }
    Some(prev)
}
fn unwrap_operand(mut n: Node<'_>) -> Node<'_> {
    loop {
        let transparent = matches!(
            n.kind(),
            "logical_expression"
                | "bitwise_expression"
                | "comparison_expression"
                | "additive_expression"
                | "multiplicative_expression"
                | "format_expression"
                | "range_expression"
                | "array_literal_expression"
                | "unary_expression"
                | "parenthesized_expression"
                | "pipeline"
                | "pipeline_chain"
        );
        if !transparent || n.named_child_count() != 1 {
            break;
        }
        let Some(one) = n.named_child(0) else {
            break;
        };
        if one.kind() == "comment" {
            break;
        }
        n = one;
    }
    n
}
fn ps_command_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
    let mut args = Vec::new();
    match first_of_kind(n, &["command_elements"]) {
        Some(list) => {
            for arg in named_children(list) {
                if arg.kind() != "comment" && arg.kind() != "command_argument_sep" {
                    args.push(ps_arg(cx, arg));
                }
            }
        }
        None => {
            for child in named_children(n) {
                if child.id() != name.id() && child.kind() != "comment" {
                    args.push(ps_arg(cx, child));
                }
            }
        }
    }
    if args.is_empty()
        && let Some(operand) = pipeline_operand(n)
    {
        args.push(ps_span_arg(cx, unwrap_operand(operand), operand));
    }
    Some(CallHead {
        callee: text(cx, name),
        kind: CallKind::Plain,
        callee_root: None,
        args,
    })
}
fn invokation_receiver(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |c| {
        c.kind() != "member_name" && c.kind() != "argument_list"
    })
}
fn ps_receiver_root<'s>(cx: &Cx<'_, 's>, recv: Node<'_>) -> Option<&'s str> {
    let mut current = recv;
    loop {
        match current.kind() {
            "variable" => return Some(ps_var_name(text(cx, current))),
            "member_access" | "element_access" => {
                let next = first_named_child(current)?;
                current = next
            }
            _ => return None,
        }
    }
}
fn ps_invokation_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let member = first_of_kind(n, &["member_name"])?;
    let member_text = text(cx, member);
    let recv = invokation_receiver(n);
    let mut kind = CallKind::Plain;
    let callee: &'s str = match recv {
        Some(r) => {
            if member_text == "new" && r.kind() == "type_literal" {
                kind = CallKind::New;
                let t = text(cx, r);
                t.strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or(t)
            } else {
                slice(cx.src, r.start_byte(), member.end_byte())
            }
        }
        None => member_text,
    };
    let mut args = Vec::new();
    if let Some(list) = first_of_kind(n, &["argument_list"]) {
        let exprs = first_of_kind(list, &["argument_expression_list"]).unwrap_or(list);
        for arg in named_children(exprs) {
            if arg.kind() != "comment" {
                args.push(ps_arg(cx, arg));
            }
        }
    }
    Some(CallHead {
        callee,
        kind,
        callee_root: recv.and_then(|r| ps_receiver_root(cx, r)),
        args,
    })
}
fn is_plain_ps_var(t: &str) -> bool {
    if !t.starts_with('$') {
        return false;
    }
    let s = ps_var_name(t);
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
    {
        return false;
    }
    !s.starts_with("env:")
        && !s.starts_with("global:")
        && !s.starts_with("script:")
        && !s.starts_with("using:")
        && !s.starts_with("local:")
}
fn write_operand_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut current = match first_named_child(n) {
        Some(first) => first,
        None => return,
    };
    loop {
        match current.kind() {
            "variable" => {
                out.push((ps_var_name(text(cx, current)), pos_of(current)));
                return;
            }
            "unary_expression"
            | "member_access"
            | "element_access"
            | "parenthesized_expression" => match first_named_child(current) {
                Some(next) => current = next,
                None => return,
            },
            _ => return,
        }
    }
}
fn sibling_enum_like(n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    for sib in named_children(parent) {
        if sib.id() == n.id() || sib.kind() != "switch_clause" {
            continue;
        }
        if let Some(cond) = first_of_kind(sib, &["switch_clause_condition"]) {
            let member = any_named_child(cond, |c| c.kind() == "member_access");
            let typed = any_named_child(cond, |c| c.kind() == "type_literal");
            if member && typed {
                return true;
            }
        }
    }
    false
}
fn ps_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>) -> Stmt<'s> {
    if stmt.kind() == "command" {
        return match ps_command_call(cx, stmt) {
            Some(head) => Stmt::Call {
                callee: head.callee,
                kind: head.kind,
            },
            None => Stmt::Other,
        };
    }
    if stmt.kind() == "invokation_expression" {
        return match ps_invokation_call(cx, stmt) {
            Some(head) => Stmt::Call {
                callee: head.callee,
                kind: head.kind,
            },
            None => Stmt::Other,
        };
    }
    if stmt.kind() == "pipeline" || stmt.kind() == "pipeline_chain" {
        return match single_named(stmt) {
            Some(one) => ps_stmt(cx, one),
            None => Stmt::Other,
        };
    }
    if stmt.kind() == "flow_control_statement" && trimmed(cx, stmt).starts_with("throw") {
        return match last_named_child(stmt) {
            None => Stmt::Rethrow,
            Some(last) => {
                let t = text(cx, last);
                let known = t == "$_" || t == "$PSItem" || t == "$_.Exception";
                if known { Stmt::Rethrow } else { Stmt::Other }
            }
        };
    }
    Stmt::Other
}
fn unwrap_pipeline(n: Node<'_>) -> Node<'_> {
    let mut current = n;
    loop {
        if current.kind() != "pipeline" && current.kind() != "pipeline_chain" {
            return current;
        }
        match single_named(current) {
            Some(one) => current = one,
            None => return current,
        }
    }
}
fn subtree_has_kind(n: Node<'_>, kind: &str) -> bool {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        if current.kind() == kind {
            return true;
        }
        stack.extend(named_children(current));
    }
    false
}
fn is_comparison_param(t: &str) -> bool {
    matches!(
        t,
        "-Be"
            | "-BeExactly"
            | "-BeGreaterThan"
            | "-BeGreaterOrEqual"
            | "-BeLessThan"
            | "-BeLessOrEqual"
            | "-BeLike"
            | "-Match"
            | "-Contain"
    )
}
impl LangHooks for PowerShellHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let kind = n.kind();
        if kind != "function_statement" && kind != "class_method_definition" {
            return None;
        }
        let want = if kind == "function_statement" {
            "function_name"
        } else {
            "simple_name"
        };
        let name_node = first_of_kind(n, &[want])?;
        let name = text(cx, name_node);
        let public = if kind == "function_statement" {
            true
        } else {
            !any_named_child(n, |c| {
                c.kind() == "class_attribute" && trimmed(cx, c) == "hidden"
            })
        };
        let return_type = if kind == "class_method_definition" {
            first_of_kind(n, &["type_literal"]).map(|t| text(cx, t))
        } else {
            None
        };
        let params = ps_params(cx, n);
        let mut missing: Vec<&'s str> = Vec::new();
        for param in &params {
            if param.ty.is_none() {
                missing.push(param.name);
            }
        }
        let ctor = name == enclosing_class(cx, n);
        if kind == "class_method_definition" && return_type.is_none() && !ctor {
            missing.push("return");
        }
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public,
            interface_bound: false,
            in_test: cx.is_test_file,
            params,
            return_type,
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: ps_sole_call(n),
        })
    }
    fn is_else_if(&self, _n: Node<'_>) -> bool {
        false
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment_expression" {
            return;
        }
        let Some(left) = first_named_child_matching(n, &["left_assignment_expression"]) else {
            return;
        };
        let t = trimmed(cx, left);
        if is_plain_ps_var(t) {
            out.push((ps_var_name(t), pos_of(left)));
        }
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if matches!(
            n.kind(),
            "pre_increment_expression"
                | "post_increment_expression"
                | "pre_decrement_expression"
                | "post_decrement_expression"
        ) {
            write_operand_root(cx, n, out);
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "command" {
            return ps_command_call(cx, n);
        }
        if n.kind() == "invokation_expression" {
            return ps_invokation_call(cx, n);
        }
        None
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        if kind != "class_statement" && kind != "enum_statement" {
            return None;
        }
        let name_node = first_of_kind(n, &["simple_name"])?;
        let container = if kind == "class_statement" {
            Some(Container::Class {
                exported: true,
                implements: false,
            })
        } else {
            None
        };
        Some(TypeHead {
            def: Some(TypeDef {
                name: text(cx, name_node),
                pos: pos_of(name_node),
                public: true,
                abstraction: false,
            }),
            impls: Vec::new(),
            container,
        })
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "switch_clause" {
            return None;
        }
        match first_of_kind(n, &["switch_clause_condition"]) {
            None => Some(WildcardArm {
                pos: pos_of(n),
                enum_like: sibling_enum_like(n),
            }),
            Some(cond) => (text(cx, cond) == "default").then_some(WildcardArm {
                pos: pos_of(n),
                enum_like: sibling_enum_like(n),
            }),
        }
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let block = first_of_kind(n, &["statement_block"])?;
        let list = first_of_kind(block, &["statement_list"]).unwrap_or(block);
        let mut stmts = Vec::new();
        for stmt in named_children(list) {
            if stmt.kind() == "comment" {
                continue;
            }
            stmts.push(ps_stmt(cx, stmt));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "hash_literal_expression" {
            return None;
        }
        let entries: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() == "hash_entry")
            .collect();
        if entries.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        for entry in entries {
            let key = find_named_child(entry, |c| c.kind() == "key_expression")?;
            let value = find_named_child(entry, |c| c.kind() != "key_expression")?;
            let want = strip_quotes(text(cx, key));
            let unwrapped = unwrap_pipeline(value);
            if unwrapped.kind() != "member_access" {
                return None;
            }
            let obj = first_named_child(unwrapped)?;
            if obj.kind() != "variable" {
                return None;
            }
            let member = first_of_kind(unwrapped, &["member_name"])?;
            if text(cx, member) != want {
                return None;
            }
            let found = ps_var_name(text(cx, obj));
            if source.is_some_and(|s| s != found) {
                return None;
            }
            source = Some(found);
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(n.named_child_count()),
            source,
        })
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() != "variable" {
            return;
        }
        let t = text(cx, n);
        if t.starts_with("$env:") || t.starts_with("${env:") {
            out.push((AmbientKind::EnvRead, t, pos_of(n)));
            return;
        }
        if t.starts_with("$global:")
            || t.starts_with("${global:")
            || t.starts_with("$script:")
            || t.starts_with("${script:")
            || t.starts_with("$using:")
            || t.starts_with("${using:")
        {
            let kind = if in_function {
                AmbientKind::GlobalWrite
            } else {
                AmbientKind::GlobalMutable
            };
            out.push((kind, t, pos_of(n)));
        }
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() == "command" {
            let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
            let t = text(cx, name);
            if t.eq_ignore_ascii_case("invoke-expression") || t.eq_ignore_ascii_case("iex") {
                return Some("Invoke-Expression");
            }
            if name.kind() == "command_name_expr" && subtree_has_kind(name, "variable") {
                return Some("computed command name");
            }
            return None;
        }
        if n.kind() == "invokation_expression" {
            let member = first_of_kind(n, &["member_name"])?;
            if text(cx, member) != "Create" {
                return None;
            }
            let recv = invokation_receiver(n)?;
            if text(cx, recv).eq_ignore_ascii_case("[scriptblock]") {
                return Some("scriptblock creation");
            }
        }
        None
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "command" {
            return None;
        }
        let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
        if text(cx, name) != "Should" {
            return None;
        }
        let mut has_message = false;
        let mut comparison = false;
        let kids = match first_of_kind(n, &["command_elements"]) {
            Some(list) => named_children(list),
            None => named_children(n),
        };
        for child in kids {
            if child.kind() != "command_parameter" {
                continue;
            }
            let t = trimmed(cx, child);
            if t.starts_with("-Because") {
                has_message = true;
            }
            if is_comparison_param(t) {
                comparison = true;
            }
        }
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison,
        })
    }
}
