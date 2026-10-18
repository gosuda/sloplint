use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    first_of_kind, has_child_kind, last_segment, named_children, narrow, pos_of,
    preceding_comments, string_arg_shape, text,
};
use crate::spec::Role;

/// Node kinds of the Kotlin grammar and the walker role each one plays.
pub(crate) static TABLE: &[(&str, Role)] = &[
    ("function_declaration", Role::Function),
    ("secondary_constructor", Role::Function),
    ("lambda_literal", Role::Closure),
    ("anonymous_function", Role::Closure),
    ("if_expression", Role::If),
    ("when_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_while_statement", Role::Loop),
    ("catch_block", Role::Catch),
    ("call_expression", Role::Call),
    ("property_declaration", Role::Decl),
    ("assignment", Role::Write),
    ("unary_expression", Role::Write),
    ("import", Role::Import),
    ("class_declaration", Role::TypeDef),
    ("object_declaration", Role::TypeDef),
    ("type_alias", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("when_entry", Role::MatchArm),
];

/// The Kotlin adapter for `.kt` files.
pub(crate) static HOOKS: KotlinHooks = KotlinHooks;

/// Hooks for the Kotlin grammar.
pub(crate) struct KotlinHooks;

const COMMENTS: &[&str] = &["line_comment", "block_comment"];
const TEST_ANNOTATIONS: &[&str] = &[
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];
const RESTRICTED: &[&str] = &["private", "protected", "internal"];
const TYPE_KINDS: &[&str] = &[
    "user_type",
    "nullable_type",
    "function_type",
    "non_nullable_type",
    "parenthesized_type",
    "dynamic",
];
const REFLECTION: &[&str] = &[
    "getMethod",
    "getDeclaredMethod",
    "getField",
    "getDeclaredField",
    "getConstructor",
    "getDeclaredConstructor",
];

fn children_of(n: Node<'_>) -> Vec<Node<'_>> {
    (0..n.child_count()).filter_map(|i| n.child(i)).collect()
}

fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| child.kind() != "line_comment" && child.kind() != "block_comment")
        .collect()
}

fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    code_children(n).into_iter().next()
}

fn modifiers_of(n: Node<'_>) -> Option<Node<'_>> {
    first_of_kind(n, &["modifiers"])
}

fn is_restricted(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    modifiers_of(n).is_some_and(|modifiers| {
        named_children(modifiers).into_iter().any(|child| {
            child.kind() == "visibility_modifier" && RESTRICTED.contains(&text(cx, child))
        })
    })
}

fn has_modifier_word(cx: &Cx<'_, '_>, n: Node<'_>, word: &str) -> bool {
    modifiers_of(n).is_some_and(|modifiers| {
        children_of(modifiers)
            .into_iter()
            .any(|child| text(cx, child) == word)
    })
}

fn annotation_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<&'s str> {
    let Some(modifiers) = modifiers_of(n) else {
        return Vec::new();
    };
    named_children(modifiers)
        .into_iter()
        .filter(|child| child.kind() == "annotation")
        .map(|child| {
            let bare = text(cx, child).trim_start_matches('@');
            last_segment(bare.split('(').next().unwrap_or(""), &["."])
        })
        .collect()
}

fn type_after_colon(n: Node<'_>) -> Option<Node<'_>> {
    let mut seen_colon = false;
    for child in children_of(n) {
        if child.kind() == ":" {
            seen_colon = true;
        } else if seen_colon && TYPE_KINDS.contains(&child.kind()) {
            return Some(child);
        }
    }
    None
}

fn value_args(n: Node<'_>) -> Vec<Node<'_>> {
    first_of_kind(n, &["value_arguments"]).map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|child| child.kind() == "value_argument")
            .collect()
    })
}

fn is_named_arg(arg: Node<'_>) -> bool {
    has_child_kind(arg, "=")
}

fn arg_value(arg: Node<'_>) -> Option<Node<'_>> {
    named_children(arg).into_iter().last()
}

fn callee_node(n: Node<'_>) -> Option<Node<'_>> {
    code_children(n).into_iter().find(|child| {
        !matches!(
            child.kind(),
            "value_arguments" | "annotated_lambda" | "type_arguments"
        )
    })
}

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
    }
}

fn is_comment(n: Node<'_>) -> bool {
    matches!(n.kind(), "line_comment" | "block_comment")
}

fn next_code_sibling(n: Node<'_>) -> Option<Node<'_>> {
    let mut next = n.next_named_sibling();
    while let Some(sibling) = next {
        if !is_comment(sibling) {
            return Some(sibling);
        }
        next = sibling.next_named_sibling();
    }
    None
}

fn function_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let Some(list) = first_of_kind(n, &["function_value_parameters"]) else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .filter(|child| child.kind() == "parameter")
        .filter_map(|parameter| {
            let name = text(cx, first_of_kind(parameter, &["identifier"])?);
            let bool_default = next_code_sibling(parameter).is_some_and(|default| {
                default.kind() == "identifier" && matches!(text(cx, default), "true" | "false")
            });
            Some(Param {
                name,
                ty: type_after_colon(parameter).map(|ty| text(cx, ty)),
                bool_default,
            })
        })
        .collect()
}

fn statement_call(statement: Node<'_>) -> Option<usize> {
    match statement.kind() {
        "call_expression" => Some(statement.id()),
        "return_expression" => code_children(statement)
            .last()
            .filter(|inner| inner.kind() == "call_expression")
            .map(|inner| inner.id()),
        _ => None,
    }
}

fn block_call(block: Node<'_>) -> Option<usize> {
    match code_children(block).as_slice() {
        [only] => statement_call(*only),
        _ => None,
    }
}

fn sole_call_of(n: Node<'_>) -> Option<usize> {
    if n.kind() == "secondary_constructor" {
        return first_of_kind(n, &["block"]).and_then(block_call);
    }
    let body = first_of_kind(n, &["function_body"])?;
    let inner = first_code(body)?;
    if inner.kind() == "block" {
        block_call(inner)
    } else {
        (inner.kind() == "call_expression").then(|| inner.id())
    }
}

fn container_public(cx: &Cx<'_, '_>) -> bool {
    match cx.containers.last() {
        None => true,
        Some(&Container::Class { exported, .. }) => exported,
        Some(&Container::Trait { public }) => public,
        Some(_) => false,
    }
}

fn expression_body(n: Node<'_>) -> bool {
    first_of_kind(n, &["function_body"])
        .and_then(first_code)
        .is_some_and(|inner| inner.kind() != "block")
}

fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let (name, name_pos) = match n.kind() {
        "function_declaration" => {
            first_of_kind(n, &["function_body"])?;
            let name = child_by_field(n, "name")?;
            (text(cx, name), pos_of(name))
        }
        "secondary_constructor" => {
            first_of_kind(n, &["block"])?;
            ("constructor", pos_of(n))
        }
        _ => return None,
    };
    let is_declaration = n.kind() == "function_declaration";
    let return_type = if is_declaration {
        type_after_colon(n).map(|ty| text(cx, ty))
    } else {
        None
    };
    let missing_annotations = if is_declaration && return_type.is_none() && expression_body(n) {
        vec!["return"]
    } else {
        Vec::new()
    };
    let annotations = annotation_names(cx, n);
    let in_test = cx.is_test_file || annotations.iter().any(|a| TEST_ANNOTATIONS.contains(a));
    let public = !is_restricted(cx, n) && container_public(cx);
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENTS, &[]);
    Some(FunctionHead {
        name,
        name_pos,
        public,
        interface_bound: has_modifier_word(cx, n, "override"),
        in_test,
        params: function_params(cx, n),
        return_type,
        missing_annotations,
        doc,
        doc_pos,
        sole_call: sole_call_of(n),
    })
}

fn delegate_ref<'s>(cx: &Cx<'_, 's>, spec: Node<'_>) -> Option<ImplRef<'s>> {
    let user = first_of_kind(spec, &["constructor_invocation"])
        .and_then(|invocation| first_of_kind(invocation, &["user_type"]))
        .or_else(|| first_of_kind(spec, &["user_type"]))?;
    let last = named_children(user)
        .into_iter()
        .rfind(|child| child.kind() == "identifier")?;
    Some(ImplRef {
        name: text(cx, last),
        pos: pos_of(last),
    })
}

fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    let name = match n.kind() {
        "type_alias" => child_by_field(n, "type")?,
        "class_declaration" | "object_declaration" => child_by_field(n, "name")?,
        _ => return None,
    };
    let public = !is_restricted(cx, n);
    let is_class = n.kind() == "class_declaration";
    let is_interface = is_class && has_child_kind(n, "interface");
    let abstraction = is_class && (is_interface || has_modifier_word(cx, n, "abstract"));
    let container = match n.kind() {
        "type_alias" => None,
        _ if is_interface => Some(Container::Trait { public }),
        _ => Some(Container::Class {
            exported: public,
            implements: false,
        }),
    };
    let impls: Vec<ImplRef<'s>> =
        first_of_kind(n, &["delegation_specifiers"]).map_or_else(Vec::new, |list| {
            named_children(list)
                .into_iter()
                .filter(|child| child.kind() == "delegation_specifier")
                .filter_map(|spec| delegate_ref(cx, spec))
                .collect()
        });
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name),
            pos: pos_of(name),
            public,
            abstraction,
        }),
        impls,
        container,
    })
}

fn import_fact<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Import<'s>> {
    let pos = pos_of(n);
    let in_test = cx.is_test_file;
    let qualified = first_of_kind(n, &["qualified_identifier"]);
    let identifiers: Vec<Node<'_>> = named_children(n)
        .into_iter()
        .filter(|child| child.kind() == "identifier")
        .collect();
    let (path, alias) = match qualified {
        Some(qualified) => (
            text(cx, qualified),
            identifiers.first().map(|id| text(cx, *id)),
        ),
        None => {
            let first = identifiers.first()?;
            (text(cx, *first), identifiers.get(1).map(|id| text(cx, *id)))
        }
    };
    if has_child_kind(n, "*") {
        return Some(Import {
            pos,
            module: Cow::Borrowed(path),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            in_test,
        });
    }
    let (module, name) = path.rsplit_once('.').unwrap_or(("", path));
    Some(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias,
        kind: ImportKind::Named,
        in_test,
    })
}

fn variable_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    for child in named_children(n) {
        let variables = match child.kind() {
            "variable_declaration" => vec![child],
            "multi_variable_declaration" => named_children(child)
                .into_iter()
                .filter(|inner| inner.kind() == "variable_declaration")
                .collect(),
            _ => continue,
        };
        for variable in variables {
            if let Some(id) = first_of_kind(variable, &["identifier"]) {
                out.push((text(cx, id), pos_of(id)));
            }
        }
    }
}

fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "navigation_expression" => callee_root(cx, first_code(n)?),
        "identifier" => Some(text(cx, n)),
        "this_expression" => Some("this"),
        "super_expression" => Some("super"),
        _ => None,
    }
}

fn value_shape<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "string_literal" | "multiline_string_literal" => {
            string_arg_shape(cx, value, &["string_content"])
        }
        "binary_expression" if binary_concat(cx, value, "+", &["string_literal"]) => {
            ArgShape::Concat
        }
        "identifier" => {
            let name = text(cx, value);
            if matches!(name, "true" | "false") {
                ArgShape::Other
            } else {
                ArgShape::Ident(name)
            }
        }
        _ => ArgShape::Other,
    }
}

fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = callee_node(n)?;
    let mut args: Vec<Arg<'s>> = value_args(n)
        .into_iter()
        .filter_map(|arg| {
            let value = arg_value(arg)?;
            Some(span_arg(arg, value_shape(cx, value)))
        })
        .collect();
    if let Some(lambda) = first_of_kind(n, &["annotated_lambda"]) {
        args.push(span_arg(lambda, ArgShape::Other));
    }
    Some(CallHead {
        callee: text(cx, callee),
        kind: CallKind::Plain,
        callee_root: callee_root(cx, callee),
        args,
    })
}

fn copy_source<'s>(cx: &Cx<'_, 's>, args: &[Node<'_>]) -> Option<&'s str> {
    let mut found: Option<&'s str> = None;
    for arg in args {
        let label = text(cx, first_of_kind(*arg, &["identifier"])?);
        let value = arg_value(*arg)?;
        if value.kind() != "navigation_expression" {
            return None;
        }
        let object = text(cx, first_code(value).filter(|o| o.kind() == "identifier")?);
        let member = named_children(value)
            .into_iter()
            .rfind(|child| child.kind() == "identifier")?;
        if text(cx, member) != label || found.is_some_and(|known| known != object) {
            return None;
        }
        found = Some(object);
    }
    found
}

fn struct_copy<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let args = value_args(n);
    let named: Vec<Node<'_>> = args
        .iter()
        .copied()
        .filter(|arg| is_named_arg(*arg))
        .collect();
    if named.is_empty() {
        return None;
    }
    let source = if named.len() == args.len() {
        copy_source(cx, &named)
    } else {
        None
    };
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(named.len()),
        source,
    })
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    match stmt.kind() {
        "call_expression" => match callee_node(stmt) {
            Some(callee) => Stmt::Call {
                callee: text(cx, callee),
                kind: CallKind::Plain,
            },
            None => Stmt::Other,
        },
        "throw_expression" => {
            let rethrows = first_code(stmt).is_some_and(|inner| {
                inner.kind() == "identifier" && binding == Some(text(cx, inner))
            });
            if rethrows { Stmt::Rethrow } else { Stmt::Other }
        }
        _ => Stmt::Other,
    }
}

fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_block" {
        return None;
    }
    let binding = first_of_kind(n, &["identifier"]).map(|id| text(cx, id));
    let body = first_of_kind(n, &["block"])?;
    let stmts = code_children(body)
        .into_iter()
        .map(|stmt| handler_stmt(cx, stmt, binding))
        .collect();
    Some(Handler {
        pos: pos_of(n),
        stmts,
    })
}

fn enum_condition(cx: &Cx<'_, '_>, condition: Node<'_>) -> bool {
    match condition.kind() {
        "navigation_expression" | "type_test" => true,
        "identifier" => !matches!(text(cx, condition), "true" | "false"),
        _ => false,
    }
}

fn when_arm(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "when_entry" || child_by_field(n, "condition").is_some() {
        return None;
    }
    let parent = n.parent()?;
    let has_subject = first_of_kind(parent, &["when_subject"]).is_some();
    let enum_like = has_subject
        && named_children(parent)
            .into_iter()
            .filter(|entry| entry.kind() == "when_entry")
            .any(|entry| {
                child_by_field(entry, "condition").is_some_and(|cond| enum_condition(cx, cond))
            });
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like,
    })
}

fn previous_code_sibling(n: Node<'_>) -> Option<Node<'_>> {
    let mut previous = n.prev_sibling();
    while let Some(sibling) = previous {
        if !matches!(sibling.kind(), ";" | "line_comment" | "block_comment") {
            return Some(sibling);
        }
        previous = sibling.prev_sibling();
    }
    None
}

fn is_else_if(n: Node<'_>) -> bool {
    n.kind() == "if_expression"
        && n.parent()
            .is_some_and(|parent| parent.kind() == "if_expression")
        && previous_code_sibling(n).is_some_and(|sibling| sibling.kind() == "else")
}

fn global_scope(n: Node<'_>) -> bool {
    match n.parent() {
        Some(parent) if parent.kind() == "source_file" => true,
        Some(body) if body.kind() == "class_body" => body
            .parent()
            .is_some_and(|owner| matches!(owner.kind(), "object_declaration" | "companion_object")),
        _ => false,
    }
}

fn ambient_sites<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    match n.kind() {
        "property_declaration" if global_scope(n) && has_child_kind(n, "var") => {
            let mut names = Vec::new();
            variable_names(cx, n, &mut names);
            out.extend(
                names
                    .into_iter()
                    .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
            );
        }
        "call_expression" => {
            let callee = callee_node(n).map(|callee| text(cx, callee));
            if let Some(callee) = callee.filter(|callee| *callee == "System.getenv") {
                out.push((AmbientKind::EnvRead, callee, pos_of(n)));
            }
        }
        _ => {}
    }
}

fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = text(cx, callee_node(n)?);
    let last = last_segment(callee, &["."]);
    let on_class = callee
        .rsplit_once('.')
        .is_some_and(|(qualifier, _)| last_segment(qualifier, &["."]) == "Class");
    let reflective = REFLECTION.contains(&last) || (last == "forName" && on_class);
    if !reflective {
        return None;
    }
    let first = arg_value(value_args(n).into_iter().next()?)?;
    (!matches!(first.kind(), "string_literal" | "multiline_string_literal"))
        .then_some("reflection by name")
}

fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = text(cx, callee_node(n)?);
    let count = value_args(n).len() + usize::from(has_child_kind(n, "annotated_lambda"));
    let (has_message, comparison) = match last_segment(callee, &["."]) {
        "assertTrue" | "assertFalse" | "assertNull" | "assertNotNull" => (count > 1, false),
        "assertEquals" | "assertNotEquals" | "assertSame" | "assertNotSame" => (count > 2, true),
        "fail" => (count > 0, false),
        _ => return None,
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message,
        comparison,
    })
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    match n.kind() {
        "identifier" | "this_expression" => Some((text(cx, n), pos_of(n))),
        "navigation_expression" | "index_expression" | "parenthesized_expression" => {
            write_root(cx, first_code(n)?)
        }
        _ => None,
    }
}

fn write_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let target = match n.kind() {
        "assignment" => child_by_field(n, "left"),
        "unary_expression"
            if child_by_field(n, "operator")
                .is_some_and(|op| matches!(text(cx, op), "++" | "--")) =>
        {
            child_by_field(n, "argument")
        }
        _ => None,
    };
    out.extend(target.and_then(|target| write_root(cx, target)));
}

impl LangHooks for KotlinHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        function_head(cx, n)
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        is_else_if(n)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "property_declaration" && has_child_kind(n, "var") {
            variable_names(cx, n, out);
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        write_targets(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import" {
            out.extend(import_fact(cx, n));
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        typedef_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        when_arm(cx, n)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        handler_site(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        struct_copy(cx, n)
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
