use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_named_child, binary_concat,
    child_by_field, count_named_child, find_named_child, first_of_kind, has_child_kind,
    named_children, narrow, only_named_child_matching, pos_of, preceding_comments, sole_call_id,
    string_arg_shape, strip_quotes, text,
};
use crate::spec::Role;

/// Node kinds of the JavaScript grammar and the walker role each one plays.
pub(crate) static TABLE: &[(&str, Role)] = &[
    ("function_declaration", Role::Function),
    ("generator_function_declaration", Role::Function),
    ("method_definition", Role::Function),
    ("arrow_function", Role::Closure),
    ("function_expression", Role::Closure),
    ("generator_function", Role::Closure),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("switch_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("for_in_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("ternary_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("new_expression", Role::NewCall),
    ("lexical_declaration", Role::Decl),
    ("variable_declaration", Role::Decl),
    // sloplint: `using` bindings are immutable; decl_names already tracks only `let`/`var`.
    ("using_declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("augmented_assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("import_statement", Role::Import),
    ("export_statement", Role::Export),
    ("class_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("switch_default", Role::MatchArm),
    ("object", Role::StructLit),
    ("member_expression", Role::Member),
];

/// The JavaScript adapter; one grammar parses `.js`, `.jsx`, `.mjs`, and `.cjs`.
pub(crate) static HOOKS: JavaScriptHooks = JavaScriptHooks;

/// Hooks for the JavaScript grammar.
pub(crate) struct JavaScriptHooks;

const COMMENTS: &[&str] = &["comment"];
const PLAIN_ASSERTS: &[&str] = &["assert", "assert.ok", "console.assert"];
const EQUALITY_ASSERTS: &[&str] = &[
    "assert.equal",
    "assert.notEqual",
    "assert.strictEqual",
    "assert.notStrictEqual",
    "assert.deepEqual",
    "assert.deepStrictEqual",
    "assert.notDeepEqual",
    "assert.notDeepStrictEqual",
];
const COMPARISON_OPS: &[&str] = &["==", "===", "!=", "!==", "<", ">", "<=", ">="];
const JS_EXTENSIONS: &[&str] = &[".js", ".jsx", ".mjs", ".cjs"];

fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| child.kind() != "comment")
        .collect()
}

fn first_code(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |child| child.kind() != "comment")
}

fn parent_kind_is(n: Node<'_>, kind: &str) -> bool {
    n.parent().is_some_and(|parent| parent.kind() == kind)
}

fn header_of(n: Node<'_>) -> Node<'_> {
    match n.parent() {
        Some(parent) if parent.kind() == "export_statement" => parent,
        _ => n,
    }
}

fn declared_public(cx: &Cx<'_, '_>, decl: Node<'_>, name: Node<'_>) -> bool {
    parent_kind_is(decl, "export_statement")
        || (parent_kind_is(decl, "program") && cx.is_exported(text(cx, name)))
}

fn exported_class_member(cx: &Cx<'_, '_>, member: Node<'_>) -> bool {
    let class = member
        .parent()
        .filter(|body| body.kind() == "class_body")
        .and_then(|body| body.parent())
        .filter(|class| class.kind() == "class_declaration");
    class.is_some_and(|class| {
        child_by_field(class, "name").is_some_and(|name| declared_public(cx, class, name))
    })
}

fn top_level(n: Node<'_>) -> bool {
    matches!(
        n.parent().map(|parent| parent.kind()),
        Some("program" | "export_statement")
    )
}

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
    }
}

fn formal_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Param<'s>> {
    code_children(list)
        .into_iter()
        .filter_map(|param| formal_param(cx, param))
        .collect()
}

fn formal_param<'s>(cx: &Cx<'_, 's>, param: Node<'_>) -> Option<Param<'s>> {
    match param.kind() {
        "identifier" | "object_pattern" | "array_pattern" => Some(Param {
            name: text(cx, param),
            ty: None,
            bool_default: false,
        }),
        "assignment_pattern" => {
            let left = child_by_field(param, "left")?;
            let right = child_by_field(param, "right")?;
            Some(Param {
                name: text(cx, left),
                ty: None,
                bool_default: matches!(right.kind(), "true" | "false"),
            })
        }
        "rest_pattern" => {
            let name = first_code(param).map_or_else(|| text(cx, param), |inner| text(cx, inner));
            Some(Param {
                name,
                ty: None,
                bool_default: false,
            })
        }
        _ => None,
    }
}

struct ClosureSite<'a> {
    name: Node<'a>,
    public: bool,
    header: Node<'a>,
}

fn closure_site<'a>(
    cx: &Cx<'_, '_>,
    n: Node<'a>,
    parent: Option<Node<'a>>,
) -> Option<ClosureSite<'a>> {
    let parent = parent?;
    match parent.kind() {
        "variable_declarator" => declarator_site(cx, n, parent),
        "field_definition" => field_site(cx, n, parent),
        _ => None,
    }
}

fn declarator_site<'a>(
    cx: &Cx<'_, '_>,
    n: Node<'a>,
    declarator: Node<'a>,
) -> Option<ClosureSite<'a>> {
    if child_by_field(declarator, "value")?.id() != n.id() {
        return None;
    }
    let name = child_by_field(declarator, "name").filter(|name| name.kind() == "identifier")?;
    let declaration = declarator.parent()?;
    if !matches!(
        declaration.kind(),
        "lexical_declaration" | "variable_declaration"
    ) {
        return None;
    }
    let scope = declaration.parent()?;
    let wrapped = match scope.kind() {
        "export_statement" => true,
        "program" => false,
        _ => return None,
    };
    let public = wrapped || cx.is_exported(text(cx, name));
    let header = if wrapped { scope } else { declaration };
    Some(ClosureSite {
        name,
        public,
        header,
    })
}

fn field_site<'a>(cx: &Cx<'_, '_>, n: Node<'a>, field: Node<'a>) -> Option<ClosureSite<'a>> {
    if child_by_field(field, "value")?.id() != n.id() {
        return None;
    }
    let name = child_by_field(field, "property")?;
    let public = exported_class_member(cx, field) && name.kind() != "private_property_identifier";
    Some(ClosureSite {
        name,
        public,
        header: field,
    })
}

struct ImportSite<'s> {
    pos: Pos,
    module: &'s str,
    in_test: bool,
}

impl<'s> ImportSite<'s> {
    fn fact(&self, name: &'s str, alias: Option<&'s str>, kind: ImportKind) -> Import<'s> {
        Import {
            pos: self.pos,
            module: Cow::Borrowed(self.module),
            name,
            alias,
            kind,
            in_test: self.in_test,
        }
    }
}

fn closure_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    if let Some(single) = child_by_field(n, "parameter") {
        return vec![Param {
            name: text(cx, single),
            ty: None,
            bool_default: false,
        }];
    }
    child_by_field(n, "parameters").map_or_else(Vec::new, |list| formal_params(cx, list))
}

fn sole_call_of(body: Node<'_>) -> Option<usize> {
    sole_call_id(
        body,
        |block| only_named_child_matching(block, |c| c.kind() != "comment"),
        statement_call,
    )
}

fn statement_call(statement: Node<'_>) -> Option<usize> {
    if !matches!(
        statement.kind(),
        "return_statement" | "expression_statement"
    ) {
        return None;
    }
    let mut expression = first_code(statement)?;
    if expression.kind() == "await_expression" {
        expression = first_code(expression)?;
    }
    (expression.kind() == "call_expression").then(|| expression.id())
}

struct HeadSite<'a> {
    name: Node<'a>,
    header: Node<'a>,
    public: bool,
    body: Node<'a>,
}

fn head_for<'s>(cx: &Cx<'_, 's>, site: HeadSite<'_>, params: Vec<Param<'s>>) -> FunctionHead<'s> {
    let (doc, doc_pos) = preceding_comments(cx, site.header, COMMENTS, &[]);
    FunctionHead {
        name: text(cx, site.name),
        name_pos: pos_of(site.name),
        public: site.public,
        interface_bound: false,
        in_test: cx.is_test_file,
        params,
        return_type: None,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_of(site.body),
    }
}

fn root_of_callee(n: Node<'_>) -> Option<Node<'_>> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "super" => return Some(current),
            "member_expression" | "subscript_expression" => child_by_field(current, "object")?,
            "parenthesized_expression" => first_code(current)?,
            _ => return None,
        };
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" | "template_string" => string_arg_shape(cx, n, &["string_fragment"]),
        "binary_expression" if binary_concat(cx, n, "+", &["string", "template_string"]) => {
            ArgShape::Concat
        }
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, arguments: Node<'_>) -> Vec<Arg<'s>> {
    if arguments.kind() == "template_string" {
        return vec![span_arg(arguments, arg_shape(cx, arguments))];
    }
    code_children(arguments)
        .into_iter()
        .map(|arg| span_arg(arg, arg_shape(cx, arg)))
        .collect()
}

fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "call_expression" => {
            let function = child_by_field(n, "function")?;
            let args = child_by_field(n, "arguments").map_or_else(Vec::new, |a| call_args(cx, a));
            Some(CallHead {
                callee: text(cx, function),
                kind: CallKind::Plain,
                callee_root: root_of_callee(function).map(|root| text(cx, root)),
                args,
            })
        }
        "new_expression" => {
            let constructor = child_by_field(n, "constructor")?;
            let args = child_by_field(n, "arguments").map_or_else(Vec::new, |a| call_args(cx, a));
            Some(CallHead {
                callee: text(cx, constructor),
                kind: CallKind::New,
                callee_root: None,
                args,
            })
        }
        _ => None,
    }
}

fn declared_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    for declarator in named_children(n) {
        let Some(name) = child_by_field(declarator, "name") else {
            continue;
        };
        if declarator.kind() == "variable_declarator" {
            collect_pattern(cx, name, out);
        }
    }
}

enum PatternFrame<'t> {
    Pattern(Node<'t>),
    Member(Node<'t>),
}

fn collect_pattern<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending = vec![PatternFrame::Pattern(n)];
    while let Some(frame) = pending.pop() {
        match frame {
            PatternFrame::Pattern(node) => expand_pattern(cx, node, out, &mut pending),
            PatternFrame::Member(node) => expand_member(cx, node, out, &mut pending),
        }
    }
}

fn expand_pattern<'s, 't>(
    cx: &Cx<'_, 's>,
    n: Node<'t>,
    out: &mut Vec<(&'s str, Pos)>,
    pending: &mut Vec<PatternFrame<'t>>,
) {
    match n.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => {
            out.push((text(cx, n), pos_of(n)));
        }
        "object_pattern" => {
            pending.extend(
                named_children(n)
                    .into_iter()
                    .rev()
                    .map(PatternFrame::Member),
            );
        }
        "array_pattern" => {
            pending.extend(
                named_children(n)
                    .into_iter()
                    .rev()
                    .map(PatternFrame::Pattern),
            );
        }
        "rest_pattern" => pending.extend(first_code(n).map(PatternFrame::Pattern)),
        "assignment_pattern" => {
            pending.extend(child_by_field(n, "left").map(PatternFrame::Pattern));
        }
        _ => {}
    }
}

fn expand_member<'s, 't>(
    cx: &Cx<'_, 's>,
    n: Node<'t>,
    out: &mut Vec<(&'s str, Pos)>,
    pending: &mut Vec<PatternFrame<'t>>,
) {
    match n.kind() {
        "shorthand_property_identifier_pattern" => out.push((text(cx, n), pos_of(n))),
        "pair_pattern" => pending.extend(child_by_field(n, "value").map(PatternFrame::Pattern)),
        "object_assignment_pattern" => {
            pending.extend(child_by_field(n, "left").map(PatternFrame::Pattern));
        }
        "rest_pattern" => pending.extend(first_code(n).map(PatternFrame::Pattern)),
        _ => {}
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some((text(cx, current), pos_of(current))),
            "member_expression" | "subscript_expression" => child_by_field(current, "object")?,
            "parenthesized_expression" => first_code(current)?,
            _ => return None,
        };
    }
}

fn pair_copy<'s>(cx: &Cx<'_, 's>, pair: Node<'_>) -> Option<&'s str> {
    let key = child_by_field(pair, "key").filter(|key| key.kind() == "property_identifier")?;
    let value =
        child_by_field(pair, "value").filter(|value| value.kind() == "member_expression")?;
    let object = child_by_field(value, "object").filter(|object| object.kind() == "identifier")?;
    let property = child_by_field(value, "property")?;
    (text(cx, property) == text(cx, key)).then(|| text(cx, object))
}

fn object_source<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut found: Option<&'s str> = None;
    for child in named_children(n) {
        match child.kind() {
            "spread_element" | "comment" => continue,
            "pair" => {}
            _ => return None,
        }
        let object = pair_copy(cx, child)?;
        if found.is_some_and(|known| known != object) {
            return None;
        }
        found = Some(object);
    }
    found
}

fn local_export_names<'s>(cx: &Cx<'_, 's>, statement: Node<'_>, out: &mut Vec<&'s str>) {
    for part in code_children(statement) {
        match part.kind() {
            "identifier" => out.push(text(cx, part)),
            "export_clause" => {
                for spec in code_children(part) {
                    out.extend(child_by_field(spec, "name").map(|name| text(cx, name)));
                }
            }
            _ => {}
        }
    }
}

fn import_module<'s>(cx: &Cx<'_, 's>, statement: Node<'_>) -> Option<&'s str> {
    let source = child_by_field(statement, "source")?;
    Some(strip_quotes(text(cx, source)))
}

fn named_import_facts<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    for child in code_children(clause) {
        match child.kind() {
            "identifier" => out.push(site.fact(text(cx, child), None, ImportKind::Named)),
            "namespace_import" => {
                if let Some(id) = first_code(child) {
                    out.push(site.fact(text(cx, id), None, ImportKind::Namespace));
                }
            }
            "named_imports" => {
                for spec in code_children(child) {
                    named_specifier_fact(cx, spec, site, out);
                }
            }
            _ => {}
        }
    }
}

fn named_specifier_fact<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    let Some(name) = child_by_field(spec, "name") else {
        return;
    };
    let alias = child_by_field(spec, "alias").map(|alias| text(cx, alias));
    out.push(site.fact(text(cx, name), alias, ImportKind::Named));
}

fn export_facts<'s>(
    cx: &Cx<'_, 's>,
    statement: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    if let Some(clause) = find_named_child(statement, |child| {
        child.kind() != "comment" && child.kind() == "export_clause"
    }) {
        for spec in code_children(clause) {
            named_specifier_fact(cx, spec, site, out);
        }
        return;
    }
    if let Some(namespace) = find_named_child(statement, |child| {
        child.kind() != "comment" && child.kind() == "namespace_export"
    }) {
        let name = first_code(namespace).map_or("*", |id| text(cx, id));
        out.push(site.fact(name, None, ImportKind::Namespace));
        return;
    }
    if has_child_kind(statement, "*") {
        out.push(site.fact("*", None, ImportKind::Wildcard));
    }
}

fn import_facts<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(module) = import_module(cx, n) else {
        return;
    };
    let site = ImportSite {
        pos: pos_of(n),
        module,
        in_test: cx.is_test_file,
    };
    match n.kind() {
        "import_statement" => {
            let clauses: Vec<Node<'_>> = code_children(n)
                .into_iter()
                .filter(|child| child.kind() == "import_clause")
                .collect();
            if clauses.is_empty() {
                out.push(site.fact("", None, ImportKind::SideEffect));
            }
            for clause in clauses {
                named_import_facts(cx, clause, &site, out);
            }
        }
        "export_statement" => export_facts(cx, n, &site, out),
        _ => {}
    }
}

fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if n.kind() != "class_declaration" {
        return None;
    }
    let name = child_by_field(n, "name")?;
    let public = parent_kind_is(n, "export_statement");
    let impls: Vec<ImplRef<'s>> = first_of_kind(n, &["class_heritage"])
        .and_then(first_code)
        .and_then(|base| heritage_ref(cx, base))
        .into_iter()
        .collect();
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name),
            pos: pos_of(name),
            public,
            abstraction: false,
        }),
        impls,
        container: Some(Container::Class {
            exported: public,
            implements: false,
        }),
    })
}

fn heritage_ref<'s>(cx: &Cx<'_, 's>, base: Node<'_>) -> Option<ImplRef<'s>> {
    let name = match base.kind() {
        "identifier" => base,
        "member_expression" => child_by_field(base, "property")?,
        _ => return None,
    };
    Some(ImplRef {
        name: text(cx, name),
        pos: pos_of(name),
    })
}

fn declaration_globals<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function || !top_level(n) {
        return;
    }
    if n.kind() == "lexical_declaration"
        && child_by_field(n, "kind").map(|kind| text(cx, kind)) != Some("let")
    {
        return;
    }
    let mut names = Vec::new();
    declared_names(cx, n, &mut names);
    out.extend(
        names
            .into_iter()
            .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
    );
}

fn env_read<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let (Some(object), Some(property)) =
        (child_by_field(n, "object"), child_by_field(n, "property"))
    else {
        return;
    };
    if text(cx, property) != "env" {
        return;
    }
    let is_env = match object.kind() {
        "identifier" => text(cx, object) == "process",
        "meta_property" => text(cx, object) == "import.meta",
        _ => false,
    };
    if is_env {
        out.push((AmbientKind::EnvRead, text(cx, n), pos_of(n)));
    }
}

fn ambient_sites<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "lexical_declaration" | "variable_declaration" => {
            declaration_globals(cx, n, in_function, out);
        }
        "member_expression" => env_read(cx, n, out),
        _ => {}
    }
}

fn call_dynamic(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    let function = child_by_field(n, "function")?;
    match function.kind() {
        "subscript_expression" => {
            let index = child_by_field(function, "index")?;
            (!matches!(index.kind(), "string" | "number")).then_some("computed method call")
        }
        "identifier" => (text(cx, function) == "eval").then_some("eval"),
        "import" => {
            let first = child_by_field(n, "arguments").and_then(first_code)?;
            (first.kind() != "string").then_some("dynamic import")
        }
        _ => None,
    }
}

fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    match n.kind() {
        "call_expression" => call_dynamic(cx, n),
        "new_expression" => {
            let constructor = child_by_field(n, "constructor")?;
            (text(cx, constructor) == "Function").then_some("Function constructor")
        }
        _ => None,
    }
}

fn comparison_arg(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "binary_expression"
        && child_by_field(n, "operator")
            .is_some_and(|operator| COMPARISON_OPS.contains(&text(cx, operator)))
}

fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = text(cx, child_by_field(n, "function")?);
    let args: Vec<Node<'_>> = child_by_field(n, "arguments")
        .filter(|arguments| arguments.kind() == "arguments")
        .map(code_children)
        .unwrap_or_default();
    let pos = pos_of(n);
    if PLAIN_ASSERTS.contains(&callee) {
        let comparison = args.first().is_some_and(|first| comparison_arg(cx, *first));
        return Some(Assertion {
            pos,
            has_message: args.len() >= 2,
            comparison,
        });
    }
    EQUALITY_ASSERTS.contains(&callee).then_some(Assertion {
        pos,
        has_message: args.len() >= 3,
        comparison: true,
    })
}

fn join_normalized(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in dir.split('/').chain(rel.split('/')) {
        match segment {
            "" | "." => {}
            ".." => parts.pop().map(|_| ())?,
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn re_export_candidates(dir: &str, source: &str) -> Vec<String> {
    if !(source.starts_with("./") || source.starts_with("../")) {
        return Vec::new();
    }
    let Some(path) = join_normalized(dir, source) else {
        return Vec::new();
    };
    if JS_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
        return vec![path];
    }
    [
        ".js",
        ".jsx",
        ".mjs",
        ".cjs",
        "/index.js",
        "/index.jsx",
        ".ts",
        ".tsx",
        "/index.ts",
        "/index.tsx",
    ]
    .iter()
    .map(|suffix| format!("{path}{suffix}"))
    .collect()
}

fn barrel_of(root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
    if root.kind() != "program" {
        return None;
    }
    let dir = cx.path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut targets = Vec::new();
    for statement in code_children(root) {
        if statement.kind() == "hash_bang_line" {
            continue;
        }
        if statement.kind() != "export_statement" {
            return None;
        }
        let source = import_module(cx, statement)?;
        targets.push(re_export_candidates(dir, source));
    }
    (!targets.is_empty()).then_some(Barrel { targets })
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, binding: Option<&str>) -> Stmt<'s> {
    let Some(inner) = first_code(stmt) else {
        return Stmt::Other;
    };
    match stmt.kind() {
        "expression_statement" if inner.kind() == "call_expression" => {
            match child_by_field(inner, "function") {
                Some(function) => Stmt::Call {
                    callee: text(cx, function),
                    kind: CallKind::Plain,
                },
                None => Stmt::Other,
            }
        }
        "throw_statement" if inner.kind() == "identifier" && binding == Some(text(cx, inner)) => {
            Stmt::Rethrow
        }
        _ => Stmt::Other,
    }
}

fn handler_site<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() != "catch_clause" {
        return None;
    }
    let binding = child_by_field(n, "parameter")
        .filter(|parameter| parameter.kind() == "identifier")
        .map(|parameter| text(cx, parameter));
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

fn switch_default_arm(n: Node<'_>) -> WildcardArm {
    let enum_like = n.parent().is_some_and(|body| {
        any_named_child(body, |sibling| {
            sibling.kind() != "comment"
                && sibling.kind() == "switch_case"
                && child_by_field(sibling, "value")
                    .is_some_and(|value| value.kind() == "member_expression")
        })
    });
    WildcardArm {
        pos: pos_of(n),
        enum_like,
    }
}

impl LangHooks for JavaScriptHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let name = child_by_field(n, "name")?;
        let public = if n.kind() == "method_definition" {
            exported_class_member(cx, n) && name.kind() != "private_property_identifier"
        } else {
            declared_public(cx, n, name)
        };
        let params =
            child_by_field(n, "parameters").map_or_else(Vec::new, |list| formal_params(cx, list));
        let site = HeadSite {
            name,
            header: header_of(n),
            public,
            body,
        };
        Some(head_for(cx, site, params))
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        let found = closure_site(cx, n, parent)?;
        let body = child_by_field(n, "body")?;
        let site = HeadSite {
            name: found.name,
            header: found.header,
            public: found.public,
            body,
        };
        Some(head_for(cx, site, closure_params(cx, n)))
    }

    fn is_else_if(&self, _n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|parent| parent.kind() == "else_clause")
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let tracked = match n.kind() {
            "lexical_declaration" => {
                child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "let")
            }
            "variable_declaration" => true,
            _ => false,
        };
        if tracked {
            declared_names(cx, n, out);
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let target = match n.kind() {
            "assignment_expression" | "augmented_assignment_expression" => {
                child_by_field(n, "left")
            }
            "update_expression" => child_by_field(n, "argument"),
            _ => None,
        };
        if let Some(root) = target.and_then(|target| write_root(cx, target)) {
            out.push(root);
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        import_facts(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        typedef_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        (n.kind() == "switch_default").then(|| switch_default_arm(n))
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        handler_site(cx, n)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "object" {
            return None;
        }
        let fields = count_named_child(n, |child| child.kind() == "pair");
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(fields),
            source: object_source(cx, n),
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        ambient_sites(cx, n, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        dynamic_site(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        assertion_site(cx, n)
    }

    fn exported_names<'s>(&self, root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        for statement in code_children(root) {
            if statement.kind() == "export_statement" && import_module(cx, statement).is_none() {
                local_export_names(cx, statement, out);
            }
        }
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        barrel_of(root, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::sole_call_of;

    fn sole_call_in_js(source: &str) -> Option<usize> {
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
        parser
            .set_language(&language)
            .expect("javascript grammar loads");
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let function = tree.root_node().child(0).expect("one top-level function");
        let body = function.child_by_field_name("body").expect("function body");
        sole_call_of(body)
    }

    #[test]
    fn leading_comment_before_the_returned_call_is_skipped() {
        assert!(
            sole_call_in_js("function f() { return /* c */ g(); }").is_some(),
            "a leading comment before the returned call must not hide it"
        );
    }
}
