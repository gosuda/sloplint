use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, Import, ImportKind, Param,
    Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, first_of_kind,
    named_children, narrow, pos_of, preceding_comments, slice, string_arg_shape, strip_quotes,
    text, trimmed,
};
use crate::spec::Role;

/// Role table mapping Odin node kinds to walker roles.
pub(crate) static TABLE: crate::lang::Table = &[
    ("procedure", Role::Closure),
    ("if_statement", Role::If),
    ("when_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("else_if_clause", Role::ElseIf),
    ("else_when_clause", Role::ElseIf),
    ("switch_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("or_return_expression", Role::Catch),
    ("ternary_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("var_declaration", Role::Decl),
    ("variable_declaration", Role::Decl),
    ("assignment_statement", Role::Bind),
    ("update_statement", Role::Write),
    ("import_declaration", Role::Import),
    ("struct_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("union_declaration", Role::TypeDef),
    ("attribute", Role::Module),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("switch_case", Role::MatchArm),
    ("struct", Role::StructLit),
];

/// Odin language hooks.
pub(crate) static HOOKS: OdinHooks = OdinHooks;

/// Odin adapter.
pub(crate) struct OdinHooks;

fn is_anon_text(cx: &Cx<'_, '_>, c: Node<'_>, want: &str) -> bool {
    !c.is_named() && text(cx, c) == want
}

fn has_operator(n: Node<'_>, cx: &Cx<'_, '_>, op: &str) -> bool {
    (0..n.child_count()).any(|i| n.child(i).is_some_and(|c| is_anon_text(cx, c, op)))
}

fn attr_has(cx: &Cx<'_, '_>, decl: Node<'_>, wanted: &str) -> bool {
    let Some(attrs) = child_by_field(decl, "attributes") else {
        return false;
    };
    let mut stack = vec![attrs];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" && text(cx, node) == wanted {
            return true;
        }
        for child in named_children(node) {
            stack.push(child);
        }
    }
    false
}

fn is_identifier(c: &Node<'_>) -> bool {
    c.kind() == "identifier"
}

fn names_before_sep<'a>(cx: &Cx<'_, '_>, parent: Node<'a>, seps: &[&str]) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for i in 0..parent.child_count() {
        let Some(child) = parent.child(i) else {
            continue;
        };
        if child.is_named() {
            if is_identifier(&child) {
                out.push(child);
            }
        } else if seps.iter().any(|sep| text(cx, child) == *sep) {
            break;
        }
    }
    out
}

fn odin_params<'s>(cx: &Cx<'_, 's>, holder: Option<Node<'_>>) -> (Vec<Param<'s>>, Vec<&'s str>) {
    let mut params = Vec::new();
    let mut missing = Vec::new();
    let Some(holder) = holder else {
        return (params, missing);
    };
    for child in named_children(holder) {
        if child.kind() == "parameter" {
            let ty = first_of_kind(child, &["type"]).map(|t| text(cx, t));
            for name_node in names_before_sep(cx, child, &[":"]) {
                let param = Param {
                    name: text(cx, name_node),
                    ty,
                    bool_default: false,
                };
                params.push(param);
            }
        } else if child.kind() == "default_parameter" {
            let name = named_children(child)
                .into_iter()
                .find(is_identifier)
                .map(|c| text(cx, c))
                .unwrap_or("");
            let value = named_children(child)
                .into_iter()
                .find(|c| !is_identifier(c))
                .map(|e| text(cx, e))
                .unwrap_or("");
            let defaulted = value == "true" || value == "false";
            params.push(Param {
                name,
                ty: None,
                bool_default: defaulted,
            });
            if !name.is_empty() {
                missing.push(name);
            }
        }
    }
    (params, missing)
}

fn odin_sole(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    if body.kind() != "block" {
        return None;
    }
    let live: Vec<Node<'_>> = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment")
        .collect();
    if live.len() != 1 {
        return None;
    }
    let mut node = live[0];
    if node.kind() == "statement" {
        node = named_children(node).first().copied()?;
    }
    if node.kind() == "return_statement" {
        node = named_children(node).first().copied()?;
    }
    (node.kind() == "call_expression").then_some(node.id())
}

fn odin_lhs(n: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    for i in 0..n.child_count() {
        let Some(child) = n.child(i) else { continue };
        if child.is_named() {
            out.push(child);
        } else {
            break;
        }
    }
    out
}

fn odin_write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        let kind = current.kind();
        if kind == "identifier" {
            return Some((text(cx, current), pos_of(current)));
        }
        let kids = named_children(current);
        if kind == "member_expression" && kids.len() == 2 {
            current = kids[0];
        } else if kind == "index_expression" {
            current = kids.first().copied()?;
        } else if kind == "parenthesized_expression" {
            current = child_by_field(current, "expression")?;
        } else {
            return None;
        }
    }
}

fn push_roots<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    for target in odin_lhs(n) {
        if let Some(root) = odin_write_root(cx, target) {
            out.push(root);
        }
    }
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    let shape = match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    };
    Arg {
        shape,
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
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

fn qualified_callee<'s>(cx: &Cx<'_, 's>, call: Node<'_>, func: Node<'_>) -> &'s str {
    if let Some(parent) = call.parent()
        && parent.kind() == "member_expression"
        && parent
            .child(parent.child_count().saturating_sub(1))
            .is_some_and(|last| last.id() == call.id())
    {
        return slice(cx.src, parent.start_byte(), func.end_byte()).trim();
    }
    trimmed(cx, func)
}

fn odin_callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        if current.kind() == "identifier" {
            return Some(text(cx, current));
        }
        current = named_children(current).first().copied()?;
    }
}

fn last_path_segment(path: &str) -> &str {
    let colon = path.rfind(':').map(|at| at + 1).unwrap_or(0);
    let slash = path.rfind('/').map(|at| at + 1).unwrap_or(0);
    &path[colon.max(slash).min(path.len())..]
}

fn odin_field_source<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> Option<&'s str> {
    let assigned =
        (0..field.child_count()).any(|i| field.child(i).is_some_and(|c| is_anon_text(cx, c, "=")));
    if !assigned {
        return None;
    }
    let kids = named_children(field);
    if kids.len() != 2 || kids[1].kind() != "member_expression" {
        return None;
    }
    let parts = named_children(kids[1]);
    if parts.len() != 2 || !is_identifier(&parts[0]) {
        return None;
    }
    if text(cx, parts[1]) != text(cx, kids[0]) {
        return None;
    }
    Some(text(cx, parts[0]))
}

fn odin_is_compare(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "binary_expression" {
        return false;
    }
    let equal = |c: Node<'_>| text(cx, c) == "==" || text(cx, c) == "!=";
    (0..n.child_count()).any(|i| n.child(i).is_some_and(equal))
}

fn is_implicit_selector(c: Node<'_>) -> bool {
    c.kind() == "member_expression" && named_children(c).len() == 1
}

fn sibling_enum(n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    named_children(parent)
        .into_iter()
        .filter(|s| s.kind() == "switch_case" && s.id() != n.id())
        .any(|s| child_by_field(s, "condition").is_some_and(is_implicit_selector))
}

fn is_test_ident(cx: &Cx<'_, '_>, c: Node<'_>) -> bool {
    c.kind() == "identifier" && text(cx, c) == "test"
}

fn typedef_at(name: &str, pos: Pos, public: bool) -> TypeDef<'_> {
    TypeDef {
        name,
        pos,
        public,
        abstraction: false,
    }
}

fn is_env_callee(callee: &str) -> bool {
    callee == "os.get_env"
        || callee == "os.lookup_env"
        || callee == "os2.get_env"
        || callee == "os2.lookup_env"
}

fn is_reflect_callee(callee: &str) -> bool {
    callee.contains(".struct_field_by_name") || callee.ends_with("reflect.enum_from_name")
}

fn is_formatted_assert(callee: &str) -> bool {
    callee == "fmt.assertf"
        || callee == "fmt.ensuref"
        || callee.ends_with(".assertf")
        || callee.ends_with(".ensuref")
}

fn is_plain_assert(callee: &str) -> bool {
    callee == "assert"
        || callee == "ensure"
        || callee.ends_with(".assert")
        || callee.ends_with(".ensure")
}

impl LangHooks for OdinHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let _ = (n, cx);
        None
    }

    fn closure_as_function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        if n.kind() != "procedure" {
            return None;
        }
        let parent = n.parent()?;
        let name_node = match parent.kind() {
            "procedure_declaration" | "const_declaration" | "variable_declaration" => {
                named_children(parent)
                    .into_iter()
                    .find(|c| matches!(c.kind(), "expression" | "identifier"))?
            }
            "assignment_statement" => {
                if !has_operator(parent, cx, ":=") {
                    return None;
                }
                let first = named_children(parent).first().copied()?;
                if !is_identifier(&first) {
                    return None;
                }
                first
            }
            _ => return None,
        };
        let holder = first_of_kind(n, &["parameters"]);
        let (params, missing_annotations) = odin_params(cx, holder);
        let return_type = first_of_kind(n, &["type", "named_type"]).map(|t| text(cx, t));
        let (doc, doc_pos) = preceding_comments(cx, parent, &["comment", "block_comment"], &[]);
        let top = parent.parent().is_some_and(|g| g.kind() == "source_file");
        let public = top && !attr_has(cx, parent, "private");
        let tested = cx.is_test_file || attr_has(cx, parent, "test");
        let body = first_of_kind(n, &["block"]);
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public,
            interface_bound: false,
            in_test: tested,
            params,
            return_type,
            missing_annotations,
            doc,
            doc_pos,
            sole_call: body.and_then(|b| odin_sole(cx, b)),
        })
    }

    fn is_else_if(&self, _n: Node<'_>) -> bool {
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "var_declaration" && n.kind() != "variable_declaration" {
            return;
        }
        for name_node in names_before_sep(cx, n, &[":", ":="]) {
            out.push((text(cx, name_node), pos_of(name_node)));
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "update_statement"
            || (n.kind() == "assignment_statement" && has_operator(n, cx, "="))
        {
            push_roots(cx, n, out);
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func =
            child_by_field(n, "function").or_else(|| named_children(n).into_iter().next())?;
        let args = positional_nodes(n, func)
            .into_iter()
            .map(|a| make_arg(cx, a))
            .collect();
        let callee = qualified_callee(cx, n, func);
        Some(CallHead {
            callee,
            kind: CallKind::Plain,
            callee_root: odin_callee_root(cx, func),
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() != "import_declaration" {
            return;
        }
        let pos = pos_of(n);
        let raw = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "string")
            .map(|s| text(cx, s))
            .unwrap_or("");
        let module = Cow::Borrowed(strip_quotes(raw));
        let alias = child_by_field(n, "alias").map(|a| text(cx, a));
        let foreign = (0..n.child_count())
            .any(|i| n.child(i).is_some_and(|c| is_anon_text(cx, c, "foreign")));
        if foreign {
            let side = Import {
                pos,
                module,
                name: "",
                alias: None,
                kind: ImportKind::SideEffect,
                in_test: false,
            };
            out.push(side);
            return;
        }
        let inner = strip_quotes(raw);
        let name = alias.unwrap_or_else(|| last_path_segment(inner));
        let namespaced = Import {
            pos,
            module,
            name,
            alias,
            kind: ImportKind::Namespace,
            in_test: false,
        };
        out.push(namespaced);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        match n.kind() {
            "struct_declaration" | "enum_declaration" | "union_declaration" => {}
            _ => return None,
        }
        let name_node = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "expression")?;
        let top = n.parent().is_some_and(|p| p.kind() == "source_file");
        let public = top && !attr_has(cx, n, "private");
        let def = typedef_at(text(cx, name_node), pos_of(name_node), public);
        Some(TypeHead {
            def: Some(def),
            impls: Vec::new(),
            container: None,
        })
    }

    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        if n.kind() != "attribute" {
            return None;
        }
        let tested = named_children(n).iter().any(|c| is_test_ident(cx, *c));
        tested.then_some(Container::TestModule)
    }

    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "switch_case" {
            return None;
        }
        if child_by_field(n, "condition").is_some() {
            return None;
        }
        let arm = WildcardArm {
            pos: pos_of(n),
            enum_like: sibling_enum(n),
        };
        Some(arm)
    }

    fn handler<'s>(&self, n: Node<'_>, _cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "or_return_expression" {
            return None;
        }
        let rethrow = Handler {
            pos: pos_of(n),
            stmts: vec![Stmt::Rethrow],
        };
        Some(rethrow)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "struct" {
            return None;
        }
        let fields: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() == "struct_field")
            .collect();
        if fields.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        let mut complete = true;
        for field in &fields {
            match odin_field_source(cx, *field) {
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
        if n.kind() == "var_declaration" || n.kind() == "variable_declaration" {
            if in_function {
                return;
            }
            let found = named_children(n).into_iter().find(is_identifier);
            if let Some(id) = found {
                let global = (AmbientKind::GlobalMutable, text(cx, id), pos_of(id));
                out.push(global);
            }
            return;
        }
        if n.kind() != "call_expression" {
            return;
        }
        let callee = child_by_field(n, "function")
            .or_else(|| named_children(n).into_iter().next())
            .map(|f| qualified_callee(cx, n, f))
            .unwrap_or("");
        if is_env_callee(callee) {
            let read = (AmbientKind::EnvRead, callee, pos_of(n));
            out.push(read);
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func =
            child_by_field(n, "function").or_else(|| named_children(n).into_iter().next())?;
        if !is_reflect_callee(qualified_callee(cx, n, func)) {
            return None;
        }
        let args = positional_nodes(n, func);
        if args.len() < 2 {
            return None;
        }
        (args[1].kind() != "string").then_some("runtime name lookup (reflect)")
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func =
            child_by_field(n, "function").or_else(|| named_children(n).into_iter().next())?;
        let callee = qualified_callee(cx, n, func);
        if is_formatted_assert(callee) {
            let messaged = Assertion {
                pos: pos_of(n),
                has_message: true,
                comparison: false,
            };
            return Some(messaged);
        }
        if !is_plain_assert(callee) {
            return None;
        }
        let args = positional_nodes(n, func);
        let first = args.first().copied();
        let comparison = first.is_some_and(|a| odin_is_compare(cx, a));
        let arity = args.len() >= 2;
        Some(Assertion {
            pos: pos_of(n),
            has_message: arity,
            comparison,
        })
    }
}
