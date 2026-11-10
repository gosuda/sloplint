use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, AssertShape, Assertion, CallKind, Handler, ImplRef, Import,
    ImportKind, ImportScope, Param, Pos, PrimitiveKind, ScrutineeKind, Stmt, TypeDef, Visibility,
    WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, Link, TypeHead, any_named_child,
    attributed_region, binary_concat, child_by_field, count_named_child, find_named_child,
    first_named_child_matching, first_of_kind, has_child_kind, last_segment, named_children,
    narrow, only_named_child_matching, pos_of, preceding_comments, rfind_named_child, slice,
    string_arg_shape, text,
};
use crate::spec::Role;
use sloplint_core::role::CodeRole;

pub(crate) static TABLE: &[(&str, Role)] = &[
    ("method_declaration", Role::Function),
    ("constructor_declaration", Role::Function),
    ("compact_constructor_declaration", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_statement", Role::If),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("enhanced_for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("ternary_expression", Role::Ternary),
    ("method_invocation", Role::Call),
    ("object_creation_expression", Role::NewCall),
    ("local_variable_declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("import_declaration", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("record_declaration", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("switch_label", Role::MatchArm),
    ("assert_statement", Role::Assert),
    ("field_declaration", Role::StaticItem),
];

pub(crate) static HOOKS: JavaHooks = JavaHooks;

pub(crate) struct JavaHooks;

const COMMENTS: &[&str] = &["line_comment", "block_comment"];
const TEST_ANNOTATIONS: &[&str] = &[
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];
const NUMERIC_KINDS: &[&str] = &[
    "decimal_integer_literal",
    "decimal_floating_point_literal",
    "hex_integer_literal",
    "octal_integer_literal",
    "binary_integer_literal",
];
const REFLECTION: &[&str] = &[
    "getMethod",
    "getDeclaredMethod",
    "getField",
    "getDeclaredField",
    "getConstructor",
    "getDeclaredConstructor",
];

fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| child.kind() != "comment")
        .collect()
}

fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| child.kind() != "comment")
}

fn modifiers_of(n: Node<'_>) -> Option<Node<'_>> {
    first_of_kind(n, &["modifiers"])
}

fn has_keyword(n: Node<'_>, keyword: &str) -> bool {
    modifiers_of(n).is_some_and(|modifiers| has_child_kind(modifiers, keyword))
}

fn annotation_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<&'s str> {
    let Some(modifiers) = modifiers_of(n) else {
        return Vec::new();
    };
    named_children(modifiers)
        .into_iter()
        .filter(|child| matches!(child.kind(), "marker_annotation" | "annotation"))
        .filter_map(|child| child_by_field(child, "name"))
        .map(|name| last_segment(text(cx, name), &["."]))
        .collect()
}

fn arguments_of(n: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(n, "arguments").map_or_else(Vec::new, code_children)
}

fn method_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let name = child_by_field(n, "name")?;
    Some(match child_by_field(n, "object") {
        Some(object) => slice(cx.src, object.start_byte(), name.end_byte()),
        None => text(cx, name),
    })
}

fn numeric_literal(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        if NUMERIC_KINDS.contains(&current.kind()) {
            return true;
        }
        if current.kind() != "unary_expression" {
            return false;
        }
        if !child_by_field(current, "operator").is_some_and(|op| op.kind() == "-") {
            return false;
        }
        match child_by_field(current, "operand") {
            Some(next) => current = next,
            None => return false,
        }
    }
}

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
    }
}

fn primitive_of(ty: &str) -> PrimitiveKind {
    match ty.trim() {
        "String" => PrimitiveKind::Str,
        "boolean" | "Boolean" => PrimitiveKind::Bool,
        "byte" | "short" | "int" | "long" | "char" | "Byte" | "Short" | "Integer" | "Long"
        | "Character" => PrimitiveKind::Int,
        "float" | "double" | "Float" | "Double" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn spread_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let ty_node = find_named_child(p, |child| {
        !matches!(
            child.kind(),
            "modifiers" | "annotation" | "marker_annotation" | "variable_declarator"
        )
    })?;
    let declarator = first_of_kind(p, &["variable_declarator"])?;
    let name = text(cx, child_by_field(declarator, "name")?);
    let ty = match first_of_kind(p, &["..."]) {
        Some(dots) => slice(cx.src, ty_node.start_byte(), dots.end_byte()),
        None => text(cx, ty_node),
    };
    Some(Param {
        name,
        ty: Some(ty),
        primitive_kind: primitive_of(ty),
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: true,
    })
}

fn formal_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    match p.kind() {
        "formal_parameter" => {
            let name = text(cx, child_by_field(p, "name")?);
            let type_node = child_by_field(p, "type")?;
            let ty = match child_by_field(p, "dimensions") {
                Some(dims) => slice(cx.src, type_node.start_byte(), dims.end_byte()),
                None => text(cx, type_node),
            };
            Some(Param {
                name,
                ty: Some(ty),
                primitive_kind: primitive_of(ty),
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            })
        }
        "spread_parameter" => spread_param(cx, p),
        _ => None,
    }
}
fn formal_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Param<'s>> {
    code_children(list)
        .into_iter()
        .filter_map(|p| formal_param(cx, p))
        .collect()
}

fn statement_call(statement: Node<'_>) -> Option<usize> {
    if !matches!(
        statement.kind(),
        "expression_statement" | "return_statement"
    ) {
        return None;
    }
    let expression = first_code(statement)?;
    (expression.kind() == "method_invocation").then(|| expression.id())
}

fn sole_call_of(n: Node<'_>, body: Node<'_>) -> Option<usize> {
    if n.kind() != "method_declaration" {
        return None;
    }
    let only = only_named_child_matching(body, |c| c.kind() != "comment")?;
    statement_call(only)
}

fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body")?;
    let name = child_by_field(n, "name")?;
    let annotations = annotation_names(cx, n);
    let in_trait = matches!(cx.innermost_container(), Some(Container::Trait { .. }));
    let private = has_keyword(n, "private");
    let visibility = if private {
        Visibility::Private
    } else if has_keyword(n, "public") || has_keyword(n, "protected") || in_trait {
        Visibility::Exported
    } else {
        Visibility::Restricted
    };
    let visibility = if visibility == Visibility::Exported && !container_exports(cx) {
        Visibility::Restricted
    } else {
        visibility
    };
    let tested = annotations.iter().any(|a| TEST_ANNOTATIONS.contains(a));
    let benched = !tested && annotations.contains(&"Benchmark");
    let name_text = text(cx, name);
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENTS, &[]);
    Some(FunctionHead {
        name: name_text,
        name_pos: pos_of(name),
        visibility,
        signature_forced: annotations.contains(&"Override"),
        entry_point: name_text == "main",
        in_test: tested || benched,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: in_trait,
        region: tested
            .then(|| attributed_region(n, modifiers_of(n), CodeRole::Test))
            .or_else(|| benched.then(|| attributed_region(n, modifiers_of(n), CodeRole::Bench))),
        params: child_by_field(n, "parameters")
            .map_or_else(Vec::new, |list| formal_params(cx, list)),
        return_type: child_by_field(n, "type").map(|ty| text(cx, ty)),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_of(n, body),
    })
}
fn container_exports(cx: &Cx<'_, '_>) -> bool {
    !matches!(
        cx.innermost_container(),
        Some(Container::Class {
            exported: false,
            ..
        }) | Some(Container::Trait { public: false, .. })
    )
}

fn type_name(n: Node<'_>) -> Option<Node<'_>> {
    let mut current = n;
    loop {
        match current.kind() {
            "type_identifier" => return Some(current),
            "generic_type" => {
                current = first_named_child_matching(
                    current,
                    &["type_identifier", "scoped_type_identifier"],
                )?;
            }
            "scoped_type_identifier" => {
                return rfind_named_child(current, |child| child.kind() == "type_identifier");
            }
            _ => return None,
        }
    }
}

fn push_type_ref<'s>(cx: &Cx<'_, 's>, ty: Node<'_>, refs: &mut Vec<ImplRef<'s>>) {
    if let Some(name) = type_name(ty) {
        refs.push(ImplRef {
            name: text(cx, name),
            pos: pos_of(name),
            is_generic: false,
            child_is_abstract: false,
        });
    }
}

fn supertype_refs<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<ImplRef<'s>> {
    let mut refs = Vec::new();
    for clause in named_children(n) {
        match clause.kind() {
            "superclass" => {
                for ty in named_children(clause) {
                    push_type_ref(cx, ty, &mut refs);
                }
            }
            "super_interfaces" | "extends_interfaces" => {
                for list in named_children(clause) {
                    for ty in named_children(list) {
                        push_type_ref(cx, ty, &mut refs);
                    }
                }
            }
            _ => {}
        }
    }
    refs
}

fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if !matches!(
        n.kind(),
        "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
    ) {
        return None;
    }
    let name = child_by_field(n, "name")?;
    let visibility = if has_keyword(n, "private") {
        Visibility::Private
    } else if has_keyword(n, "public") || has_keyword(n, "protected") {
        Visibility::Exported
    } else {
        Visibility::Restricted
    };
    let visibility = if visibility == Visibility::Exported && !container_exports(cx) {
        Visibility::Restricted
    } else {
        visibility
    };
    let exported = visibility == Visibility::Exported;
    let is_interface = n.kind() == "interface_declaration";
    let abstraction =
        is_interface || (n.kind() == "class_declaration" && has_keyword(n, "abstract"));
    let container = if is_interface {
        Container::Trait { public: exported }
    } else {
        Container::Class {
            exported,
            implements: false,
        }
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name),
            pos: pos_of(name),
            visibility,
            abstraction,
        }),
        impls: supertype_refs(cx, n),
        container: Some(container),
    })
}

fn import_fact<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Import<'s>> {
    let path = first_of_kind(n, &["scoped_identifier", "identifier"])?;
    let pos = pos_of(n);
    if has_child_kind(n, "asterisk") {
        return Some(Import {
            pos,
            module: Cow::Borrowed(text(cx, path)),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        });
    }
    let (module, name) = match path.kind() {
        "scoped_identifier" => (
            child_by_field(path, "scope").map_or("", |scope| text(cx, scope)),
            child_by_field(path, "name").map_or("", |leaf| text(cx, leaf)),
        ),
        _ => ("", text(cx, path)),
    };
    Some(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias: None,
        kind: ImportKind::Named,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    })
}

const RECEIVER_CHAIN: u8 = 2;

fn is_chain_root(t: &str) -> bool {
    !matches!(t, "true" | "false" | "null")
        && !t.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || !b.is_ascii())
}

fn receiver_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "identifier" | "this" | "super" | "field_access" | "method_invocation" => {}
        _ => return None,
    }
    let root = cx.chain_root(RECEIVER_CHAIN, n, |current| match current.kind() {
        "field_access" | "method_invocation" => match child_by_field(current, "object") {
            Some(next) => Link::Next(next),
            None => Link::Root(current),
        },
        _ => Link::Root(current),
    });
    is_chain_root(root).then_some(root)
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => {
            string_arg_shape(cx, n, &["string_fragment", "multiline_string_fragment"])
        }
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat,
        "identifier" => ArgShape::Ident(text(cx, n)),
        "lambda_expression" => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arguments_of(n)
        .into_iter()
        .map(|arg| span_arg(arg, arg_shape(cx, arg)))
        .collect()
}

fn declares_value(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    cx.parent()
        .filter(|parent| parent.kind() == "variable_declarator")
        .and_then(|parent| child_by_field(parent, "value"))
        .is_some_and(|value| value.id() == n.id())
}

fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "method_invocation" => {
            let name = child_by_field(n, "name")?;
            let object = child_by_field(n, "object");
            let callee_root = match object {
                Some(found) => receiver_root(cx, found),
                None => Some(text(cx, name)),
            };
            Some(CallHead {
                callee: method_callee(cx, n)?,
                kind: CallKind::Plain,
                callee_root,
                is_simple_path: object.is_none(),
                initializes_binding: declares_value(cx, n),
                args: call_args(cx, n),
            })
        }
        "object_creation_expression" => {
            let ty = child_by_field(n, "type")?;
            Some(CallHead {
                callee: text(cx, ty),
                kind: CallKind::New,
                callee_root: None,
                is_simple_path: true,
                initializes_binding: declares_value(cx, n),
                args: call_args(cx, n),
            })
        }
        _ => None,
    }
}

fn is_else_if(n: Node<'_>, parent: Option<Node<'_>>) -> bool {
    parent
        .filter(|parent| parent.kind() == "if_statement")
        .and_then(|parent| child_by_field(parent, "alternative"))
        .is_some_and(|alternative| alternative.id() == n.id())
}

fn local_declared<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "local_variable_declaration" || has_keyword(n, "final") {
        return;
    }
    for declarator in named_children(n) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name) = child_by_field(declarator, "name") else {
            continue;
        };
        if name.kind() == "identifier" {
            out.push((text(cx, name), pos_of(name)));
        }
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some((text(cx, current), pos_of(current))),
            "field_access" => child_by_field(current, "object")?,
            "array_access" => child_by_field(current, "array")?,
            _ => return None,
        };
    }
}

fn write_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let target = match n.kind() {
        "assignment_expression" => child_by_field(n, "left"),
        "update_expression" => first_code(n),
        _ => None,
    };
    out.extend(target.and_then(|target| write_root(cx, target)));
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    let Some(inner) = first_code(stmt) else {
        return Stmt::Other;
    };
    match (stmt.kind(), inner.kind()) {
        ("expression_statement", "method_invocation") => Stmt::Call {
            callee: method_callee(cx, inner).unwrap_or(""),
            kind: CallKind::Plain,
        },
        ("throw_statement", "identifier") if binding == Some(text(cx, inner)) => Stmt::Rethrow,
        _ => Stmt::Other,
    }
}

fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let parameter = first_of_kind(n, &["catch_formal_parameter"])?;
    let binding = child_by_field(parameter, "name").map(|name| text(cx, name));
    let body = child_by_field(n, "body")?;
    let stmts = code_children(body)
        .into_iter()
        .map(|stmt| handler_stmt(cx, stmt, binding))
        .collect();
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
}

fn sibling_labels<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Vec<Node<'t>> {
    let Some(group) = cx.parent_of(n) else {
        return Vec::new();
    };
    let Some(block) = cx.parent_of(group) else {
        return Vec::new();
    };
    let mut labels = Vec::new();
    for child in named_children(block) {
        if child.kind() == "switch_label" {
            labels.push(child);
        } else {
            for label in named_children(child) {
                if label.kind() == "switch_label" {
                    labels.push(label);
                }
            }
        }
    }
    labels
}

fn switch_subject<'t>(cx: &Cx<'t, '_>, n: Node<'_>) -> Option<Node<'t>> {
    let mut current = cx.parent_of(n);
    while let Some(node) = current {
        if node.kind() == "switch_expression" || node.kind() == "switch_statement" {
            return Some(node);
        }
        current = cx.parent_of(node);
    }
    None
}

fn scrutinee_of(subject: Node<'_>) -> ScrutineeKind {
    let cond = child_by_field(subject, "condition").and_then(|cond| {
        if cond.kind() == "parenthesized_expression" {
            first_code(cond)
        } else {
            Some(cond)
        }
    });
    match cond.map(|selected| selected.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_access") => ScrutineeKind::Field,
        Some("scoped_identifier") => ScrutineeKind::Path,
        _ => ScrutineeKind::Other,
    }
}

fn label_is_enum(label: Node<'_>) -> bool {
    any_named_child(label, |child| {
        matches!(
            child.kind(),
            "pattern"
                | "type_pattern"
                | "record_pattern"
                | "identifier"
                | "field_access"
                | "scoped_identifier"
        )
    })
}

fn switch_label_arm(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "switch_label" || n.named_child_count() != 0 || text(cx, n).trim() != "default" {
        return None;
    }
    let subject = switch_subject(cx, n)?;
    let variant_count = narrow(
        sibling_labels(cx, n)
            .iter()
            .filter(|label| label_is_enum(**label))
            .count(),
    );
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variant_count >= 2,
        scrutinee: scrutinee_of(subject),
        variant_count,
        has_nullable_ctor: false,
    })
}
fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "method_invocation" {
        return None;
    }
    let name = text(cx, child_by_field(n, "name")?);
    let on_class = child_by_field(n, "object").is_some_and(|object| text(cx, object) == "Class");
    if !((name == "forName" && on_class) || REFLECTION.contains(&name)) {
        return None;
    }
    let args = child_by_field(n, "arguments")?;
    let mut cursor = args.walk();
    let first = args
        .named_children(&mut cursor)
        .find(|child| !COMMENTS.contains(&child.kind()))?;
    (first.kind() != "string_literal").then_some("reflection by name")
}

fn string_valued(n: Node<'_>) -> bool {
    let mut pending = vec![n];
    while let Some(current) = pending.pop() {
        match current.kind() {
            "string_literal" => return true,
            "parenthesized_expression" => pending.extend(named_children(current)),
            "binary_expression"
                if child_by_field(current, "operator").is_some_and(|op| op.kind() == "+") =>
            {
                pending.extend(
                    [
                        child_by_field(current, "left"),
                        child_by_field(current, "right"),
                    ]
                    .into_iter()
                    .flatten(),
                );
            }
            _ => {}
        }
    }
    false
}

fn equality_has_message(args: &[Node<'_>]) -> bool {
    match args {
        [first, _, third] => !numeric_literal(*third) || string_valued(*first),
        [_, _, _, _, ..] => true,
        _ => false,
    }
}

fn method_assertion(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    let name = text(cx, child_by_field(n, "name")?);
    let args = arguments_of(n);
    let count = args.len();
    let (has_message, comparison) = match name {
        "assertTrue" | "assertFalse" | "assertNull" | "assertNotNull" | "assertDoesNotThrow" => {
            (count > 1, false)
        }
        "fail" => (count > 0, false),
        "assertThrows" | "assertThat" => (count > 2, false),
        "assertSame" | "assertNotSame" => (count > 2, true),
        "assertEquals" | "assertNotEquals" | "assertArrayEquals" | "assertIterableEquals" => {
            (equality_has_message(&args), true)
        }
        _ => return None,
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
        shape: if comparison {
            AssertShape::TopEqNe
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

fn assert_statement(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let first = find_named_child(n, |child| child.kind() != "comment");
    let comparison = first.is_some_and(|node| {
        node.kind() == "binary_expression"
            && child_by_field(node, "operator")
                .is_some_and(|op| matches!(text(cx, op), "==" | "!="))
    });
    let shape = if comparison {
        AssertShape::TopEqNe
    } else if first.is_some_and(|node| node.kind() == "identifier") {
        AssertShape::BareName
    } else {
        AssertShape::Other
    };
    Assertion {
        pos: pos_of(n),
        has_message: count_named_child(n, |child| child.kind() != "comment") >= 2,
        comparison,
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    }
}

fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    match n.kind() {
        "assert_statement" => Some(assert_statement(cx, n)),
        "method_invocation" => method_assertion(cx, n),
        _ => None,
    }
}

fn static_globals<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let in_interface = cx
        .parent_of(n)
        .is_some_and(|parent| parent.kind() == "interface_body");
    if in_interface || !has_keyword(n, "static") || has_keyword(n, "final") {
        return;
    }
    for declarator in named_children(n) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = child_by_field(declarator, "name") {
            out.push((AmbientKind::GlobalMutable, text(cx, name), pos_of(name)));
        }
    }
}

fn ambient_write_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "assignment_expression" => {
            let left = child_by_field(n, "left")?;
            (left.kind() == "identifier").then_some(text(cx, left))
        }
        "update_expression" => {
            let operand = first_code(n)?;
            (operand.kind() == "identifier").then_some(text(cx, operand))
        }
        _ => None,
    }
}

fn static_field_named(cx: &Cx<'_, '_>, field: Node<'_>, name: &str) -> bool {
    if field.kind() != "field_declaration"
        || !has_keyword(field, "static")
        || has_keyword(field, "final")
    {
        return false;
    }
    named_children(field).into_iter().any(|decl| {
        decl.kind() == "variable_declarator"
            && child_by_field(decl, "name").is_some_and(|id| text(cx, id) == name)
    })
}

fn in_static_scope<'s>(cx: &Cx<'_, 's>, n: Node<'_>, name: &str) -> bool {
    let mut current = cx.parent_of(n);
    while let Some(node) = current {
        if matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return named_children(node).into_iter().any(|child| {
                static_field_named(cx, child, name)
                    || named_children(child)
                        .into_iter()
                        .any(|sub| static_field_named(cx, sub, name))
            });
        }
        current = cx.parent_of(node);
    }
    false
}
fn ambient_sites<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    match n.kind() {
        "field_declaration" => static_globals(cx, n, out),
        "method_invocation" => {
            if let Some(callee) = method_callee(cx, n).filter(|callee| *callee == "System.getenv") {
                out.push((AmbientKind::EnvRead, callee, pos_of(n)));
            }
        }
        "assignment_expression" | "update_expression" => {
            if let Some(name) = ambient_write_name(cx, n)
                && in_static_scope(cx, n, name)
            {
                out.push((AmbientKind::GlobalWrite, name, pos_of(n)));
            }
        }
        _ => {}
    }
}

impl LangHooks for JavaHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        function_head(cx, n)
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        is_else_if(n, parent)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        local_declared(cx, n, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        write_targets(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import_declaration" {
            out.extend(import_fact(cx, n));
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        typedef_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        switch_label_arm(cx, n)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        handler_site(cx, n)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        _in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        ambient_sites(cx, n, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        dynamic_site(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        assertion_site(cx, n)
    }
}
