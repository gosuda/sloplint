use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, child_by_field,
    find_named_child, first_named_child, first_named_child_matching, first_of_kind, last_segment,
    named_children, narrow, only_named_child_matching, pos_of, preceding_comments,
    rfind_named_child, slice, string_arg_shape, strip_quotes, text, trimmed,
};
use crate::spec::Role;

/// Role table mapping PHP node kinds to walker roles.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("method_declaration", Role::Function),
    ("anonymous_function", Role::Closure),
    ("arrow_function", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("else_if_clause", Role::ElseIf),
    ("switch_statement", Role::Match),
    ("match_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("foreach_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("function_call_expression", Role::Call),
    ("member_call_expression", Role::Call),
    ("scoped_call_expression", Role::Call),
    ("nullsafe_member_call_expression", Role::Call),
    ("object_creation_expression", Role::NewCall),
    ("assignment_expression", Role::Bind),
    ("augmented_assignment_expression", Role::Bind),
    ("reference_assignment_expression", Role::Bind),
    ("update_expression", Role::Write),
    ("namespace_use_declaration", Role::Import),
    ("include_expression", Role::Import),
    ("include_once_expression", Role::Import),
    ("require_expression", Role::Import),
    ("require_once_expression", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("trait_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("case_statement", Role::MatchArm),
    ("default_statement", Role::MatchArm),
    ("match_conditional_expression", Role::MatchArm),
    ("match_default_expression", Role::MatchArm),
    ("array_creation_expression", Role::StructLit),
    ("subscript_expression", Role::Subscript),
    ("dynamic_variable_name", Role::Global),
    ("global_declaration", Role::Global),
    ("function_static_declaration", Role::StaticItem),
    ("property_declaration", Role::StaticItem),
];

/// PHP language hooks.
pub(crate) static HOOKS: PhpHooks = PhpHooks;

/// PHP adapter.
pub(crate) struct PhpHooks;

const CALL_KINDS: [&str; 4] = [
    "function_call_expression",
    "member_call_expression",
    "scoped_call_expression",
    "nullsafe_member_call_expression",
];

fn node_id(n: Node<'_>) -> usize {
    n.id()
}
fn is_call_kind(kind: &str) -> bool {
    CALL_KINDS.contains(&kind)
}
fn strip_dollar(t: &str) -> &str {
    t.strip_prefix('$').unwrap_or(t)
}
fn field_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> Option<&'s str> {
    child_by_field(n, field).map(|v| text(cx, v))
}
fn is_private_vis<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    n.kind() == "visibility_modifier" && (text(cx, n) == "private" || text(cx, n) == "protected")
}
fn php_public<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kind: &str) -> bool {
    if kind != "method_declaration" {
        return true;
    }
    !any_named_child(n, |c| is_private_vis(cx, c))
}
fn php_in_test<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    if cx.is_test_file {
        return true;
    }
    let mut current = n.parent();
    while let Some(parent) = current {
        if parent.kind() == "class_declaration" {
            return any_named_child(parent, |c| {
                c.kind() == "base_clause"
                    && any_named_child(c, |base| {
                        last_segment(text(cx, base), &["\\"]) == "TestCase"
                    })
            });
        }
        current = parent.parent();
    }
    false
}
fn php_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, missing: &mut Vec<&'s str>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let Some(list) = child_by_field(n, "parameters") else {
        return out;
    };
    for p in named_children(list) {
        let ok = matches!(
            p.kind(),
            "simple_parameter" | "property_promotion_parameter" | "variadic_parameter"
        );
        if !ok {
            continue;
        }
        let name = strip_dollar(field_text(cx, p, "name").unwrap_or(""));
        let ty = field_text(cx, p, "type");
        let bool_default = child_by_field(p, "default_value")
            .map(|d| trimmed(cx, d))
            .is_some_and(|d| d == "true" || d == "false");
        if ty.is_none() {
            missing.push(name);
        }
        out.push(Param {
            name,
            ty,
            bool_default,
        });
    }
    out
}
fn php_sole_call(n: Node<'_>) -> Option<usize> {
    let body = child_by_field(n, "body")?;
    if body.kind() != "compound_statement" {
        return None;
    }
    let stmt = only_named_child_matching(body, |c| c.kind() != "comment")?;
    let inner = if stmt.kind() == "expression_statement" || stmt.kind() == "return_statement" {
        first_named_child(stmt)?
    } else {
        stmt
    };
    (is_call_kind(inner.kind()) || inner.kind() == "object_creation_expression")
        .then(|| node_id(inner))
}
fn php_arg_expr(arg: Node<'_>) -> Node<'_> {
    let named = child_by_field(arg, "name").map(|v| v.id());
    rfind_named_child(arg, |c| {
        Some(c.id()) != named && c.kind() != "reference_modifier"
    })
    .unwrap_or(arg)
}
fn fragment_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>, body: &str, frag: &str) -> ArgShape<'s> {
    match find_named_child(n, |c| c.kind() == body) {
        Some(found) => {
            let parts: Vec<&'s str> = named_children(found)
                .into_iter()
                .filter(|c| c.kind() == frag)
                .map(|c| text(cx, c))
                .collect();
            ArgShape::Str {
                longest_static: longest_static_chunk(&parts),
            }
        }
        None => ArgShape::Other,
    }
}
fn php_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" | "encapsed_string" => string_arg_shape(cx, n, &["string_content"]),
        "heredoc" => fragment_shape(cx, n, "heredoc_body", "string_content"),
        "nowdoc" => fragment_shape(cx, n, "nowdoc_body", "nowdoc_string"),
        "binary_expression" => {
            let dot = child_by_field(n, "operator").is_some_and(|o| text(cx, o) == ".");
            let has_str = any_named_child(n, |c| {
                matches!(
                    c.kind(),
                    "string" | "encapsed_string" | "heredoc" | "nowdoc"
                )
            });
            if dot && has_str {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        "variable_name" => ArgShape::Ident(strip_dollar(text(cx, n))),
        _ => ArgShape::Other,
    }
}
fn arg_nodes(n: Node<'_>) -> Vec<Node<'_>> {
    let Some(list) = child_by_field(n, "arguments").or_else(|| first_of_kind(n, &["arguments"]))
    else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .filter(|c| c.kind() == "argument")
        .collect()
}
fn php_call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arg_nodes(n)
        .into_iter()
        .map(|a| {
            let expr = php_arg_expr(a);
            let shape = if expr.kind() == "variadic_unpacking" {
                ArgShape::Other
            } else {
                php_shape(cx, expr)
            };
            Arg {
                shape,
                start_line: narrow(a.start_position().row) + 1,
                end_line: narrow(a.end_position().row) + 1,
            }
        })
        .collect()
}
fn php_object_root<'s>(cx: &Cx<'_, 's>, obj: Node<'_>) -> Option<&'s str> {
    let mut current = obj;
    loop {
        if current.kind() == "variable_name" {
            return Some(strip_dollar(text(cx, current)));
        }
        {
            let next = child_by_field(current, "object")?;
            current = next
        }
    }
}
fn php_scope_root<'s>(cx: &Cx<'_, 's>, scope: Node<'_>) -> Option<&'s str> {
    let t = text(cx, scope);
    Some(t.split('\\').next().unwrap_or(t))
}
fn php_callee_span<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> (&'s str, CallKind) {
    match (child_by_field(n, field), child_by_field(n, "name")) {
        (Some(o), Some(m)) => (slice(cx.src, o.start_byte(), m.end_byte()), CallKind::Plain),
        _ => ("", CallKind::Plain),
    }
}
fn php_stmt_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, CallKind) {
    match n.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" => {
            php_callee_span(cx, n, "object")
        }
        "scoped_call_expression" => php_callee_span(cx, n, "scope"),
        "object_creation_expression" => (
            first_named_child_matching(n, &["name", "qualified_name", "relative_name"])
                .map(|c| text(cx, c))
                .unwrap_or(""),
            CallKind::New,
        ),
        _ => (field_text(cx, n, "function").unwrap_or(""), CallKind::Plain),
    }
}
fn use_clauses(n: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        if child.kind() == "namespace_use_clause" {
            out.push(child);
        } else if child.kind() == "namespace_use_group" {
            for sub in named_children(child) {
                if sub.kind() == "namespace_use_clause" {
                    out.push(sub);
                }
            }
        }
    }
    out
}
fn php_include_module<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    for child in named_children(n) {
        if child.kind() == "string" {
            return strip_quotes(text(cx, child));
        }
    }
    ""
}
fn push_first_var<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    kind: AmbientKind,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if let Some(name) = first_var_name(cx, n) {
        out.push((kind, name, pos_of(n)));
    }
}
fn each_var_name<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "variable_name" {
            out.push((strip_dollar(text(cx, current)), pos_of(current)));
        } else {
            stack.extend(named_children(current));
        }
    }
}
fn first_var_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut found = Vec::new();
    each_var_name(cx, n, &mut found);
    found.into_iter().next().map(|(name, _)| name)
}
fn is_env_superglobal(t: &str) -> bool {
    matches!(
        t,
        "$_ENV" | "$_SERVER" | "$_GET" | "$_POST" | "$_COOKIE" | "$_REQUEST" | "$_FILES"
    )
}
fn php_first_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let list = child_by_field(n, "arguments")?;
    first_named_child_matching(list, &["argument"]).map(|a| text(cx, php_arg_expr(a)))
}
fn php_two_operand(rest: &str) -> bool {
    matches!(
        rest,
        "Same"
            | "NotSame"
            | "Equals"
            | "NotEquals"
            | "Count"
            | "InstanceOf"
            | "MatchesRegularExpression"
            | "StringContainsString"
    ) || rest.starts_with("GreaterThan")
        || rest.starts_with("LessThan")
        || rest.starts_with("Contains")
}
fn php_handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: &str) -> Stmt<'s> {
    let inner = if stmt.kind() == "expression_statement" {
        first_named_child(stmt).unwrap_or(stmt)
    } else {
        stmt
    };
    if is_call_kind(inner.kind()) || inner.kind() == "object_creation_expression" {
        let (callee, kind) = php_stmt_callee(cx, inner);
        return Stmt::Call { callee, kind };
    }
    if inner.kind() == "throw_expression" {
        let thrown = first_named_child(inner);
        let rethrown = thrown
            .is_some_and(|t| t.kind() == "variable_name" && strip_dollar(text(cx, t)) == binding);
        if rethrown {
            return Stmt::Rethrow;
        }
    }
    Stmt::Other
}
fn member_source<'s>(cx: &Cx<'_, 's>, expr: Node<'_>, want: &str) -> Option<&'s str> {
    if expr.kind() != "member_access_expression" {
        return None;
    }
    if expr.named_child_count() != 2 {
        return None;
    }
    let (Some(obj_node), Some(mem_node)) = (expr.named_child(0), expr.named_child(1)) else {
        return None;
    };
    (obj_node.kind() == "variable_name" && text(cx, mem_node) == want)
        .then(|| strip_dollar(text(cx, obj_node)))
}
fn array_field_source<'s>(cx: &Cx<'_, 's>, elem: Node<'_>) -> Option<&'s str> {
    if elem.named_child_count() != 2 {
        return None;
    }
    let (Some(key_node), Some(val_node)) = (elem.named_child(0), elem.named_child(1)) else {
        return None;
    };
    if key_node.kind() != "string" {
        return None;
    }
    member_source(cx, val_node, strip_quotes(text(cx, key_node)))
}
fn array_copy<'s>(cx: &Cx<'_, 's>, n: Node<'_>, elems: &[Node<'_>]) -> Option<FieldCopy<'s>> {
    if elems.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    for elem in elems {
        let found = array_field_source(cx, *elem)?;
        if source.is_some_and(|s| s != found) {
            return None;
        }
        source = Some(found);
    }
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(elems.len()),
        source,
    })
}
fn push_clause_refs<'s>(cx: &Cx<'_, 's>, clause: Node<'_>, out: &mut Vec<ImplRef<'s>>) {
    for item in named_children(clause) {
        out.push(ImplRef {
            name: last_segment(text(cx, item), &["\\"]),
            pos: pos_of(item),
        });
    }
}
impl LangHooks for PhpHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let kind = n.kind();
        if kind != "function_definition" && kind != "method_declaration" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let mut missing = Vec::new();
        let params = php_params(cx, n, &mut missing);
        if child_by_field(n, "return_type").is_none() && name != "__construct" {
            missing.push("return");
        }
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &["attribute_list"]);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public: php_public(cx, n, kind),
            interface_bound: matches!(cx.innermost_container(), Some(Container::Trait { .. })),
            in_test: php_in_test(cx, n),
            params,
            return_type: field_text(cx, n, "return_type"),
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: php_sole_call(n),
        })
    }
    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        n.kind() == "if_statement" && parent.is_some_and(|p| p.kind() == "else_clause")
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if !matches!(
            n.kind(),
            "assignment_expression"
                | "augmented_assignment_expression"
                | "reference_assignment_expression"
        ) {
            return;
        }
        let Some(left) = child_by_field(n, "left") else {
            return;
        };
        if left.kind() == "variable_name" {
            out.push((strip_dollar(text(cx, left)), pos_of(left)));
        } else if left.kind() == "list_literal" {
            each_var_name(cx, left, out);
        }
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "update_expression" {
            return;
        }
        let Some(arg) = child_by_field(n, "argument") else {
            return;
        };
        let mut current = arg;
        loop {
            if current.kind() == "variable_name" {
                out.push((strip_dollar(text(cx, current)), pos_of(current)));
                return;
            }
            if current.kind() == "subscript_expression" {
                match first_named_child(current) {
                    Some(next) => current = next,
                    None => return,
                }
            } else {
                match child_by_field(current, "object") {
                    Some(next) => current = next,
                    None => return,
                }
            }
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        match n.kind() {
            "function_call_expression" => {
                let fun = child_by_field(n, "function")?;
                Some(CallHead {
                    callee: text(cx, fun),
                    kind: CallKind::Plain,
                    callee_root: None,
                    args: php_call_args(cx, n),
                })
            }
            "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression" => {
                let scoped = n.kind() == "scoped_call_expression";
                let field = if scoped { "scope" } else { "object" };
                let base = child_by_field(n, field)?;
                let (callee, kind) = php_stmt_callee(cx, n);
                if callee.is_empty() {
                    return None;
                }
                let root = if scoped {
                    php_scope_root(cx, base)
                } else {
                    php_object_root(cx, base)
                };
                Some(CallHead {
                    callee,
                    kind,
                    callee_root: root,
                    args: php_call_args(cx, n),
                })
            }
            "object_creation_expression" => {
                let (callee, kind) = php_stmt_callee(cx, n);
                if callee.is_empty() {
                    return None;
                }
                Some(CallHead {
                    callee,
                    kind,
                    callee_root: None,
                    args: php_call_args(cx, n),
                })
            }
            _ => None,
        }
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "namespace_use_declaration" {
            let prefix = first_named_child_matching(n, &["namespace_name"])
                .map(|c| text(cx, c))
                .unwrap_or("");
            for clause in use_clauses(n) {
                let alias_node = child_by_field(clause, "alias");
                let path = find_named_child(clause, |c| Some(c.id()) != alias_node.map(|a| a.id()))
                    .map(|c| text(cx, c))
                    .unwrap_or("");
                if path.is_empty() {
                    continue;
                }
                let cut = path.rfind('\\').map(|at| at + 1).unwrap_or(0);
                let (prior, name) = path.split_at(cut);
                let prior = prior.strip_suffix('\\').unwrap_or(prior);
                let module: Cow<'s, str> = if prefix.is_empty() {
                    Cow::Borrowed(prior)
                } else if prior.is_empty() {
                    Cow::Borrowed(prefix)
                } else {
                    Cow::Owned(format!("{prefix}\\{prior}"))
                };
                out.push(Import {
                    pos: pos_of(clause),
                    module,
                    name,
                    alias: alias_node.map(|a| text(cx, a)),
                    kind: ImportKind::Named,
                    in_test: false,
                });
            }
            return;
        }
        if matches!(
            n.kind(),
            "include_expression"
                | "include_once_expression"
                | "require_expression"
                | "require_once_expression"
        ) {
            out.push(Import {
                pos: pos_of(n),
                module: Cow::Borrowed(php_include_module(cx, n)),
                name: "",
                alias: None,
                kind: ImportKind::SideEffect,
                in_test: false,
            });
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        if !matches!(
            kind,
            "class_declaration"
                | "interface_declaration"
                | "trait_declaration"
                | "enum_declaration"
        ) {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let mut impls = Vec::new();
        let mut implements = false;
        for child in named_children(n) {
            if child.kind() == "base_clause" {
                push_clause_refs(cx, child, &mut impls);
            } else if child.kind() == "class_interface_clause" {
                implements = true;
                push_clause_refs(cx, child, &mut impls);
            }
        }
        let container = if kind == "class_declaration" || kind == "enum_declaration" {
            Some(Container::Class {
                exported: true,
                implements,
            })
        } else {
            Some(Container::Trait { public: true })
        };
        Some(TypeHead {
            def: Some(TypeDef {
                name: text(cx, name_node),
                pos: pos_of(name_node),
                public: true,
                abstraction: kind == "interface_declaration" || kind == "trait_declaration",
            }),
            impls,
            container,
        })
    }
    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let kind = n.kind();
        if kind != "default_statement" && kind != "match_default_expression" {
            return None;
        }
        let mut enum_like = false;
        if let Some(parent) = n.parent() {
            enum_like = any_named_child(parent, |sib| {
                (sib.kind() == "case_statement" || sib.kind() == "match_conditional_expression")
                    && any_named_child(sib, |c| c.kind() == "class_constant_access_expression")
            });
        }
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like,
        })
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let binding = field_text(cx, n, "name").map(strip_dollar).unwrap_or("");
        let body = child_by_field(n, "body")?;
        let mut stmts = Vec::new();
        for stmt in named_children(body) {
            if stmt.kind() == "comment" {
                continue;
            }
            stmts.push(php_handler_stmt(cx, stmt, binding));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() == "array_creation_expression" {
            let elems: Vec<Node<'_>> = named_children(n)
                .into_iter()
                .filter(|c| c.kind() == "array_element_initializer")
                .collect();
            return array_copy(cx, n, &elems);
        }
        if n.kind() != "object_creation_expression" {
            return None;
        }
        let args = arg_nodes(n);
        if args.is_empty() {
            return None;
        }
        let fields = narrow(args.len());
        let mut source: Option<&'s str> = None;
        for arg in args {
            let want = field_text(cx, arg, "name")?;
            let found = member_source(cx, php_arg_expr(arg), want)?;
            if source.is_some_and(|s| s != found) {
                return None;
            }
            source = Some(found);
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields,
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
        let _ = in_function;
        match n.kind() {
            "subscript_expression" => {
                let base = first_named_child(n).map(|b| text(cx, b)).unwrap_or("");
                if is_env_superglobal(base) {
                    out.push((AmbientKind::EnvRead, base, pos_of(n)));
                } else if base == "$_SESSION" || base == "$GLOBALS" {
                    out.push((AmbientKind::GlobalMutable, base, pos_of(n)));
                }
            }
            "global_declaration" => push_first_var(cx, n, AmbientKind::GlobalWrite, out),
            "function_static_declaration" => push_first_var(cx, n, AmbientKind::GlobalMutable, out),
            "property_declaration" => {
                if any_named_child(n, |c| c.kind() == "static_modifier") {
                    push_first_var(cx, n, AmbientKind::GlobalMutable, out);
                }
            }
            kind if is_call_kind(kind) => {
                let (callee, _) = php_stmt_callee(cx, n);
                let last = last_segment(callee, &["::", "->", ".", "\\"]);
                if last == "getenv" || last == "apache_getenv" {
                    out.push((
                        AmbientKind::EnvRead,
                        php_first_arg(cx, n).unwrap_or(callee),
                        pos_of(n),
                    ));
                } else if last == "putenv" {
                    out.push((
                        AmbientKind::GlobalMutable,
                        php_first_arg(cx, n).unwrap_or(callee),
                        pos_of(n),
                    ));
                }
            }
            _ => {}
        }
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() == "dynamic_variable_name" {
            return Some("variable variable");
        }
        if n.kind() == "function_call_expression" {
            let fun = child_by_field(n, "function")?;
            if matches!(
                fun.kind(),
                "variable_name"
                    | "dynamic_variable_name"
                    | "string"
                    | "encapsed_string"
                    | "parenthesized_expression"
            ) {
                return Some("variable function");
            }
            let callee = text(cx, fun);
            if callee == "eval" {
                return Some("eval");
            }
            if callee.starts_with("call_user_func") || callee.starts_with("forward_static_call") {
                return Some("call_user_func");
            }
            return None;
        }
        if matches!(
            n.kind(),
            "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression"
        ) {
            let name = child_by_field(n, "name")?;
            if name.kind() != "name" {
                return Some("variable method name");
            }
        }
        None
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if !is_call_kind(n.kind()) {
            return None;
        }
        let (callee, _) = php_stmt_callee(cx, n);
        let name = last_segment(callee, &["::", "->", ".", "\\"]);
        if !name.starts_with("assert") {
            return None;
        }
        let two = php_two_operand(name.strip_prefix("assert").unwrap_or(name));
        let args = if child_by_field(n, "arguments").is_none() {
            return Some(Assertion {
                pos: pos_of(n),
                has_message: false,
                comparison: two,
            });
        } else {
            arg_nodes(n)
        };
        let last_str = args.last().is_some_and(|a| {
            let expr = php_arg_expr(*a);
            expr.kind() == "string" || expr.kind() == "encapsed_string"
        });
        Some(Assertion {
            pos: pos_of(n),
            has_message: args.len() > (if two { 2 } else { 1 }) && last_str,
            comparison: two,
        })
    }
}
