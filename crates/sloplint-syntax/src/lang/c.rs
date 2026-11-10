use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Import, ImportKind, ImportScope,
    Pos, ScrutineeKind, TypeDef, Visibility, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, binary_concat, child_by_field,
    first_named_child, named_children, narrow, pos_of, preceding_comments, string_arg_shape,
    strip_quotes, text,
};
use crate::lang::c_family::{
    self, C_DECLARATORS, angles, in_function, init_value, inner, pair_source, plain_assert,
    root_of, sole_call, span, word,
};
use crate::spec::Role;

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
pub(crate) struct CHooks;
pub(crate) static HOOKS: CHooks = CHooks;
impl LangHooks for CHooks {
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::DeclsAndBinds
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let (fd, name_node) = c_family::func_parts(n, &C_DECLARATORS, &["identifier"])?;
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        let public = !word(cx, n, &["storage_class_specifier"], &["static"]);
        let params = c_family::params(cx, fd, &C_DECLARATORS, &["parameter_declaration"]);
        Some(FunctionHead {
            shadow_params: params
                .iter()
                .map(|param| param.name)
                .filter(|name| !name.is_empty())
                .collect(),
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            signature_forced: false,
            entry_point: text(cx, name_node) == "main" && !in_function(cx),
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            region: None,
            receiver: None,
            params,
            return_type: return_type(cx, n, fd),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
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
        if n.kind() != "call_expression" {
            return None;
        }
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
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        });
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "struct_specifier" | "union_specifier" | "enum_specifier" => tag_typedef(cx, n),
            "type_definition" => alias_typedef(cx, n),
            _ => None,
        }
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if child_by_field(n, "value").is_some() {
            return None;
        }
        let mut ancestors = cx.ancestors();
        let body = ancestors.next();
        let switch = ancestors.next();
        let (scrutinee, variant_count) = match (body, switch) {
            (Some(body), Some(switch)) if switch.kind() == "switch_statement" => {
                (switch_scrutinee(switch), case_labels(body))
            }
            _ => (ScrutineeKind::Other, 0),
        };
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like: variant_count >= 2,
            scrutinee,
            variant_count,
            has_nullable_ctor: false,
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
            "call_expression" => c_family::env_ambient(cx, n, out, C_DECLARATORS.envs),
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
                match text(cx, func) {
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
fn return_type<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
}
fn call_args<'s>(cx: &Cx<'_, 's>, args_node: Node<'_>) -> Vec<Arg<'s>> {
    c_family::call_args(cx, args_node, arg_shape)
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "concatenated_string" => concat_shape(cx, n),
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat {
            longest_static: binary_static(cx, n),
        },
        "identifier" => ArgShape::Ident(text(cx, n)),
        "initializer_list" => init_shape(n),
        "compound_literal_expression" => compound_shape(n),
        _ => ArgShape::Other,
    }
}
fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let mut best = 0;
    let mut current = String::new();
    let mut prefixed = false;
    for c in named_children(n) {
        if c.kind() == "comment" {
            continue;
        }
        if c.kind() == "string_literal" {
            let mut cursor = c.walk();
            for piece in c.children(&mut cursor) {
                if piece.kind() == "escape_sequence" {
                    best = best.max(longest_static_chunk(&[current.as_str()]));
                    current.clear();
                } else if piece.kind() == "string_content" {
                    current.push_str(text(cx, piece));
                }
            }
            continue;
        }
        if c.kind() == "identifier" {
            best = best.max(longest_static_chunk(&[current.as_str()]));
            current.clear();
            prefixed = true;
            continue;
        }
        return ArgShape::Other;
    }
    let longest_static = best.max(longest_static_chunk(&[current.as_str()]));
    if prefixed {
        ArgShape::Concat { longest_static }
    } else {
        ArgShape::Str { longest_static }
    }
}
fn binary_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut best = 0;
    let mut current = n;
    while let (Some(left), Some(right)) = (current.child(0), current.child(2)) {
        for operand in [left, right] {
            if operand.kind() == "string_literal"
                && let ArgShape::Str { longest_static } =
                    string_arg_shape(cx, operand, &["string_content"])
            {
                best = best.max(longest_static);
            }
        }
        if left.kind() == "binary_expression" {
            current = left;
        } else {
            break;
        }
    }
    best
}
fn compound_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    let Some(value) = child_by_field(n, "value") else {
        return ArgShape::Other;
    };
    if value.kind() != "initializer_list" {
        return ArgShape::Other;
    }
    init_shape(value)
}
fn init_shape<'s>(body: Node<'_>) -> ArgShape<'s> {
    let mut code = false;
    for child in named_children(body) {
        match child.kind() {
            "initializer_pair" => return ArgShape::Other,
            "compound_statement"
            | "function_definition"
            | "lambda_expression"
            | "block_literal" => {
                code = true;
            }
            "parenthesized_expression"
                if first_named_child(child)
                    .is_some_and(|inner| inner.kind() == "compound_statement") =>
            {
                code = true;
            }
            _ => {}
        }
    }
    if code {
        ArgShape::CodeBody
    } else {
        ArgShape::Collection
    }
}
fn switch_scrutinee(switch: Node<'_>) -> ScrutineeKind {
    let cond = child_by_field(switch, "condition").and_then(|cond| {
        if cond.kind() == "parenthesized_expression" {
            first_named_child(cond)
        } else {
            Some(cond)
        }
    });
    match cond.map(|node| node.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}
fn case_labels(body: Node<'_>) -> u32 {
    let mut count = 0;
    for sib in named_children(body) {
        if sib.kind() == "case_statement"
            && child_by_field(sib, "value").is_some_and(|value| value.kind() == "identifier")
        {
            count += 1;
        }
    }
    narrow(count)
}
fn tag_typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    let public = !in_function(cx);
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: if public {
                Visibility::Exported
            } else {
                Visibility::Private
            },
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
            let public = !in_function(cx);
            return Some(TypeHead {
                def: Some(TypeDef {
                    name: text(cx, current),
                    pos: pos_of(current),
                    visibility: if public {
                        Visibility::Exported
                    } else {
                        Visibility::Private
                    },
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
    let (names, func) = c_family::decl_info(cx, n, C_DECLARATORS.wrap);
    if func {
        return;
    }
    if let Some((name, pos)) = names.into_iter().next() {
        out.push((AmbientKind::GlobalMutable, name, pos));
    }
}
