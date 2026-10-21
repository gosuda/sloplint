use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Import, ImportKind, Param, Pos,
    TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    child_by_field, named_children, narrow, pos_of, preceding_comments, slice, string_arg_shape,
    strip_quotes, text,
};
use crate::lang::c_family::{
    self, angles, collect_fragments, flat, in_function, inner, pair_source, plain_assert, root_of,
    sole_call, span, word,
};
use crate::spec::Role;

/// Role table for C.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("switch_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("conditional_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("preproc_include", Role::Import),
    ("struct_specifier", Role::TypeDef),
    ("union_specifier", Role::TypeDef),
    ("enum_specifier", Role::TypeDef),
    ("type_definition", Role::TypeDef),
    ("comment", Role::Comment),
    ("case_statement", Role::MatchArm),
    ("initializer_list", Role::StructLit),
    ("field_expression", Role::Member),
    ("subscript_expression", Role::Subscript),
    ("preproc_def", Role::Global),
    ("preproc_function_def", Role::Global),
];
/// Hooks for C.
pub(crate) struct CHooks;
/// Hooks instance for C.
pub(crate) static HOOKS: CHooks = CHooks;
const WRAP: &[&str] = &[
    "pointer_declarator",
    "array_declarator",
    "parenthesized_declarator",
    "attributed_declarator",
];
const ENVS: &[&str] = &["getenv", "secure_getenv", "_dupenv_s"];
impl LangHooks for CHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let (fd, name_node) = func_parts(n)?;
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public: !word(cx, n, &["storage_class_specifier"], &["static"]),
            interface_bound: false,
            in_test: cx.is_test_file,
            params: params(cx, fd),
            return_type: return_type(cx, n, fd),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
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
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let args_node = child_by_field(n, "arguments")?;
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: root_of(cx, func).map(|(name, _)| name),
            args: call_args(cx, args_node),
        })
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() != "preproc_include" {
            return;
        }
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
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "struct_specifier" | "union_specifier" | "enum_specifier" => tag_typedef(cx, n),
            "type_definition" => alias_typedef(cx, n),
            _ => None,
        }
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
                    "GetProcAddress" => Some("runtime symbol lookup (GetProcAddress)"),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        if text(cx, func) != "assert" {
            return None;
        }
        Some(plain_assert(cx, n))
    }
}
fn unwrap(n: Node<'_>) -> Node<'_> {
    c_family::unwrap(n, WRAP)
}
fn func_parts(n: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    let fd = unwrap(child_by_field(n, "declarator")?);
    if fd.kind() != "function_declarator" {
        return None;
    }
    let name = unwrap(child_by_field(fd, "declarator")?);
    (name.kind() == "identifier").then_some((fd, name))
}
fn params<'s>(cx: &Cx<'_, 's>, fd: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let Some(list) = child_by_field(fd, "parameters") else {
        return out;
    };
    for c in named_children(list) {
        if c.kind() != "parameter_declaration" {
            continue;
        }
        if let Some(found) = param(cx, c) {
            out.push(found);
        }
    }
    out
}
fn param_target(d: Node<'_>) -> (Option<Node<'_>>, bool) {
    c_family::param_target(d, WRAP)
}
fn param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let ty_node = child_by_field(p, "type")?;
    let declarator = child_by_field(p, "declarator");
    let (named, callable) = declarator.map_or((None, false), param_target);
    let name = named.map(|l| text(cx, l)).unwrap_or("");
    let ty = match (named, declarator) {
        (Some(found), _) if !callable => span(cx, ty_node, found),
        (_, Some(d)) => slice(cx.src, p.start_byte(), d.end_byte()),
        (_, None) => text(cx, ty_node),
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
fn return_type<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
}
fn decl_info<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<(&'s str, Pos)>, bool) {
    c_family::decl_info(cx, n, WRAP)
}
fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    c_family::call_args(cx, args_node, arg_shape)
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "concatenated_string" => concat_shape(cx, n),
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}
fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if any_named_child(n, |c| c.kind() == "identifier") {
        return ArgShape::Concat;
    }
    let mut segments = Vec::new();
    for c in named_children(n) {
        if c.kind() != "string_literal" {
            return ArgShape::Other;
        }
        collect_fragments(cx, c, &["string_content"], &mut segments);
    }
    ArgShape::Str {
        longest_static: longest_static_chunk(&segments),
    }
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
    c_family::env_ambient(cx, n, out, ENVS)
}
