use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, Handler, ImplRef, Import, ImportKind, Param,
    Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    child_by_field, count_named_child, find_named_child, first_named_child_matching, first_of_kind,
    has_child_kind, last_segment, named_children, narrow, only_named_child_matching, pos_of,
    preceding_comments, rfind_named_child, slice, string_arg_shape, text,
};
use crate::spec::Role;

/// Node kinds of the Java grammar and the walker role each one plays.
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

/// The Java adapter for `.java` files.
pub(crate) static HOOKS: JavaHooks = JavaHooks;

/// Hooks for the Java grammar.
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
    NUMERIC_KINDS.contains(&n.kind())
        || (n.kind() == "unary_expression"
            && child_by_field(n, "operator").is_some_and(|op| op.kind() == "-")
            && child_by_field(n, "operand").is_some_and(numeric_literal))
}

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
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
        bool_default: false,
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
                bool_default: false,
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
    let in_trait = matches!(
        cx.innermost_container(),
        Some(Container::Trait { public: true })
    );
    let public = has_keyword(n, "public") || (in_trait && !has_keyword(n, "private"));
    let in_test = cx.is_test_file || annotations.iter().any(|a| TEST_ANNOTATIONS.contains(a));
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENTS, &[]);
    Some(FunctionHead {
        name: text(cx, name),
        name_pos: pos_of(name),
        public,
        interface_bound: annotations.contains(&"Override"),
        in_test,
        params: child_by_field(n, "parameters")
            .map_or_else(Vec::new, |list| formal_params(cx, list)),
        return_type: child_by_field(n, "type").map(|ty| text(cx, ty)),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_of(n, body),
    })
}

fn type_name(n: Node<'_>) -> Option<Node<'_>> {
    match n.kind() {
        "type_identifier" => Some(n),
        "generic_type" => {
            first_named_child_matching(n, &["type_identifier", "scoped_type_identifier"])
                .and_then(type_name)
        }
        "scoped_type_identifier" => rfind_named_child(n, |child| child.kind() == "type_identifier"),
        _ => None,
    }
}

fn push_type_ref<'s>(cx: &Cx<'_, 's>, ty: Node<'_>, refs: &mut Vec<ImplRef<'s>>) {
    if let Some(name) = type_name(ty) {
        refs.push(ImplRef {
            name: text(cx, name),
            pos: pos_of(name),
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
    let public = has_keyword(n, "public");
    let is_interface = n.kind() == "interface_declaration";
    let abstraction =
        is_interface || (n.kind() == "class_declaration" && has_keyword(n, "abstract"));
    let container = if is_interface {
        Container::Trait { public }
    } else {
        Container::Class {
            exported: public,
            implements: false,
        }
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name),
            pos: pos_of(name),
            public,
            abstraction,
        }),
        impls: supertype_refs(cx, n),
        container: Some(container),
    })
}

fn import_fact<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Import<'s>> {
    let path = first_of_kind(n, &["scoped_identifier", "identifier"])?;
    let pos = pos_of(n);
    let in_test = cx.is_test_file;
    if has_child_kind(n, "asterisk") {
        return Some(Import {
            pos,
            module: Cow::Borrowed(text(cx, path)),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            in_test,
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
        in_test,
    })
}

fn receiver_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "identifier" | "this" | "super" => Some(text(cx, n)),
        "field_access" | "method_invocation" => receiver_root(cx, child_by_field(n, "object")?),
        _ => None,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => {
            string_arg_shape(cx, n, &["string_fragment", "multiline_string_fragment"])
        }
        "binary_expression" if binary_concat(cx, n, "+", &["string_literal"]) => ArgShape::Concat,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arguments_of(n)
        .into_iter()
        .map(|arg| span_arg(arg, arg_shape(cx, arg)))
        .collect()
}

fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "method_invocation" => {
            let name = child_by_field(n, "name")?;
            let callee_root = match child_by_field(n, "object") {
                Some(object) => receiver_root(cx, object),
                None => Some(text(cx, name)),
            };
            Some(CallHead {
                callee: method_callee(cx, n)?,
                kind: CallKind::Plain,
                callee_root,
                args: call_args(cx, n),
            })
        }
        "object_creation_expression" => {
            let ty = child_by_field(n, "type")?;
            Some(CallHead {
                callee: text(cx, ty),
                kind: CallKind::New,
                callee_root: None,
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
    match n.kind() {
        "identifier" | "this" => Some((text(cx, n), pos_of(n))),
        "field_access" => write_root(cx, child_by_field(n, "object")?),
        "array_access" => write_root(cx, child_by_field(n, "array")?),
        _ => None,
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
    })
}

fn sibling_labels(n: Node<'_>) -> Vec<Node<'_>> {
    let Some(group) = n.parent() else {
        return Vec::new();
    };
    let Some(block) = group.parent() else {
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
    let enum_like = sibling_labels(n).into_iter().any(label_is_enum);
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like,
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
    })
}

fn assert_statement(cx: &Cx<'_, '_>, n: Node<'_>) -> Assertion {
    let comparison = find_named_child(n, |child| child.kind() != "comment").is_some_and(|first| {
        first.kind() == "binary_expression"
            && child_by_field(first, "operator")
                .is_some_and(|op| matches!(text(cx, op), "==" | "!="))
    });
    Assertion {
        pos: pos_of(n),
        has_message: count_named_child(n, |child| child.kind() != "comment") >= 2,
        comparison,
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
    let in_interface = n
        .parent()
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

fn ambient_sites<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    match n.kind() {
        "field_declaration" => static_globals(cx, n, out),
        "method_invocation" => {
            if let Some(callee) = method_callee(cx, n).filter(|callee| *callee == "System.getenv") {
                out.push((AmbientKind::EnvRead, callee, pos_of(n)));
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
