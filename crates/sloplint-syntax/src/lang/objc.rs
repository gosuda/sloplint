use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, Import, ImportKind, Param,
    Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    named_children, narrow, pos_of, preceding_comments, slice, string_arg_shape, strip_quotes,
    text,
};
use crate::spec::Role;

/// Role table for Objective-C.
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
/// Hooks for Objective-C.
pub(crate) struct ObjcHooks;
/// Hooks instance for Objective-C.
pub(crate) static HOOKS: ObjcHooks = ObjcHooks;
const WRAP: &[&str] = &[
    "pointer_declarator",
    "array_declarator",
    "parenthesized_declarator",
    "attributed_declarator",
];
const ENVS: &[&str] = &["getenv", "secure_getenv", "_dupenv_s"];
impl LangHooks for ObjcHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        match n.kind() {
            "function_definition" => c_function(cx, n),
            "method_definition" => method(cx, n),
            _ => None,
        }
    }
    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent().is_some_and(|p| p.kind() == "else_clause")
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if word(cx, n, &["type_qualifier"], &["const"]) {
            return;
        }
        if word(cx, n, &["storage_class_specifier"], &["extern"]) {
            return;
        }
        out.extend(decl_info(cx, n).0);
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
                Some(CallHead {
                    callee: text(cx, func),
                    kind: CallKind::Plain,
                    callee_root: root_of(cx, func).map(|(name, _)| name),
                    args: call_args(cx, args_node),
                })
            }
            "message_expression" => {
                let callee = first_selector(cx, n)?;
                let receiver = child_by_field(n, "receiver");
                let mut args = Vec::new();
                for c in named_children(n) {
                    if Some(c) == receiver || c.kind() == "comment" {
                        continue;
                    }
                    args.push(mk_arg(cx, c));
                }
                Some(CallHead {
                    callee,
                    kind: CallKind::Plain,
                    callee_root: receiver.and_then(|r| msg_root(cx, r)),
                    args,
                })
            }
            _ => None,
        }
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        match n.kind() {
            "preproc_include" => push_include(cx, n, out),
            "module_import" => push_module(cx, n, out),
            _ => {}
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "struct_specifier" | "union_specifier" | "enum_specifier" => tag_typedef(cx, n),
            "type_definition" => alias_typedef(cx, n),
            "class_interface" => class_typedef(cx, n),
            "protocol_declaration" => protocol_typedef(cx, n),
            _ => None,
        }
    }
    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        (n.kind() == "class_implementation").then_some(Container::Impl {
            interface_bound: true,
        })
    }
    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if child_by_field(n, "value").is_some() {
            return None;
        }
        let mut enum_like = false;
        if let Some(parent) = n.parent() {
            for sib in named_children(parent) {
                if sib.kind() == "case_statement"
                    && child_by_field(sib, "value").is_some_and(|v| v.kind() == "identifier")
                {
                    enum_like = true;
                    break;
                }
            }
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
        let bound = catch_binding(cx, n);
        let body = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "compound_statement")?;
        let mut stmts = Vec::new();
        for stmt in named_children(body) {
            if stmt.kind() == "comment" {
                continue;
            }
            stmts.push(catch_stmt(cx, stmt, bound));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
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
            "declaration" if !in_function => global_ambient(cx, n, out),
            "call_expression" => env_ambient(cx, n, out),
            "field_expression" => {
                if let Some((name, pos)) = dot_env(cx, n) {
                    out.push((AmbientKind::EnvRead, name, pos));
                }
            }
            "message_expression" => {
                if let Some((name, pos)) = msg_env(cx, n) {
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
                match flat(text(cx, func)).as_str() {
                    "dlsym" | "dlvsym" => Some("runtime symbol lookup (dlsym)"),
                    "NSClassFromString" => Some("runtime class lookup (NSClassFromString)"),
                    "NSSelectorFromString" => {
                        Some("runtime selector lookup (NSSelectorFromString)")
                    }
                    "objc_getClass" | "objc_getProtocol" => {
                        Some("runtime class lookup (objc_getClass)")
                    }
                    _ => None,
                }
            }
            "message_expression" => first_selector(cx, n)
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
            .map(|a| {
                named_children(a)
                    .into_iter()
                    .filter(|c| c.kind() != "comment")
                    .count()
            })
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
        })
    }
}
fn flat(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn angles(t: &str) -> &str {
    t.strip_prefix('<')
        .unwrap_or(t)
        .strip_suffix('>')
        .unwrap_or(t)
}
fn word<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kinds: &[&str], words: &[&str]) -> bool {
    named_children(n)
        .iter()
        .any(|c| kinds.contains(&c.kind()) && words.contains(&text(cx, *c)))
}
fn inner(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "declarator").or_else(|| named_children(n).into_iter().next())
}
fn unwrap(n: Node<'_>) -> Node<'_> {
    let mut current = n;
    loop {
        if !WRAP.contains(&current.kind()) {
            break;
        }
        let Some(found) = inner(current) else {
            break;
        };
        current = found;
    }
    current
}
fn leaf(n: Node<'_>) -> Node<'_> {
    unwrap(match n.kind() {
        "init_declarator" => child_by_field(n, "declarator").unwrap_or(n),
        _ => n,
    })
}
fn c_function<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body")?;
    let (fd, name_node) = func_parts(n)?;
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    Some(FunctionHead {
        name: text(cx, name_node),
        name_pos: pos_of(name_node),
        public: !word(cx, n, &["storage_class_specifier"], &["static"]),
        interface_bound: false,
        in_test: cx.is_test_file,
        params: c_params(cx, fd),
        return_type: c_return(cx, n, fd),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call(body, &["call_expression"]),
    })
}
fn func_parts(n: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    let fd = unwrap(child_by_field(n, "declarator")?);
    if fd.kind() != "function_declarator" {
        return None;
    }
    let name = unwrap(child_by_field(fd, "declarator")?);
    (name.kind() == "identifier").then_some((fd, name))
}
fn span<'s>(cx: &Cx<'_, 's>, ty: Node<'_>, end: Node<'_>) -> &'s str {
    let shown = slice(cx.src, ty.start_byte(), end.start_byte()).trim();
    if !shown.is_empty() {
        shown
    } else {
        text(cx, ty)
    }
}
fn c_params<'s>(cx: &Cx<'_, 's>, fd: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let Some(list) = child_by_field(fd, "parameters") else {
        return out;
    };
    for c in named_children(list) {
        if c.kind() != "parameter_declaration" {
            continue;
        }
        if let Some(found) = c_param(cx, c) {
            out.push(found);
        }
    }
    out
}
fn c_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let ty_node = child_by_field(p, "type")?;
    let named = child_by_field(p, "declarator")
        .map(unwrap)
        .filter(|l| l.kind() == "identifier");
    let name = named.map(|l| text(cx, l)).unwrap_or("");
    let ty = match named {
        Some(found) => span(cx, ty_node, found),
        None => text(cx, ty_node),
    };
    if name.is_empty() && ty == "void" {
        return None;
    }
    Some(Param {
        name,
        ty: Some(ty),
        bool_default: false,
    })
}
fn c_return<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
}
fn parens(raw: &str) -> &str {
    if raw.starts_with('(') && raw.ends_with(')') && raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}
fn method<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "compound_statement")?;
    let name_node = selector_name(n)?;
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    Some(FunctionHead {
        name: text(cx, name_node),
        name_pos: pos_of(name_node),
        public: true,
        interface_bound: true,
        in_test: cx.is_test_file || impl_is_test(cx, n),
        params: method_params(cx, n),
        return_type: method_return(cx, n),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call(body, &["call_expression", "message_expression"]),
    })
}
fn selector_name(n: Node<'_>) -> Option<Node<'_>> {
    let kids = named_children(n);
    match kids.iter().find(|c| c.kind() == "keyword_declarator") {
        Some(kd) => named_children(*kd)
            .into_iter()
            .find(|c| c.kind() == "identifier"),
        None => kids.into_iter().find(|c| c.kind() == "identifier"),
    }
}
fn impl_is_test<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    let mut parent = n.parent();
    while let Some(found) = parent {
        if found.kind() == "class_implementation" {
            return named_children(found)
                .into_iter()
                .find(|c| c.kind() == "identifier")
                .is_some_and(|id| {
                    let name = text(cx, id);
                    name.ends_with("Test") || name.ends_with("Tests")
                });
        }
        parent = found.parent();
    }
    false
}
fn method_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    for c in named_children(n) {
        let found = match c.kind() {
            "keyword_declarator" => keyword_param(cx, c),
            "method_parameter" => method_param(cx, c),
            _ => None,
        };
        if let Some(found) = found {
            out.push(found);
        }
    }
    out
}
fn keyword_param<'s>(cx: &Cx<'_, 's>, kd: Node<'_>) -> Option<Param<'s>> {
    let ids: Vec<Node<'_>> = named_children(kd)
        .into_iter()
        .filter(|c| c.kind() == "identifier")
        .collect();
    let name_node = *ids.last()?;
    let ty = named_children(kd)
        .into_iter()
        .find(|c| c.kind() == "method_type")
        .map(|t| parens(text(cx, t)));
    Some(Param {
        name: text(cx, name_node),
        ty,
        bool_default: false,
    })
}
fn method_param<'s>(cx: &Cx<'_, 's>, mp: Node<'_>) -> Option<Param<'s>> {
    let name_node = named_children(mp)
        .into_iter()
        .find(|c| c.kind() == "identifier")?;
    let ty = child_by_field(mp, "type").map(|t| text(cx, t));
    Some(Param {
        name: text(cx, name_node),
        ty,
        bool_default: false,
    })
}
fn method_return<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    named_children(n)
        .into_iter()
        .find(|c| c.kind() == "method_type")
        .map(|t| parens(text(cx, t)))
}
fn sole_call(body: Node<'_>, kinds: &[&str]) -> Option<usize> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let mut stmts = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment");
    let stmt = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    if stmt.kind() != "expression_statement" && stmt.kind() != "return_statement" {
        return None;
    }
    let expr =
        child_by_field(stmt, "expression").or_else(|| named_children(stmt).into_iter().next())?;
    if expr.kind() != "call_expression" && expr.kind() != "message_expression" {
        return None;
    }
    kinds.contains(&expr.kind()).then_some(expr.id())
}
fn decl_info<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<(&'s str, Pos)>, bool) {
    let mut cursor = n.walk();
    let ds: Vec<Node<'_>> = n
        .children_by_field_name("declarator", &mut cursor)
        .collect();
    let mut out = Vec::new();
    let mut func = false;
    for d in ds {
        let found = leaf(d);
        if found.kind() == "function_declarator" {
            func = true;
        } else if found.kind() == "identifier" {
            out.push((text(cx, found), pos_of(found)));
        }
    }
    (out, func)
}
fn root_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" => return Some((text(cx, current), pos_of(current))),
            "field_expression" | "subscript_expression" | "pointer_expression" => {
                let next = child_by_field(current, "argument")?;
                current = next
            }
            "parenthesized_expression" => {
                let next = named_children(current).into_iter().next()?;
                current = next
            }
            _ => return None,
        }
    }
}
fn msg_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut current = n;
    loop {
        if current.kind() == "identifier" {
            return Some(text(cx, current));
        }
        if current.kind() == "message_expression" {
            {
                let next = child_by_field(current, "receiver")?;
                current = next
            }
        } else {
            return None;
        }
    }
}
fn first_selector<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut cursor = n.walk();
    let methods: Vec<Node<'_>> = n.children_by_field_name("method", &mut cursor).collect();
    methods
        .into_iter()
        .find(|c| c.kind() == "identifier")
        .map(|c| text(cx, c))
}
fn mk_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}
fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    let mut args = Vec::new();
    for c in named_children(args_node) {
        if c.kind() == "comment" {
            continue;
        }
        args.push(mk_arg(cx, c));
    }
    args
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if n.kind() == "at_expression" {
        return match named_children(n).into_iter().next() {
            Some(found) if found.kind() == "string_literal" => {
                string_arg_shape(cx, found, &["string_content"])
            }
            _ => ArgShape::Other,
        };
    }
    match n.kind() {
        "string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "concatenated_string" => concat_shape(cx, n),
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}
fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let kids = named_children(n);
    if kids.iter().any(|c| c.kind() == "identifier") {
        return ArgShape::Concat;
    }
    let mut segments = Vec::new();
    for c in kids {
        if c.kind() != "string_literal" {
            return ArgShape::Other;
        }
        collect_fragments(cx, c, &["string_content"], &mut segments);
    }
    ArgShape::Str {
        longest_static: longest_static_chunk(&segments),
    }
}
fn collect_fragments<'s>(cx: &Cx<'_, 's>, n: Node<'_>, kinds: &[&str], out: &mut Vec<&'s str>) {
    for i in 0..n.child_count() {
        if let Some(found) = n.child(i)
            && kinds.contains(&found.kind())
        {
            out.push(text(cx, found));
        }
    }
}
fn push_include<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(path) = child_by_field(n, "path") else {
        return;
    };
    let module = match path.kind() {
        "string_literal" => strip_quotes(text(cx, path)),
        "system_lib_string" => angles(text(cx, path)),
        _ => text(cx, path),
    };
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name: "",
        alias: None,
        kind: ImportKind::SideEffect,
        in_test: cx.is_test_file,
    });
}
fn push_module<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let raw = flat(text(cx, n));
    let inner_text = raw.strip_prefix("@import").unwrap_or(&raw);
    let module = inner_text.strip_suffix(';').unwrap_or(inner_text);
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Owned(module.to_string()),
        name: "",
        alias: None,
        kind: ImportKind::SideEffect,
        in_test: cx.is_test_file,
    });
}
fn in_function(n: Node<'_>) -> bool {
    let mut parent = n.parent();
    while let Some(found) = parent {
        if found.kind() == "function_definition" {
            return true;
        }
        parent = found.parent();
    }
    false
}
fn tag_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            public: !in_function(n),
            abstraction: false,
        }),
        impls: Vec::new(),
        container: None,
    })
}
fn alias_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let mut current = child_by_field(n, "declarator")?;
    loop {
        if current.kind() == "type_identifier" || current.kind() == "identifier" {
            return Some(TypeHead {
                def: Some(TypeDef {
                    name: text(cx, current),
                    pos: pos_of(current),
                    public: !in_function(n),
                    abstraction: false,
                }),
                impls: Vec::new(),
                container: None,
            });
        }
        {
            let next = inner(current)?;
            current = next
        }
    }
}
fn class_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if child_by_field(n, "category").is_some() {
        return None;
    }
    let superclass = child_by_field(n, "superclass");
    let name_node = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "identifier" && Some(*c) != superclass)?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            public: true,
            abstraction: false,
        }),
        impls: Vec::new(),
        container: None,
    })
}
fn protocol_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let name_node = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "identifier")?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            public: true,
            abstraction: true,
        }),
        impls: Vec::new(),
        container: None,
    })
}
fn catch_binding<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let type_name = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "type_name")?;
    let id = named_children(type_name)
        .into_iter()
        .find(|c| c.kind() == "identifier");
    if let Some(found) = id {
        return Some(text(cx, found));
    }
    let declarator = named_children(type_name)
        .into_iter()
        .find(|c| c.kind() == "pointer_declarator")?;
    first_ident(cx, declarator)
}
fn first_ident<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut stack = named_children(n);
    while let Some(next) = stack.pop() {
        if next.kind() == "identifier" {
            return Some(text(cx, next));
        }
        stack.extend(named_children(next));
    }
    None
}
fn catch_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, bound: Option<&'s str>) -> Stmt<'s> {
    if stmt.kind() == "expression_statement" {
        let expr =
            child_by_field(stmt, "expression").or_else(|| named_children(stmt).into_iter().next());
        if expr.is_some_and(|e| e.kind() == "call_expression") {
            return call_stmt(cx, expr);
        }
        if expr.is_some_and(|e| e.kind() == "message_expression") {
            let callee = expr.and_then(|e| first_selector(cx, e)).unwrap_or("");
            return Stmt::Call {
                callee,
                kind: CallKind::Plain,
            };
        }
        return Stmt::Other;
    }
    if stmt.kind() == "message_expression" {
        return Stmt::Call {
            callee: first_selector(cx, stmt).unwrap_or(""),
            kind: CallKind::Plain,
        };
    }
    if stmt.kind() == "throw_statement" {
        let thrown = named_children(stmt).into_iter().next();
        match thrown {
            None => return Stmt::Rethrow,
            Some(found) if found.kind() == "identifier" && Some(text(cx, found)) == bound => {
                return Stmt::Rethrow;
            }
            _ => return Stmt::Other,
        }
    }
    Stmt::Other
}
fn call_stmt<'s>(cx: &Cx<'_, 's>, expr: Option<Node<'_>>) -> Stmt<'s> {
    let callee = expr
        .and_then(|e| child_by_field(e, "function"))
        .map(|f| text(cx, f))
        .unwrap_or("");
    Stmt::Call {
        callee,
        kind: CallKind::Plain,
    }
}
fn pair_source<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    if pair.kind() != "initializer_pair" {
        return None;
    }
    let value = child_by_field(pair, "value")?;
    if value.kind() != "field_expression" {
        return None;
    }
    let designator = designator_of(cx, pair)?;
    let field = child_by_field(value, "field")?;
    if designator != text(cx, field) {
        return None;
    }
    let arg = child_by_field(value, "argument")?;
    if arg.kind() != "identifier" {
        return None;
    }
    Some(text(cx, arg))
}
fn designator_of<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    for c in named_children(pair) {
        if c.kind() == "field_designator" {
            let found = named_children(c)
                .into_iter()
                .find(|d| d.kind() == "field_identifier")?;
            return Some(text(cx, found));
        }
        if c.kind() == "field_identifier" {
            return Some(text(cx, c));
        }
    }
    None
}
fn global_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if word(cx, n, &["type_qualifier"], &["const"]) {
        return;
    }
    if word(cx, n, &["storage_class_specifier"], &["extern"]) {
        return;
    }
    let (names, func) = decl_info(cx, n);
    if func {
        return;
    }
    if let Some((name, pos)) = names.into_iter().next() {
        out.push((AmbientKind::GlobalMutable, name, pos));
    }
}
fn env_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let raw = text(cx, func);
    if ENVS.contains(&flat(raw).as_str()) {
        out.push((AmbientKind::EnvRead, raw, pos_of(n)));
    }
}
fn dot_env<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let field = child_by_field(n, "field")?;
    if text(cx, field) != "environment" {
        return None;
    }
    let arg = child_by_field(n, "argument")?;
    let raw = flat(text(cx, arg));
    (raw == "NSProcessInfo.processInfo").then_some((text(cx, n), pos_of(n)))
}
fn msg_env<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cursor = n.walk();
    let methods: Vec<Node<'_>> = n.children_by_field_name("method", &mut cursor).collect();
    if methods.len() != 1
        || methods[0].kind() != "identifier"
        || text(cx, methods[0]) != "environment"
    {
        return None;
    }
    let receiver = child_by_field(n, "receiver")?;
    let raw = flat(text(cx, receiver));
    (raw == "[NSProcessInfoprocessInfo]").then_some((text(cx, n), pos_of(n)))
}
fn bin_op<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if n.child_count() != 3 {
        return None;
    }
    n.child(1).map(|op| text(cx, op))
}
fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|a| {
        named_children(a)
            .into_iter()
            .find(|c| c.kind() != "comment")
    })
}
fn plain_assert(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let arg = first_arg(n).filter(|a| a.kind() == "binary_expression");
    let mut has_message = false;
    let mut comparison = false;
    if let Some(found) = arg {
        let op = bin_op(cx, found).unwrap_or("");
        if op == "&&" {
            has_message =
                child_by_field(found, "right").is_some_and(|r| r.kind() == "string_literal");
        }
        comparison = op == "==" || op == "!=";
    }
    Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
    }
}
