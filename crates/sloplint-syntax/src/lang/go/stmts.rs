//! Go bindings, writes, handlers, literals, and ambient facts.

use sloplint_core::facts::{AmbientKind, CallKind, FieldCopy, Handler, Pos, Stmt};
use tree_sitter::Node;

use super::calls::go_callee;
use crate::hooks::{
    Cx, FileSetKind, child_by_field, first_named_child, first_named_child_matching, named_children,
    pos_of, slice, text,
};

pub(crate) fn go_decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "var_spec" {
        for c in named_children(n) {
            if c.kind() == "identifier" && text(cx, c) != "_" {
                out.push((text(cx, c), pos_of(c)));
            }
        }
        return;
    }
    if n.kind() == "var_declaration" {
        return;
    }
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    for id in named_children(left) {
        if id.kind() == "identifier" && text(cx, id) != "_" {
            out.push((text(cx, id), pos_of(id)));
        }
    }
    if left.kind() == "identifier" && text(cx, left) != "_" {
        out.push((text(cx, left), pos_of(left)));
    }
}

fn push_go_decl_list_names<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<&'s str>) {
    for id in named_children(list) {
        if id.kind() == "identifier" && text(cx, id) != "_" {
            out.push(text(cx, id));
        }
    }
    if list.kind() == "identifier" && text(cx, list) != "_" {
        out.push(text(cx, list));
    }
}

pub(crate) fn go_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "range_clause" | "receive_statement" => {
            let Some(left) = child_by_field(n, "left") else {
                return;
            };
            let Some(right) = child_by_field(n, "right") else {
                return;
            };
            if !slice(cx.src, left.end_byte(), right.start_byte()).contains(":=") {
                return;
            }
            push_go_decl_list_names(cx, left, out);
        }
        "type_switch_statement" => {
            let Some(alias) = child_by_field(n, "alias") else {
                return;
            };
            push_go_decl_list_names(cx, alias, out);
        }
        _ => {}
    }
}

fn push_go_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if let Some(root) = go_root(cx, n) {
        out.push(root);
    }
}

fn write_target<'s>(cx: &Cx<'_, 's>, c: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if c.kind() == "expression_list" {
        for e in named_children(c) {
            push_go_root(cx, e, out);
        }
    } else {
        push_go_root(cx, c, out);
    }
}

fn go_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some((text(cx, cur), pos_of(cur))),
            "selector_expression" => cur = child_by_field(cur, "operand")?,
            "index_expression" => cur = child_by_field(cur, "operand")?,
            "unary_expression" => cur = child_by_field(cur, "operand")?,
            "parenthesized_expression" => cur = first_named_child(cur)?,
            _ => return None,
        }
    }
}

pub(crate) fn go_write_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "assignment_statement" {
        let right = child_by_field(n, "right");
        for c in named_children(n) {
            if right.is_some_and(|r| r.id() == c.id()) {
                continue;
            }
            write_target(cx, c, out);
        }
        return;
    }
    if let Some(child) = first_named_child(n)
        && let Some(root) = go_root(cx, child)
    {
        out.push(root);
    }
}

fn go_nil_bound<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let cond = child_by_field(n, "condition")?;
    if cond.kind() != "binary_expression" {
        return None;
    }
    let left = child_by_field(cond, "left")?;
    if left.kind() != "identifier" {
        return None;
    }
    if child_by_field(cond, "operator").map_or("", |o| text(cx, o)) != "!=" {
        return None;
    }
    let right = child_by_field(cond, "right")?;
    if right.kind() != "nil" {
        return None;
    }
    Some(text(cx, left))
}

fn go_has_call(root: Node<'_>) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression" {
            return true;
        }
        let mut cursor = n.walk();
        stack.extend(n.children(&mut cursor));
    }
    false
}

fn go_lhs_names<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>) -> Vec<&'s str> {
    let kind = stmt.kind();
    if kind != "short_var_declaration" && kind != "assignment_statement" {
        return Vec::new();
    }
    let Some(left) = child_by_field(stmt, "left") else {
        return Vec::new();
    };
    named_children(left)
        .into_iter()
        .filter(|c| c.kind() == "identifier" && text(cx, *c) != "_")
        .map(|c| text(cx, c))
        .collect()
}

fn go_names_call_value(cx: &Cx<'_, '_>, stmt: Node<'_>, bound: &str) -> bool {
    let kind = stmt.kind();
    if kind != "short_var_declaration" && kind != "assignment_statement" {
        return false;
    }
    let Some(value) = child_by_field(stmt, "right") else {
        return false;
    };
    if !go_has_call(value) {
        return false;
    }
    go_lhs_names(cx, stmt).contains(&bound)
}

fn go_prev_sibling<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Option<Node<'t>> {
    let (_, earlier) = cx
        .levels
        .iter()
        .rev()
        .filter_map(|level| level.split_last())
        .find(|(last, _)| last.id() == n.id())?;
    earlier
        .iter()
        .rev()
        .find(|s| s.is_named() && s.kind() != "comment")
        .copied()
}

fn go_error_param(cx: &Cx<'_, '_>, decl: Node<'_>, bound: &str) -> bool {
    if decl.kind() != "parameter_declaration" {
        return false;
    }
    if !child_by_field(decl, "type").is_some_and(|t| text(cx, t) == "error") {
        return false;
    }
    named_children(decl)
        .into_iter()
        .any(|id| id.kind() == "identifier" && text(cx, id) == bound)
}

fn go_error_params(cx: &Cx<'_, '_>, list: Node<'_>, bound: &str) -> bool {
    named_children(list)
        .into_iter()
        .any(|decl| go_error_param(cx, decl, bound))
}

fn go_is_error_name(bound: &str) -> bool {
    bound == "err" || bound.ends_with("Err")
}

fn go_bound_is_error_param(cx: &Cx<'_, '_>, bound: &str) -> bool {
    for ancestor in cx.ancestors() {
        if matches!(
            ancestor.kind(),
            "function_declaration" | "method_declaration" | "func_literal"
        ) && child_by_field(ancestor, "parameters")
            .is_some_and(|list| go_error_params(cx, list, bound))
        {
            return true;
        }
    }
    false
}

fn go_bound_is_error_result(cx: &Cx<'_, '_>, bound: &str) -> bool {
    for ancestor in cx.ancestors() {
        if matches!(
            ancestor.kind(),
            "function_declaration" | "method_declaration" | "func_literal"
        ) && child_by_field(ancestor, "result")
            .is_some_and(|result| go_error_params(cx, result, bound))
        {
            return true;
        }
    }
    false
}

fn go_in_deferred_closure(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        match ancestor.kind() {
            "func_literal" => {
                let call = cx.parent_of(ancestor);
                let defer = call.and_then(|c| cx.parent_of(c));
                return call.is_some_and(|c| c.kind() == "call_expression")
                    && defer.is_some_and(|d| d.kind() == "defer_statement");
            }
            "function_declaration" | "method_declaration" => return false,
            _ => {}
        }
    }
    false
}

fn go_error_guarded(cx: &Cx<'_, '_>, n: Node<'_>, bound: &str) -> bool {
    if go_bound_is_error_param(cx, bound) || go_bound_is_error_result(cx, bound) {
        return true;
    }
    if !go_is_error_name(bound) {
        return false;
    }
    if let Some(init) = child_by_field(n, "initializer")
        && go_names_call_value(cx, init, bound)
    {
        return true;
    }
    if let Some(prev) = go_prev_sibling(cx, n)
        && go_names_call_value(cx, prev, bound)
    {
        return true;
    }
    false
}

fn go_mentions(cx: &Cx<'_, '_>, root: Node<'_>, bound: &str) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && text(cx, n) == bound {
            return true;
        }
        let mut cursor = n.walk();
        stack.extend(n.children(&mut cursor));
    }
    false
}

fn go_handler_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, bound: &str) -> Stmt<'s> {
    if s.kind() == "expression_statement" {
        if let Some(v) = first_named_child(s)
            && v.kind() == "call_expression"
        {
            let callee = child_by_field(v, "function").map_or("", |f| go_callee(cx, f));
            return Stmt::Call {
                callee,
                kind: CallKind::Plain,
            };
        }
        return Stmt::Other;
    }
    if s.kind() == "return_statement" && go_mentions(cx, s, bound) {
        return Stmt::Rethrow;
    }
    Stmt::Other
}

pub(crate) fn go_handler<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "if_statement" {
        return None;
    }
    let bound = go_nil_bound(cx, n)?;
    if !go_error_guarded(cx, n, bound) {
        return None;
    }
    let body = child_by_field(n, "consequence")?;
    let mut stmts = Vec::new();
    if let Some(list) = first_named_child_matching(body, &["statement_list"]) {
        for s in named_children(list) {
            if s.kind() == "comment" {
                continue;
            }
            stmts.push(go_handler_stmt(cx, s, bound));
        }
    }
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: child_by_field(n, "alternative").is_some(),
        try_imports_only: false,
        in_cleanup: go_in_deferred_closure(cx),
    })
}

fn go_keyed<'s>(cx: &Cx<'_, 's>, el: Node<'_>) -> Option<&'s str> {
    let key = unwrap_literal_element(child_by_field(el, "key")?);
    if key.kind() != "identifier" {
        return None;
    }
    let val = unwrap_literal_element(child_by_field(el, "value")?);
    if val.kind() != "selector_expression" {
        return None;
    }
    let obj = child_by_field(val, "operand")?;
    if obj.kind() != "identifier" {
        return None;
    }
    if child_by_field(val, "field").map_or("", |f| text(cx, f)) != text(cx, key) {
        return None;
    }
    Some(text(cx, obj))
}

fn unwrap_literal_element(n: Node<'_>) -> Node<'_> {
    if n.kind() == "literal_element" && n.named_child_count() == 1 {
        return n.named_child(0).unwrap_or(n);
    }
    n
}

pub(crate) fn go_struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "composite_literal" {
        return None;
    }
    let body = child_by_field(n, "body")?;
    let mut fields = 0u32;
    let mut source: Option<&'s str> = None;
    let mut poisoned = false;
    for el in named_children(body) {
        if el.kind() == "comment" {
            continue;
        }
        if el.kind() != "keyed_element" {
            poisoned = true;
            continue;
        }
        fields += 1;
        match (source, go_keyed(cx, el)) {
            (None, Some(s)) => source = Some(s),
            (Some(a), Some(b)) if a == b => {}
            _ => poisoned = true,
        }
    }
    if fields == 0 {
        return None;
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields,
        source: if poisoned { None } else { source },
    })
}

fn is_sentinel(cx: &Cx<'_, '_>, spec: Node<'_>) -> bool {
    child_by_field(spec, "value").is_some_and(|v| {
        let expr = if v.kind() == "expression_list" {
            v.named_child(0).unwrap_or(v)
        } else {
            v
        };
        expr.kind() == "call_expression"
            && child_by_field(expr, "function").is_some_and(|f| {
                let t = text(cx, f);
                t == "errors.New" || t == "fmt.Errorf"
            })
    })
}

fn go_var_specs(n: Node<'_>) -> Vec<Node<'_>> {
    let mut specs = Vec::new();
    for child in named_children(n) {
        if child.kind() == "var_spec" {
            specs.push(child);
            continue;
        }
        specs.extend(
            named_children(child)
                .into_iter()
                .filter(|s| s.kind() == "var_spec"),
        );
    }
    specs
}

fn push_go_global<'s>(cx: &Cx<'_, 's>, spec: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if is_sentinel(cx, spec) {
        return;
    }
    for c in named_children(spec) {
        if c.kind() != "identifier" {
            continue;
        }
        out.push((AmbientKind::GlobalMutable, text(cx, c), pos_of(c)));
    }
}

fn report_go_globals<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    for spec in go_var_specs(n) {
        push_go_global(cx, spec, out);
    }
}

fn report_go_env<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
    if callee != "os.Getenv" && callee != "os.LookupEnv" && callee != "os.Environ" {
        return;
    }
    out.push((AmbientKind::EnvRead, callee, pos_of(n)));
}

fn go_direct_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<(&'s str, Pos)> {
    let mut targets = Vec::new();
    if n.kind() == "assignment_statement" {
        if let Some(left) = child_by_field(n, "left") {
            for c in named_children(left) {
                if c.kind() == "identifier" && text(cx, c) != "_" {
                    targets.push((text(cx, c), pos_of(c)));
                }
            }
        }
        return targets;
    }
    if let Some(operand) = first_named_child(n)
        && operand.kind() == "identifier"
        && text(cx, operand) != "_"
    {
        targets.push((text(cx, operand), pos_of(operand)));
    }
    targets
}

fn go_spec_names<'s>(cx: &Cx<'_, 's>, spec: Node<'_>, out: &mut Vec<&'s str>) {
    for c in named_children(spec) {
        if c.kind() == "identifier" && text(cx, c) != "_" {
            out.push(text(cx, c));
        }
    }
}

fn report_go_write<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let targets = go_direct_targets(cx, n);
    if targets.is_empty() {
        return;
    }
    let globals = cx.file_names(FileSetKind::Scope, || {
        let mut names = Vec::new();
        if let Some(root) = cx.root() {
            for child in named_children(root) {
                if child.kind() != "var_declaration" {
                    continue;
                }
                for spec in go_var_specs(child) {
                    go_spec_names(cx, spec, &mut names);
                }
            }
        }
        names
    });
    for (name, pos) in targets {
        if globals.contains(&name) {
            out.push((AmbientKind::GlobalWrite, name, pos));
        }
    }
}

pub(crate) fn go_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "var_declaration" {
        if in_function {
            return;
        }
        if cx.parent().is_some_and(|p| p.kind() != "source_file") {
            return;
        }
        report_go_globals(cx, n, out);
        return;
    }
    if n.kind() == "call_expression" {
        report_go_env(cx, n, out);
        return;
    }
    if in_function
        && matches!(
            n.kind(),
            "assignment_statement" | "inc_statement" | "dec_statement"
        )
    {
        report_go_write(cx, n, out);
    }
}
