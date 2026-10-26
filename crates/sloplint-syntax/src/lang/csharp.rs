use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    child_by_field, find_map_named_child, find_named_child, first_named_child,
    first_named_child_matching, has_child_kind, named_children, narrow, only_named_child_matching,
    pos_of, preceding_comments, rfind_named_child, string_arg_shape, text, trimmed,
};
use crate::spec::Role;
use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use std::borrow::Cow;
use tree_sitter::Node;
/// Node-kind roles for the C# grammar.
pub(crate) static TABLE: crate::lang::Table = &[
    ("method_declaration", Role::Function),
    ("constructor_declaration", Role::Function),
    ("local_function_statement", Role::Function),
    ("lambda_expression", Role::Closure),
    ("anonymous_method_expression", Role::Closure),
    ("if_statement", Role::If),
    ("switch_statement", Role::Match),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("foreach_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("invocation_expression", Role::Call),
    ("object_creation_expression", Role::NewCall),
    ("local_declaration_statement", Role::Decl),
    ("assignment_expression", Role::Write),
    ("prefix_unary_expression", Role::Write),
    ("postfix_unary_expression", Role::Write),
    ("using_directive", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("struct_declaration", Role::TypeDef),
    ("record_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("switch_section", Role::MatchArm),
    ("switch_expression_arm", Role::MatchArm),
    ("initializer_expression", Role::StructLit),
    ("field_declaration", Role::StaticItem),
];
/// Hooks reading C# declarations, calls, imports, and handlers.
pub(crate) static HOOKS: CSharpHooks = CSharpHooks;
/// Unit hooks for C#.
pub(crate) struct CSharpHooks;
const TEST_ATTRS: &[&str] = &[
    "Test",
    "TestMethod",
    "TestCase",
    "TestCaseSource",
    "DataTestMethod",
    "Fact",
    "Theory",
    "SetUp",
    "TearDown",
];
const EQUALITY_ASSERTS: &[&str] = &[
    "Equal",
    "NotEqual",
    "AreEqual",
    "AreNotEqual",
    "Same",
    "NotSame",
    "Contains",
];
const STRING_KINDS: &[&str] = &[
    "string_literal",
    "verbatim_string_literal",
    "raw_string_literal",
    "interpolated_string_expression",
];
fn has_mod(cx: &Cx<'_, '_>, n: Node<'_>, word: &str) -> bool {
    any_named_child(n, |c| c.kind() == "modifier" && trimmed(cx, c) == word)
}
fn attr_hit(cx: &Cx<'_, '_>, n: Node<'_>, wants: &[&str]) -> bool {
    any_named_child(n, |c| {
        c.kind() == "attribute_list"
            && any_named_child(c, |a| {
                a.kind() == "attribute"
                    && child_by_field(a, "name")
                        .is_some_and(|nm| wants.contains(&seg_last(trimmed(cx, nm))))
            })
    })
}
fn cut_generics(t: &str) -> &str {
    match t.find('<') {
        Some(i) => &t[..i],
        None => t,
    }
}
fn seg_last(t: &str) -> &str {
    let base = cut_generics(t);
    let bytes = base.as_bytes();
    let mut end = base.len();
    while end > 0 && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    &base[start..end]
}
fn unwrap_await(mut n: Node<'_>) -> Node<'_> {
    while n.kind() == "await_expression" {
        let Some(next) = first_named_child(n) else {
            break;
        };
        n = next;
    }
    n
}
fn callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if func.kind() == "generic_name"
        && let Some(i) = first_named_child_matching(func, &["identifier"])
    {
        return text(cx, i);
    }
    cut_generics(text(cx, func))
}
fn last_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if func.kind() == "member_access_expression"
        && let Some(m) = child_by_field(func, "name")
    {
        return text(cx, m);
    }
    if func.kind() == "generic_name"
        && let Some(i) = first_named_child_matching(func, &["identifier"])
    {
        return text(cx, i);
    }
    seg_last(trimmed(cx, func))
}
fn callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "base" => return Some(text(cx, current)),
            "generic_name" => {
                return first_named_child_matching(current, &["identifier"]).map(|i| text(cx, i));
            }
            "member_access_expression" => child_by_field(current, "expression")?,
            _ => return None,
        };
    }
}
fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some((text(cx, current), pos_of(current))),
            "member_access_expression" | "element_access_expression" => {
                child_by_field(current, "expression")?
            }
            "parenthesized_expression" => first_named_child(current)?,
            _ => return None,
        };
    }
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> ArgShape<'s> {
    match e.kind() {
        "string_literal" => string_arg_shape(cx, e, &["string_literal_content"]),
        "verbatim_string_literal" => {
            let t = text(cx, e);
            let seg = t.get(2..t.len().saturating_sub(1)).unwrap_or(t);
            ArgShape::Str {
                longest_static: longest_static_chunk(&[seg]),
            }
        }
        "raw_string_literal" => string_arg_shape(cx, e, &["raw_string_content"]),
        "interpolated_string_expression" => string_arg_shape(cx, e, &["string_content"]),
        "binary_expression" => {
            if binary_concat(cx, e, "+", STRING_KINDS) {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        "identifier" => ArgShape::Ident(text(cx, e)),
        _ => ArgShape::Other,
    }
}
fn arg_expr(n: Node<'_>) -> Option<Node<'_>> {
    let named = child_by_field(n, "name").map(|m| m.id());
    find_named_child(n, |c| Some(c.id()) != named)
}
fn args_from_list<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Arg<'s>> {
    named_children(list)
        .into_iter()
        .filter(|a| a.kind() == "argument")
        .map(|a| Arg {
            shape: arg_expr(a)
                .map(|e| arg_shape(cx, e))
                .unwrap_or(ArgShape::Other),
            start_line: narrow(a.start_position().row) + 1,
            end_line: narrow(a.end_position().row) + 1,
        })
        .collect()
}
fn arg_len(n: Node<'_>) -> u32 {
    let mut count = 0u32;
    if let Some(list) = child_by_field(n, "arguments") {
        for a in named_children(list) {
            if a.kind() == "argument" {
                count += 1;
            }
        }
    }
    count
}
fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|l| first_named_child_matching(l, &["argument"]))
}
fn params_from_list<'s>(cx: &Cx<'_, 's>, pl: Node<'_>, out: &mut Vec<Param<'s>>) {
    if let Some(nm) = child_by_field(pl, "name") {
        out.push(Param {
            name: text(cx, nm),
            ty: child_by_field(pl, "type").map(|t| text(cx, t)),
            bool_default: false,
        });
    }
    for c in named_children(pl) {
        if c.kind() != "parameter" {
            continue;
        }
        let Some(nm) = child_by_field(c, "name") else {
            continue;
        };
        let dq = |d: Node<'_>| {
            d.kind() == "boolean_literal" && (trimmed(cx, d) == "true" || trimmed(cx, d) == "false")
        };
        let nested = any_named_child(c, |d| d.kind() == "expression" && any_named_child(d, &dq));
        out.push(Param {
            name: text(cx, nm),
            ty: child_by_field(c, "type").map(|t| text(cx, t)),
            bool_default: any_named_child(c, &dq) || nested,
        });
    }
}
fn sole_call_of(body: Node<'_>) -> Option<usize> {
    if body.kind() == "arrow_expression_clause" {
        let call = unwrap_await(first_named_child(body)?);
        return (call.kind() == "invocation_expression").then_some(call.id());
    }
    let stmt = only_named_child_matching(body, |c| c.kind() != "comment")?;
    if stmt.kind() != "return_statement" && stmt.kind() != "expression_statement" {
        return None;
    }
    let call = unwrap_await(first_named_child(stmt)?);
    (call.kind() == "invocation_expression").then_some(call.id())
}
fn tuple_names<'s>(cx: &Cx<'_, 's>, t: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending: Vec<Node<'_>> = named_children(t).into_iter().rev().collect();
    while let Some(c) = pending.pop() {
        if c.kind() == "tuple_pattern" {
            pending.extend(named_children(c).into_iter().rev());
        } else if c.kind() == "identifier" {
            out.push((text(cx, c), pos_of(c)));
        }
    }
}
fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "throw_statement" {
        return match (first_named_child(s), binding) {
            (None, _) => Stmt::Rethrow,
            (Some(e), Some(b)) if e.kind() == "identifier" && text(cx, e) == b => Stmt::Rethrow,
            _ => Stmt::Other,
        };
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    find_map_named_child(s, |c| {
        if c.kind() != "invocation_expression" {
            return None;
        }
        child_by_field(c, "function")
    })
    .map(|f| Stmt::Call {
        callee: callee_text(cx, f),
        kind: CallKind::Plain,
    })
    .unwrap_or(Stmt::Other)
}
fn sib_member(parent: Node<'_>, self_id: usize) -> bool {
    any_named_child(parent, |sib| {
        if sib.id() == self_id {
            return false;
        }
        let mut stack = vec![sib];
        while let Some(cur) = stack.pop() {
            if cur.id() != sib.id() && cur.kind() == "member_access_expression" {
                return true;
            }
            if cur.id() == sib.id()
                || matches!(
                    cur.kind(),
                    "expression" | "pattern" | "constant_pattern" | "when_clause"
                )
            {
                stack.extend(named_children(cur));
            }
        }
        false
    })
}
impl LangHooks for CSharpHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let kind = n.kind();
        if kind != "method_declaration"
            && kind != "constructor_declaration"
            && kind != "local_function_statement"
        {
            return None;
        }
        let body = child_by_field(n, "body")?;
        if body.kind() != "block" && body.kind() != "arrow_expression_clause" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let inner = cx.innermost_container();
        let mut public = kind != "local_function_statement"
            && (has_mod(cx, n, "public") || inner == Some(Container::Trait { public: true }));
        if let Some(Container::Class { exported, .. }) = inner {
            public = public && exported;
        }
        let overrides = any_named_child(n, |c| c.kind() == "explicit_interface_specifier")
            || has_mod(cx, n, "override");
        let interface_bound = matches!(
            inner,
            Some(Container::Class {
                implements: true,
                ..
            })
        ) && overrides;
        let in_test = cx.is_test_file || cx.in_test_module() || attr_hit(cx, n, TEST_ATTRS);
        let mut params = Vec::new();
        if let Some(pl) = child_by_field(n, "parameters") {
            params_from_list(cx, pl, &mut params);
        }
        let return_type = if kind == "constructor_declaration" {
            None
        } else if kind == "local_function_statement" {
            child_by_field(n, "type").map(|t| text(cx, t))
        } else {
            child_by_field(n, "returns").map(|t| text(cx, t))
        };
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public,
            interface_bound,
            in_test,
            params,
            return_type,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call_of(body),
        })
    }
    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        if n.kind() != "if_statement" {
            return false;
        }
        let Some(p) = parent else { return false };
        if p.kind() != "if_statement" {
            return false;
        }
        p.child_by_field_name("alternative")
            .is_some_and(|a| a.id() == n.id())
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "local_declaration_statement" || has_mod(cx, n, "const") {
            return;
        }
        for decl in named_children(n) {
            if decl.kind() != "variable_declaration" {
                continue;
            }
            for d in named_children(decl) {
                if d.kind() != "variable_declarator" {
                    continue;
                }
                let mut tupled = false;
                for t in named_children(d) {
                    if t.kind() == "tuple_pattern" {
                        tupled = true;
                        tuple_names(cx, t, out);
                    }
                }
                if !tupled && let Some(nm) = child_by_field(d, "name") {
                    out.push((text(cx, nm), pos_of(nm)));
                }
            }
        }
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "assignment_expression" {
            if let Some(l) = child_by_field(n, "left") {
                out.extend(write_root(cx, l));
            }
            return;
        }
        if n.kind() != "prefix_unary_expression" && n.kind() != "postfix_unary_expression" {
            return;
        }
        let arith = (0..n.child_count())
            .filter_map(|i| n.child(i))
            .any(|c| c.kind() == "++" || c.kind() == "--");
        if arith && let Some(e) = first_named_child(n) {
            out.extend(write_root(cx, e));
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "invocation_expression" {
            let func = child_by_field(n, "function")?;
            return Some(CallHead {
                callee: callee_text(cx, func),
                kind: CallKind::Plain,
                callee_root: callee_root(cx, func),
                args: child_by_field(n, "arguments")
                    .map(|l| args_from_list(cx, l))
                    .unwrap_or_default(),
            });
        }
        if n.kind() != "object_creation_expression" {
            return None;
        }
        let ty = child_by_field(n, "type")?;
        Some(CallHead {
            callee: cut_generics(text(cx, ty)),
            kind: CallKind::New,
            callee_root: None,
            args: child_by_field(n, "arguments")
                .map(|l| args_from_list(cx, l))
                .unwrap_or_default(),
        })
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() != "using_directive" {
            return;
        }
        let alias_node = child_by_field(n, "name");
        let alias_id = alias_node.map(|a| a.id());
        let Some(t) = rfind_named_child(n, |c| Some(c.id()) != alias_id) else {
            return;
        };
        let full = text(cx, t);
        if has_child_kind(n, "static") {
            out.push(Import {
                pos: pos_of(n),
                module: Cow::Borrowed(full),
                name: seg_last(full),
                alias: None,
                kind: ImportKind::Wildcard,
                in_test: cx.is_test_file,
            });
            return;
        }
        let name = seg_last(full);
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(match full.rfind('.') {
                Some(at) => &full[..at],
                None => "",
            }),
            name,
            alias: match alias_node.map(|a| text(cx, a)) {
                Some(a) if a != name => Some(a),
                _ => None,
            },
            kind: if alias_node.is_some() {
                ImportKind::Named
            } else {
                ImportKind::Namespace
            },
            in_test: cx.is_test_file,
        });
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        if kind != "class_declaration"
            && kind != "struct_declaration"
            && kind != "record_declaration"
            && kind != "interface_declaration"
            && kind != "enum_declaration"
        {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let inner = cx.innermost_container();
        let enclosed = match inner {
            None => true,
            Some(Container::Class { exported, .. }) => exported,
            Some(Container::Trait { public }) => public,
            Some(_) => true,
        };
        let public = has_mod(cx, n, "public") && enclosed;
        let mut impls = Vec::new();
        let mut has_base = false;
        for c in named_children(n) {
            if c.kind() != "base_list" {
                continue;
            }
            for t in named_children(c) {
                if t.kind() == "argument_list" || t.kind() == "primary_constructor_base_type" {
                    continue;
                }
                has_base = true;
                impls.push(ImplRef {
                    name: seg_last(text(cx, t)),
                    pos: pos_of(t),
                });
            }
        }
        let container = if kind == "interface_declaration" {
            Some(Container::Trait { public })
        } else if kind == "enum_declaration" {
            None
        } else if attr_hit(cx, n, &["TestFixture", "TestClass"]) {
            Some(Container::TestModule)
        } else {
            Some(Container::Class {
                exported: public,
                implements: has_base,
            })
        };
        Some(TypeHead {
            def: Some(TypeDef {
                name: text(cx, name_node),
                pos: pos_of(name_node),
                public,
                abstraction: kind == "interface_declaration" || has_mod(cx, n, "abstract"),
            }),
            impls,
            container,
        })
    }
    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let kind = n.kind();
        if kind != "switch_section" && kind != "switch_expression_arm" {
            return None;
        }
        let wildcard = if kind == "switch_section" {
            has_child_kind(n, "default")
        } else {
            any_named_child(n, |c| c.kind() == "discard")
        };
        if !wildcard {
            return None;
        }
        let pos = pos_of(n);
        let Some(parent) = n.parent() else {
            return Some(WildcardArm {
                pos,
                enum_like: false,
            });
        };
        Some(WildcardArm {
            pos,
            enum_like: sib_member(parent, n.id()),
        })
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let body = child_by_field(n, "body")?;
        let binding = first_named_child_matching(n, &["catch_declaration"])
            .and_then(|d| child_by_field(d, "name"))
            .map(|m| text(cx, m));
        let mut stmts = Vec::new();
        for s in named_children(body) {
            if s.kind() != "comment" {
                stmts.push(stmt_of(cx, s, binding));
            }
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "initializer_expression" {
            return None;
        }
        if n.parent()
            .is_none_or(|p| p.kind() != "object_creation_expression")
        {
            return None;
        }
        let mut fields = 0u32;
        let mut source = None;
        let mut uniform = true;
        for c in named_children(n) {
            if c.kind() != "assignment_expression" {
                return None;
            }
            if child_by_field(c, "operator")
                .map(|o| text(cx, o))
                .unwrap_or("")
                != "="
            {
                return None;
            }
            let (Some(l), Some(r)) = (child_by_field(c, "left"), child_by_field(c, "right")) else {
                return None;
            };
            if l.kind() != "identifier" {
                return None;
            }
            let (Some(o), Some(m)) = (child_by_field(r, "expression"), child_by_field(r, "name"))
            else {
                return None;
            };
            if r.kind() != "member_access_expression"
                || o.kind() != "identifier"
                || text(cx, m) != text(cx, l)
            {
                return None;
            }
            fields += 1;
            match source {
                None if uniform => source = Some(text(cx, o)),
                Some(prev) if uniform && prev == text(cx, o) => {}
                _ => {
                    uniform = false;
                    source = None;
                }
            }
        }
        if fields == 0 {
            return None;
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
        _in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "field_declaration" {
            if has_mod(cx, n, "static")
                && !has_mod(cx, n, "readonly")
                && !has_mod(cx, n, "const")
                && let Some(nm) = find_map_named_child(n, |c| {
                    if c.kind() != "variable_declaration" {
                        return None;
                    }
                    find_map_named_child(c, |d| {
                        if d.kind() != "variable_declarator" {
                            return None;
                        }
                        child_by_field(d, "name")
                    })
                })
            {
                out.push((AmbientKind::GlobalMutable, text(cx, nm), pos_of(nm)));
            }
            return;
        }
        if n.kind() != "invocation_expression" {
            return;
        }
        let Some(func) = child_by_field(n, "function") else {
            return;
        };
        if func.kind() != "member_access_expression" {
            return;
        }
        let expr = child_by_field(func, "expression")
            .map(|e| text(cx, e))
            .unwrap_or("");
        let name = child_by_field(func, "name")
            .map(|m| text(cx, m))
            .unwrap_or("");
        if (expr == "Environment" || expr == "System.Environment")
            && (name == "GetEnvironmentVariable" || name == "GetEnvironmentVariables")
        {
            let label = first_arg(n)
                .and_then(arg_expr)
                .map(|e| text(cx, e))
                .unwrap_or_else(|| text(cx, func));
            out.push((AmbientKind::EnvRead, label, pos_of(n)));
        }
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "invocation_expression" {
            return None;
        }
        let last = last_name(cx, child_by_field(n, "function")?);
        if last != "GetMethod"
            && last != "GetProperty"
            && last != "GetField"
            && last != "GetType"
            && last != "InvokeMember"
        {
            return None;
        }
        if first_arg(n)
            .and_then(arg_expr)
            .is_none_or(|e| STRING_KINDS.contains(&e.kind()))
        {
            return None;
        }
        if last == "InvokeMember" {
            Some("reflective member invocation")
        } else {
            Some("reflection lookup by computed name")
        }
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "invocation_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        if func.kind() != "member_access_expression" {
            return None;
        }
        let expr = child_by_field(func, "expression")?;
        let name = child_by_field(func, "name")
            .map(|m| text(cx, m))
            .unwrap_or("");
        let head = last_name(cx, expr);
        let debug = head == "Debug" && name == "Assert";
        if head != "Assert" && !debug {
            return None;
        }
        let index = match name {
            "True" | "False" | "IsTrue" | "IsFalse" | "Null" | "NotNull" | "IsNull"
            | "IsNotNull" => 1,
            "Equal" | "NotEqual" | "AreEqual" | "AreNotEqual" | "Same" | "NotSame" | "Contains"
            | "That" => 2,
            "Assert" if debug => 1,
            _ => return None,
        };
        Some(Assertion {
            pos: pos_of(n),
            has_message: arg_len(n) > index,
            comparison: EQUALITY_ASSERTS.contains(&name),
        })
    }
}
