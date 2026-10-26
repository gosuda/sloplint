use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, Import, ImportKind, Param,
    Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    child_by_field, find_named_child, first_named_child, first_named_child_matching, first_of_kind,
    has_child_kind, last_named_child, named_children, narrow, only_named_child_matching, pos_of,
    preceding_comments, string_arg_shape, text, trimmed,
};
use crate::spec::Role;

/// Role table mapping Zig node kinds to walker roles.
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
    ("variable_declaration", Role::Decl),
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

/// Zig language hooks.
pub(crate) static HOOKS: ZigHooks = ZigHooks;

/// Zig adapter.
pub(crate) struct ZigHooks;

fn is_pub_token(c: Node<'_>) -> bool {
    !c.is_named() && c.kind() == "pub"
}

fn has_pub(n: Node<'_>) -> bool {
    (0..n.child_count()).any(|i| n.child(i).is_some_and(is_pub_token))
}

fn in_test(cx: &Cx<'_, '_>) -> bool {
    cx.is_test_file || cx.in_test_module()
}

fn sole_call_id(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    if body.kind() != "block" {
        return None;
    }
    let mut node = only_named_child_matching(body, |c| c.kind() != "comment")?;
    if node.kind() == "expression_statement" {
        node = find_named_child(node, |c| c.kind() != "comment")?;
    }
    if node.kind() == "return_expression" || node.kind() == "try_expression" {
        node = first_named_child(node)?;
    }
    (node.kind() == "call_expression").then_some(node.id())
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
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
            current = first_named_child(current)?;
            continue;
        }
        if kind == "parenthesized_expression" {
            current = child_by_field(current, "expression")?;
            continue;
        }
        return None;
    }
}

fn positional_nodes<'a>(n: Node<'a>, func: Node<'a>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        if child.id() == func.id() {
            continue;
        }
        if child.kind() == "arguments" {
            out.extend(named_children(child));
        } else {
            out.push(child);
        }
    }
    out
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "multiline_string" => {
            let raw = text(cx, n);
            let clean = |line: &'s str| line.trim_start().strip_prefix("\\\\").unwrap_or(line);
            let parts: Vec<&'s str> = raw.lines().map(clean).collect();
            ArgShape::Str {
                longest_static: longest_static_chunk(&parts),
            }
        }
        "identifier" => ArgShape::Ident(text(cx, n)),
        "binary_expression" if binary_concat(cx, n, "++", &["string", "multiline_string"]) => {
            ArgShape::Concat
        }
        _ => ArgShape::Other,
    }
}

fn builtin_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let found = first_of_kind(n, &["builtin_identifier"]);
    let name = found.map(|id| trimmed(cx, id));
    name.map(|raw| raw.strip_prefix('@').unwrap_or(raw))
        .unwrap_or("")
}

fn is_import_builtin(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    first_of_kind(n, &["builtin_identifier"]).is_some_and(|id| text(cx, id) == "@import")
}

fn stmt_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, bound: &str) -> Stmt<'s> {
    let mut current = n;
    if current.kind() == "statement" {
        match first_named_child(current) {
            Some(next) => current = next,
            None => return Stmt::Other,
        }
    }
    if current.kind() == "expression_statement" {
        match first_named_child(current) {
            Some(next) => current = next,
            None => return Stmt::Other,
        }
    }
    if current.kind() == "call_expression" {
        let func = child_by_field(current, "function");
        let callee = func.map(|f| trimmed(cx, f)).unwrap_or("");
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    if current.kind() == "builtin_function" {
        let callee = builtin_name(cx, current);
        return Stmt::Call {
            callee,
            kind: CallKind::Macro,
        };
    }
    if current.kind() == "return_expression" {
        let first = first_named_child(current);
        if first.is_some_and(|a| a.kind() == "identifier" && text(cx, a) == bound) {
            return Stmt::Rethrow;
        }
    }
    Stmt::Other
}

fn import_module<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let found = first_named_child_matching(n, &["string_content"]);
    found.map(|c| text(cx, c)).unwrap_or("")
}

fn import_names<'s>(
    cx: &Cx<'_, 's>,
    builtin: Node<'_>,
) -> Option<(&'s str, Option<&'s str>, ImportKind)> {
    let holder = builtin.parent()?;
    let value = child_by_field(holder, "expression");
    if holder.kind() == "variable_declaration" && value.is_some_and(|e| e.id() == builtin.id()) {
        let id = first_named_child_matching(holder, &["identifier"])?;
        return Some((text(cx, id), None, ImportKind::Namespace));
    }
    let object = child_by_field(holder, "object");
    if holder.kind() != "field_expression" || !object.is_some_and(|o| o.id() == builtin.id()) {
        return None;
    }
    let member = child_by_field(holder, "member")
        .map(|m| text(cx, m))
        .unwrap_or("");
    let decl = holder.parent()?;
    if decl.kind() != "variable_declaration" {
        return None;
    }
    let binding = first_named_child_matching(decl, &["identifier"]).map(|c| text(cx, c));
    let alias = match binding {
        Some(found) if found != member => Some(found),
        _ => None,
    };
    Some((member, alias, ImportKind::Named))
}

fn push_import_string<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(args) = n.parent() else { return };
    if args.kind() != "arguments" {
        return;
    }
    let Some(builtin) = args.parent() else { return };
    if builtin.kind() != "builtin_function" || !is_import_builtin(cx, builtin) {
        return;
    }
    let parent = builtin.parent();
    if parent.is_some_and(|p| p.kind() == "using_namespace_declaration") {
        return;
    }
    let module = Cow::Borrowed(import_module(cx, n));
    let shape = import_names(cx, builtin);
    let (name, alias, kind) = shape.unwrap_or(("", None, ImportKind::SideEffect));
    let pos = pos_of(n);
    let flagged = cx.in_test_module();
    out.push(Import {
        pos,
        module,
        name,
        alias,
        kind,
        in_test: flagged,
    });
}

fn push_using<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(expr) = child_by_field(n, "expression") else {
        return;
    };
    if expr.kind() != "builtin_function" || !is_import_builtin(cx, expr) {
        return;
    }
    let Some(args) = child_by_field(expr, "arguments") else {
        return;
    };
    let found = first_named_child_matching(args, &["string"]);
    let Some(literal) = found else { return };
    if import_module(cx, literal).is_empty() {
        return;
    }
    let module = Cow::Borrowed(import_module(cx, literal));
    let pos = pos_of(n);
    let flagged = cx.in_test_module();
    let wild = ImportKind::Wildcard;
    out.push(Import {
        pos,
        module,
        name: "",
        alias: None,
        kind: wild,
        in_test: flagged,
    });
}

fn case_has_enum(cx: &Cx<'_, '_>, s: Node<'_>) -> bool {
    for i in 0..s.child_count() {
        let Some(child) = s.child(i) else { continue };
        if child.is_named() {
            if child.kind() == "error_type" || child.kind() == "field_expression" {
                return true;
            }
        } else if text(cx, child) == "=>" {
            break;
        }
    }
    false
}

fn sibling_enum(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    any_named_child(parent, |s| {
        if s.kind() != "switch_case" || s.id() == n.id() {
            return false;
        }
        case_has_enum(cx, s)
    })
}

fn field_value_source<'s>(cx: &Cx<'_, 's>, name: Node<'_>, value: Node<'_>) -> Option<&'s str> {
    if value.kind() != "field_expression" {
        return None;
    }
    let obj = child_by_field(value, "object")?;
    if obj.kind() != "identifier" {
        return None;
    }
    let member = child_by_field(value, "member")
        .map(|m| text(cx, m))
        .unwrap_or("");
    (member == text(cx, name)).then_some(text(cx, obj))
}

fn is_eq_compare(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "binary_expression" {
        return false;
    }
    let equal = |c: Node<'_>| text(cx, c) == "==" || text(cx, c) == "!=";
    (0..n.child_count()).any(|i| n.child(i).is_some_and(equal))
}

fn zig_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>) -> Option<Param<'s>> {
    match child_by_field(child, "name") {
        Some(decl) => {
            let ty = child_by_field(child, "type").map(|t| text(cx, t));
            Some(Param {
                name: text(cx, decl),
                ty,
                bool_default: false,
            })
        }
        None if trimmed(cx, child) == "..." => Some(Param {
            name: "...",
            ty: None,
            bool_default: false,
        }),
        None => None,
    }
}

fn is_method_receiver(cx: &Cx<'_, '_>, params: &[Param<'_>]) -> bool {
    let last = cx.innermost_container();
    let method = matches!(last, Some(Container::Class { .. }));
    method && params.first().is_some_and(|p| p.name == "self")
}

fn callee_root(func: Node<'_>) -> Option<Node<'_>> {
    let mut node = func;
    loop {
        if node.kind() == "identifier" {
            return Some(node);
        }
        {
            let next = child_by_field(node, "object")?;
            node = next
        }
    }
}

fn catch_bound<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let payload = first_of_kind(n, &["payload"]);
    payload
        .and_then(|p| first_named_child_matching(p, &["identifier"]))
        .map(|id| text(cx, id))
        .unwrap_or("")
}

impl LangHooks for ZigHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        if n.kind() != "function_declaration" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let body = child_by_field(n, "body")?;
        let mut params = Vec::new();
        if let Some(holder) = first_of_kind(n, &["parameters"]) {
            for child in named_children(holder) {
                if child.kind() != "parameter" {
                    continue;
                }
                if let Some(param) = zig_param(cx, child) {
                    params.push(param);
                }
            }
        }
        if is_method_receiver(cx, &params) {
            params.remove(0);
        }
        let exported = cx.nearest_class_exported().unwrap_or(true);
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        let return_type = child_by_field(n, "type").map(|t| text(cx, t));
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public: has_pub(n) && exported,
            interface_bound: false,
            in_test: in_test(cx),
            params,
            return_type,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call_id(cx, body),
        })
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

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "variable_declaration" || !has_child_kind(n, "var") {
            return;
        }
        if let Some(id) = first_named_child_matching(n, &["identifier"]) {
            out.push((text(cx, id), pos_of(id)));
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment_expression" {
            return;
        }
        if let Some(left) = child_by_field(n, "left")
            && let Some(root) = write_root(cx, left)
        {
            out.push(root);
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "call_expression" {
            let func = child_by_field(n, "function")?;
            let root = callee_root(func);
            let args = positional_nodes(n, func)
                .into_iter()
                .map(|a| make_arg(cx, a))
                .collect();
            return Some(CallHead {
                callee: trimmed(cx, func),
                kind: CallKind::Plain,
                callee_root: root.map(|r| text(cx, r)),
                args,
            });
        }
        if n.kind() != "builtin_function" {
            return None;
        }
        let args_node = child_by_field(n, "arguments")?;
        let args = named_children(args_node)
            .into_iter()
            .map(|a| make_arg(cx, a))
            .collect();
        Some(CallHead {
            callee: builtin_name(cx, n),
            kind: CallKind::Macro,
            callee_root: None,
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "using_namespace_declaration" {
            push_using(cx, n, out);
        } else if n.kind() == "string" {
            push_import_string(cx, n, out);
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        if !is_type_kind(n.kind()) {
            return None;
        }
        let holder = n.parent()?;
        if holder.kind() != "variable_declaration" {
            return None;
        }
        let name_node = first_named_child_matching(holder, &["identifier"])?;
        let public = has_pub(holder);
        let abstraction = n.kind() == "opaque_declaration";
        let name = text(cx, name_node);
        let class = Container::Class {
            exported: public,
            implements: false,
        };
        Some(TypeHead {
            def: Some(TypeDef {
                name,
                pos: pos_of(name_node),
                public,
                abstraction,
            }),
            impls: Vec::new(),
            container: Some(class),
        })
    }

    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        (n.kind() == "test_declaration").then_some(Container::TestModule)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "switch_case" || first_of_kind(n, &["else"]).is_none() {
            return None;
        }
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like: sibling_enum(cx, n),
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_expression" {
            return None;
        }
        let body = last_named_child(n)?;
        let bound = catch_bound(cx, n);
        let stmts = if body.kind() == "block" {
            named_children(body)
                .into_iter()
                .filter(|c| c.kind() != "comment")
                .map(|s| stmt_for(cx, s, bound))
                .collect()
        } else {
            vec![stmt_for(cx, body, bound)]
        };
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "struct_initializer" && n.kind() != "anonymous_struct_initializer" {
            return None;
        }
        let list = child_by_field(n, "initializer_list")?;
        let fields: Vec<Node<'_>> = named_children(list)
            .into_iter()
            .filter(|c| c.kind() == "field_initializer")
            .collect();
        if fields.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        let mut complete = true;
        for field in &fields {
            let found = match (
                field.named_child(0),
                field.named_child(1),
                field.named_child(2),
            ) {
                (Some(name), Some(value), None) if name.kind() == "identifier" => {
                    field_value_source(cx, name, value)
                }
                _ => None,
            };
            match found {
                Some(current) if source.is_none_or(|prev| prev == current) => {
                    source = Some(current);
                }
                _ => {
                    complete = false;
                    break;
                }
            }
        }
        let source = if complete { source } else { None };
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(fields.len()),
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
        if n.kind() == "variable_declaration" {
            if in_function || !has_child_kind(n, "var") {
                return;
            }
            if let Some(id) = first_named_child_matching(n, &["identifier"]) {
                let global = (AmbientKind::GlobalMutable, text(cx, id), pos_of(id));
                out.push(global);
            }
            return;
        }
        if n.kind() != "call_expression" {
            return;
        }
        let Some(func) = child_by_field(n, "function") else {
            return;
        };
        let callee = trimmed(cx, func);
        if is_env_callee(callee) {
            let read = (AmbientKind::EnvRead, callee, pos_of(n));
            out.push(read);
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "builtin_function" {
            return None;
        }
        if !is_field_call(cx, n) {
            return None;
        }
        let args = child_by_field(n, "arguments")?;
        let second = args.named_child(1)?;
        (second.kind() != "string").then_some("computed field name (@field)")
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let callee = trimmed(cx, func);
        if is_assert_equal(callee) {
            return Some(Assertion {
                pos: pos_of(n),
                has_message: false,
                comparison: false,
            });
        }
        if !callee.ends_with("debug.assert") && !callee.ends_with("testing.expect") {
            return None;
        }
        let first = positional_nodes(n, func).first().copied();
        let comparison = first.is_some_and(|a| is_eq_compare(cx, a));
        Some(Assertion {
            pos: pos_of(n),
            has_message: false,
            comparison,
        })
    }
}

fn is_field_call(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    first_of_kind(n, &["builtin_identifier"]).is_some_and(|id| text(cx, id) == "@field")
}

fn is_assert_equal(callee: &str) -> bool {
    callee.ends_with("testing.expectEqual")
        || callee.ends_with("testing.expectEqualStrings")
        || callee.ends_with("testing.expectEqualSlices")
        || callee.ends_with("testing.expectError")
}

fn is_type_kind(kind: &str) -> bool {
    kind == "struct_declaration"
        || kind == "enum_declaration"
        || kind == "union_declaration"
        || kind == "opaque_declaration"
}

fn is_env_callee(callee: &str) -> bool {
    callee.ends_with("process.getEnvVarOwned")
        || callee.ends_with("process.getEnvMap")
        || callee.ends_with("process.hasEnvVar")
        || callee.ends_with("posix.getenv")
        || callee.ends_with("os.getenv")
}
