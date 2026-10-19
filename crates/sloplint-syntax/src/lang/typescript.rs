use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    last_segment, named_children, narrow, pos_of, preceding_comments, string_arg_shape,
    strip_quotes, text,
};
use crate::spec::Role;

/// Role table for the TypeScript and TSX grammars.
pub(crate) static TABLE: crate::lang::Table = &[
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
    ("assignment_expression", Role::Write),
    ("augmented_assignment_expression", Role::Write),
    ("update_expression", Role::Write),
    ("import_statement", Role::Import),
    ("export_statement", Role::Export),
    ("class_declaration", Role::TypeDef),
    ("abstract_class_declaration", Role::TypeDef),
    ("interface_declaration", Role::TypeDef),
    ("type_alias_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("switch_default", Role::MatchArm),
    ("object", Role::StructLit),
    ("member_expression", Role::Member),
];

/// Hooks for the TypeScript and TSX grammars.
pub(crate) struct TypeScriptHooks;

/// Shared hook table for the TypeScript and TSX grammars.
pub(crate) static HOOKS: TypeScriptHooks = TypeScriptHooks;

fn enclosing_class(cx: &Cx<'_, '_>, member: Node<'_>) -> Option<(bool, bool)> {
    let body = member.parent().filter(|b| b.kind() == "class_body")?;
    let class = body.parent()?;
    if !matches!(
        class.kind(),
        "class_declaration" | "abstract_class_declaration"
    ) {
        return None;
    }
    cx.containers.iter().rev().find_map(|c| match c {
        Container::Class {
            exported,
            implements,
        } => Some((*exported, *implements)),
        _ => None,
    })
}

fn member_public(cx: &Cx<'_, '_>, n: Node<'_>, name_kind: &str) -> bool {
    if name_kind == "private_property_identifier" {
        return false;
    }
    if named_children(n).into_iter().any(|c| {
        c.kind() == "accessibility_modifier"
            && (text(cx, c) == "private" || text(cx, c) == "protected")
    }) {
        return false;
    }
    enclosing_class(cx, n).is_some_and(|(exported, _)| exported)
}

fn strip_ty(ty: &str) -> &str {
    ty.trim_start_matches([':', ' ', '\t'])
}

fn ts_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        if child.kind() != "required_parameter" && child.kind() != "optional_parameter" {
            continue;
        }
        let Some(pat) = child_by_field(child, "pattern") else {
            continue;
        };
        if pat.kind() == "this" {
            continue;
        }
        out.push(Param {
            name: text(cx, pat),
            ty: child_by_field(child, "type").map(|t| strip_ty(text(cx, t))),
            bool_default: false,
        });
    }
}

fn closure_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Param<'s>>) {
    if let Some(list) = child_by_field(n, "parameters") {
        ts_params(cx, list, out);
    } else if let Some(single) = child_by_field(n, "parameter") {
        out.push(Param {
            name: text(cx, single),
            ty: None,
            bool_default: false,
        });
    }
}

fn missing_of<'s>(params: &[Param<'s>], ret: Option<&'s str>, name: &str) -> Vec<&'s str> {
    let mut missing: Vec<&'s str> = params
        .iter()
        .filter(|p| p.ty.is_none())
        .map(|p| p.name)
        .collect();
    if ret.is_none() && name != "constructor" {
        missing.push("return");
    }
    missing
}

fn top_level(n: Node<'_>) -> bool {
    matches!(
        n.parent().map(|parent| parent.kind()),
        Some("program" | "export_statement")
    )
}

fn doc_anchor(n: Node<'_>) -> Node<'_> {
    let mut anchor = n;
    while let Some(p) = anchor.parent() {
        if p.kind() == "export_statement" || p.kind() == "lexical_declaration" {
            anchor = p;
        } else {
            break;
        }
    }
    anchor
}

fn ts_call_stmt(n: Node<'_>) -> Option<usize> {
    let inner = match n.kind() {
        "return_statement" | "expression_statement" => named_children(n).into_iter().next()?,
        _ => return None,
    };
    let mut cur = inner;
    loop {
        if cur.kind() == "call_expression" {
            return Some(cur.id());
        }
        if cur.kind() == "await_expression" {
            cur = named_children(cur).into_iter().next()?;
        } else {
            return None;
        }
    }
}

fn ts_sole_call(body: Node<'_>) -> Option<usize> {
    if body.kind() == "call_expression" {
        return Some(body.id());
    }
    if body.kind() != "statement_block" {
        return None;
    }
    let mut stmts = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment");
    let only = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    ts_call_stmt(only)
}

fn ts_pat<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                out.push((text(cx, cur), pos_of(cur)));
            }
            "object_pattern" | "array_pattern" | "rest_pattern" => {
                stack.extend(named_children(cur));
            }
            "pair_pattern" => {
                if let Some(v) = child_by_field(cur, "value") {
                    stack.push(v);
                }
            }
            "assignment_pattern" => {
                if let Some(l) = child_by_field(cur, "left") {
                    stack.push(l);
                }
            }
            _ => {}
        }
    }
}

fn ts_declared<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<(&'s str, Pos)> {
    let mut out = Vec::new();
    for d in named_children(n) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = child_by_field(d, "name") {
            ts_pat(cx, name, &mut out);
        }
    }
    out
}

fn ts_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" | "this" => return Some((text(cx, cur), pos_of(cur))),
            "member_expression" => cur = child_by_field(cur, "object")?,
            "subscript_expression" => cur = child_by_field(cur, "object")?,
            "parenthesized_expression" => cur = named_children(cur).into_iter().next()?,
            _ => return None,
        }
    }
}

fn ts_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_fragment"]),
        "template_string" => string_arg_shape(cx, n, &["string_fragment"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(cx, n, "+", &["string", "template_string"]) => ArgShape::Concat,
        _ => ArgShape::Other,
    }
}

fn ts_args<'s>(cx: &Cx<'_, 's>, args: Node<'_>) -> Vec<Arg<'s>> {
    named_children(args)
        .into_iter()
        .map(|c| Arg {
            shape: ts_shape(cx, c),
            start_line: pos_of(c).line,
            end_line: narrow(c.end_position().row) + 1,
        })
        .collect()
}

fn clause_imports<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    for c in named_children(clause) {
        match c.kind() {
            "identifier" => out.push(Import {
                pos: pos_of(c),
                module: Cow::Borrowed(module),
                name: text(cx, c),
                alias: None,
                kind: ImportKind::Named,
                in_test: cx.is_test_file,
            }),
            "named_imports" => import_specifiers(cx, c, module, out),
            "namespace_import" => {
                let name = named_children(c)
                    .into_iter()
                    .find(|g| g.kind() == "identifier")
                    .map_or("", |g| text(cx, g));
                out.push(Import {
                    pos: pos_of(c),
                    module: Cow::Borrowed(module),
                    name,
                    alias: None,
                    kind: ImportKind::Namespace,
                    in_test: cx.is_test_file,
                });
            }
            _ => {}
        }
    }
}

fn import_specifiers<'s>(
    cx: &Cx<'_, 's>,
    list: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    for spec in named_children(list) {
        if spec.kind() != "import_specifier" {
            continue;
        }
        let name = child_by_field(spec, "name").map_or("", |v| text(cx, v));
        let alias = child_by_field(spec, "alias").map_or("", |v| text(cx, v));
        out.push(Import {
            pos: pos_of(spec),
            module: Cow::Borrowed(module),
            name,
            alias: if alias.is_empty() || alias == name {
                None
            } else {
                Some(alias)
            },
            kind: ImportKind::Named,
            in_test: cx.is_test_file,
        });
    }
}
fn export_specs<'a, 's>(cx: &Cx<'_, 's>, clause: Node<'a>) -> Vec<(Node<'a>, &'s str, &'s str)> {
    let mut specs = Vec::new();
    for spec in named_children(clause) {
        if spec.kind() != "export_specifier" {
            continue;
        }
        let name = child_by_field(spec, "name").map_or("", |v| text(cx, v));
        let alias = child_by_field(spec, "alias").map_or("", |v| text(cx, v));
        specs.push((spec, name, alias));
    }
    specs
}
fn heritage_impls<'s>(cx: &Cx<'_, 's>, heritage: Node<'_>, out: &mut Vec<ImplRef<'s>>) -> bool {
    let mut found = false;
    for c in named_children(heritage) {
        if c.kind() == "implements_clause" {
            found = true;
            for t in named_children(c) {
                out.push(ts_impl(cx, t));
            }
        } else if c.kind() == "extends_clause" {
            for v in named_children(c) {
                if v.kind() == "type_arguments" {
                    continue;
                }
                out.push(ts_impl(cx, v));
            }
        }
    }
    found
}
fn ts_impl<'s>(cx: &Cx<'_, 's>, t: Node<'_>) -> ImplRef<'s> {
    let base = if t.kind() == "generic_type" {
        child_by_field(t, "name").map_or("", |m| text(cx, m))
    } else {
        text(cx, t)
    };
    ImplRef {
        name: last_segment(base, &[".", "::"]),
        pos: pos_of(t),
    }
}

fn catch_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    if s.kind() == "throw_statement" {
        return throw_of(cx, s, binding);
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    let Some(inner) = named_children(s).into_iter().next() else {
        return Stmt::Other;
    };
    if inner.kind() == "call_expression" {
        let callee = child_by_field(inner, "function").map_or("", |f| text(cx, f));
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    if inner.kind() == "throw_statement" {
        return throw_of(cx, inner, binding);
    }
    Stmt::Other
}

fn throw_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    let hit = named_children(s)
        .into_iter()
        .next()
        .is_some_and(|e| e.kind() == "identifier" && text(cx, e) == binding);
    if hit { Stmt::Rethrow } else { Stmt::Other }
}

fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

fn ts_targets(dir: &str, src: &str) -> Vec<String> {
    if !src.starts_with("./") && !src.starts_with("../") {
        return Vec::new();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let mut rest = src;
    while let Some(next) = rest.strip_prefix("../") {
        let _ = parts.pop();
        rest = next;
    }
    rest = rest.strip_prefix("./").map_or(rest, |s| s);
    let prefix = parts.join("/");
    let join = |base: String| {
        if prefix.is_empty() {
            base
        } else {
            prefix.clone() + "/" + base.as_str()
        }
    };
    let stem = rest
        .strip_suffix(".jsx")
        .or_else(|| rest.strip_suffix(".mjs"))
        .or_else(|| rest.strip_suffix(".js"));
    if let Some(stripped) = stem {
        return vec![
            join(stripped.to_owned() + ".ts"),
            join(stripped.to_owned() + ".tsx"),
        ];
    }
    vec![
        join(rest.to_owned() + ".ts"),
        join(rest.to_owned() + ".tsx"),
        join(rest.to_owned() + "/index.ts"),
        join(rest.to_owned() + "/index.tsx"),
    ]
}

fn import_stmt<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let module = child_by_field(n, "source").map_or("", |s| strip_quotes(text(cx, s)));
    let Some(clause) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "import_clause")
    else {
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(module),
            name: "",
            alias: None,
            kind: ImportKind::SideEffect,
            in_test: cx.is_test_file,
        });
        return;
    };
    clause_imports(cx, clause, module, out);
}
fn export_namespace<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) -> bool {
    let Some(ns) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "namespace_export")
    else {
        return false;
    };
    let name = named_children(ns)
        .into_iter()
        .find(|g| g.kind() == "identifier")
        .map_or("", |g| text(cx, g));
    out.push(Import {
        pos: pos_of(ns),
        module: Cow::Borrowed(module),
        name,
        alias: None,
        kind: ImportKind::Namespace,
        in_test: cx.is_test_file,
    });
    true
}
fn has_export_star(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    for i in 0..n.child_count() {
        if let Some(c) = n.child(i)
            && !c.is_named()
            && text(cx, c) == "*"
        {
            return true;
        }
    }
    false
}
fn push_named_exports<'a, 's>(
    cx: &Cx<'_, 's>,
    specs: Vec<(Node<'a>, &'s str, &'s str)>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    for (spec, name, alias) in specs {
        out.push(Import {
            pos: pos_of(spec),
            module: Cow::Borrowed(module),
            name,
            alias: if alias.is_empty() || alias == name {
                None
            } else {
                Some(alias)
            },
            kind: ImportKind::Named,
            in_test: cx.is_test_file,
        });
    }
}
fn export_stmt<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(src) = child_by_field(n, "source") else {
        return;
    };
    let module = strip_quotes(text(cx, src));
    if export_namespace(cx, n, module, out) {
        return;
    }
    let mut specs = Vec::new();
    if let Some(clause) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "export_clause")
    {
        specs = export_specs(cx, clause);
    }
    if specs.is_empty() {
        if has_export_star(cx, n) {
            out.push(Import {
                pos: pos_of(n),
                module: Cow::Borrowed(module),
                name: "*",
                alias: None,
                kind: ImportKind::Wildcard,
                in_test: cx.is_test_file,
            });
        }
        return;
    }
    push_named_exports(cx, specs, module, out);
}
impl LangHooks for TypeScriptHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let name_node = child_by_field(n, "name")?;
        let public = if n.kind() == "method_definition" {
            member_public(cx, n, name_node.kind())
        } else {
            n.parent().is_some_and(|p| p.kind() == "export_statement")
        };
        let mut params = Vec::new();
        if let Some(list) = child_by_field(n, "parameters") {
            ts_params(cx, list, &mut params);
        }
        let ret = child_by_field(n, "return_type").map(|t| text(cx, t));
        let name = text(cx, name_node);
        let missing = missing_of(&params, ret, name);
        let (doc, doc_pos) = preceding_comments(cx, doc_anchor(n), &["comment"], &["decorator"]);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public,
            interface_bound: enclosing_class(cx, n).is_some_and(|(_, implements)| implements),
            in_test: cx.is_test_file,
            params,
            return_type: ret,
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: ts_sole_call(body),
        })
    }

    fn closure_as_function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let parent = n.parent()?;
        if parent.kind() == "variable_declarator" {
            if child_by_field(parent, "value").is_some_and(|v| v.id() != n.id()) {
                return None;
            }
            let decl = parent.parent()?;
            let grand = decl.parent()?;
            if grand.kind() != "program" && grand.kind() != "export_statement" {
                return None;
            }
            let name_node = child_by_field(parent, "name")?;
            if name_node.kind() != "identifier" {
                return None;
            }
            let public = grand.kind() == "export_statement";
            let mut params = Vec::new();
            closure_params(cx, n, &mut params);
            let body = child_by_field(n, "body")?;
            let ret = child_by_field(n, "return_type").map(|t| text(cx, t));
            let name = text(cx, name_node);
            let missing = missing_of(&params, ret, name);
            let anchor = if decl
                .parent()
                .is_some_and(|p| p.kind() == "export_statement")
            {
                decl.parent().unwrap_or(decl)
            } else {
                decl
            };
            let (doc, doc_pos) = preceding_comments(cx, anchor, &["comment"], &["decorator"]);
            return Some(FunctionHead {
                name,
                name_pos: pos_of(name_node),
                public,
                interface_bound: false,
                in_test: cx.is_test_file,
                params,
                return_type: ret,
                missing_annotations: missing,
                doc,
                doc_pos,
                sole_call: ts_sole_call(body),
            });
        }
        if parent.kind() == "public_field_definition" {
            if child_by_field(parent, "value").is_some_and(|v| v.id() != n.id()) {
                return None;
            }
            let name_node = child_by_field(parent, "name")?;
            let mut params = Vec::new();
            closure_params(cx, n, &mut params);
            let body = child_by_field(n, "body")?;
            let ret = child_by_field(n, "return_type").map(|t| text(cx, t));
            let name = text(cx, name_node);
            let missing = missing_of(&params, ret, name);
            let (doc, doc_pos) =
                preceding_comments(cx, doc_anchor(parent), &["comment"], &["decorator"]);
            return Some(FunctionHead {
                name,
                name_pos: pos_of(name_node),
                public: member_public(cx, parent, name_node.kind()),
                interface_bound: enclosing_class(cx, parent)
                    .is_some_and(|(_, implements)| implements),
                in_test: cx.is_test_file,
                params,
                return_type: ret,
                missing_annotations: missing,
                doc,
                doc_pos,
                sole_call: ts_sole_call(body),
            });
        }
        None
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent().is_some_and(|p| p.kind() == "else_clause")
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "lexical_declaration"
            && !child_by_field(n, "kind").is_some_and(|k| text(cx, k) == "let")
        {
            return;
        }
        out.extend(ts_declared(cx, n));
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let target = if n.kind() == "update_expression" {
            child_by_field(n, "argument")
        } else {
            child_by_field(n, "left")
        };
        if let Some(t) = target
            && let Some(root) = ts_root(cx, t)
        {
            out.push(root);
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "new_expression" {
            let ctor = child_by_field(n, "constructor")?;
            let args = child_by_field(n, "arguments").map_or(Vec::new(), |a| ts_args(cx, a));
            return Some(CallHead {
                callee: text(cx, ctor),
                kind: CallKind::New,
                callee_root: None,
                args,
            });
        }
        let func = child_by_field(n, "function")?;
        let args = child_by_field(n, "arguments").map_or(Vec::new(), |a| ts_args(cx, a));
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: None,
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import_statement" {
            import_stmt(cx, n, out);
            return;
        }
        if n.kind() == "export_statement" {
            export_stmt(cx, n, out);
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        let public = n.parent().is_some_and(|p| p.kind() == "export_statement");
        let abstraction = kind == "interface_declaration" || kind == "abstract_class_declaration";
        let name_node = child_by_field(n, "name")?;
        let mut impls = Vec::new();
        let mut container = None;
        if kind == "class_declaration" || kind == "abstract_class_declaration" {
            let mut has_impl = false;
            if let Some(heritage) = named_children(n)
                .into_iter()
                .find(|c| c.kind() == "class_heritage")
            {
                has_impl = heritage_impls(cx, heritage, &mut impls);
            }
            container = Some(Container::Class {
                exported: public,
                implements: has_impl,
            });
        }
        Some(TypeHead {
            def: Some(TypeDef {
                name: text(cx, name_node),
                pos: pos_of(name_node),
                public,
                abstraction,
            }),
            impls,
            container,
        })
    }

    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let enum_like = n.parent().is_some_and(|p| {
            named_children(p)
                .into_iter()
                .filter(|c| c.kind() == "switch_case" && c.id() != n.id())
                .any(|c| {
                    child_by_field(c, "value").is_some_and(|v| v.kind() == "member_expression")
                })
        });
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like,
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let binding = child_by_field(n, "parameter").map_or("", |p| {
            if p.kind() == "identifier" {
                text(cx, p)
            } else {
                ""
            }
        });
        let body = child_by_field(n, "body")?;
        let mut stmts = Vec::new();
        for s in named_children(body) {
            if s.kind() == "comment" {
                continue;
            }
            stmts.push(catch_stmt(cx, s, binding));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "object" {
            return None;
        }
        let mut fields = 0u32;
        let mut source: Option<&'s str> = None;
        let mut poisoned = false;
        for child in named_children(n) {
            if child.kind() == "spread_element" {
                continue;
            }
            if child.kind() != "pair" {
                poisoned = true;
                continue;
            }
            fields += 1;
            let key = child_by_field(child, "key").map_or("", |k| text(cx, k));
            let cand = child_by_field(child, "value").and_then(|v| {
                if v.kind() != "member_expression" {
                    return None;
                }
                let obj = child_by_field(v, "object")?;
                if obj.kind() != "identifier" {
                    return None;
                }
                if child_by_field(v, "property").map_or("", |p| text(cx, p)) != key {
                    return None;
                }
                Some(text(cx, obj))
            });
            match (source, cand) {
                (None, Some(s)) => source = Some(s),
                (Some(a), Some(b)) if a == b => {}
                _ => poisoned = true,
            }
        }
        if fields == 0 {
            return None;
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields,
            source: if poisoned { None } else { source },
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "member_expression" {
            let obj = child_by_field(n, "object").map_or("", |o| text(cx, o));
            let prop = child_by_field(n, "property").map_or("", |p| text(cx, p));
            if prop == "env" && (obj == "process" || obj == "import.meta") {
                out.push((AmbientKind::EnvRead, text(cx, n), pos_of(n)));
            }
            return;
        }
        if in_function || !top_level(n) {
            return;
        }
        if n.kind() == "lexical_declaration" || n.kind() == "variable_declaration" {
            let mutable = n.kind() == "variable_declaration"
                || child_by_field(n, "kind").is_some_and(|k| text(cx, k) == "let");
            if mutable {
                for (name, pos) in ts_declared(cx, n) {
                    out.push((AmbientKind::GlobalMutable, name, pos));
                }
            }
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() == "new_expression" {
            if child_by_field(n, "constructor").map_or("", |c| text(cx, c)) == "Function" {
                return Some("Function constructor");
            }
            return None;
        }
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        if func.kind() == "subscript_expression" {
            let idx = child_by_field(func, "index").map_or("", |i| i.kind());
            if idx != "string" && idx != "number" {
                return Some("computed method call");
            }
            return None;
        }
        if text(cx, func) == "eval" {
            return Some("eval");
        }
        None
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
        if callee != "assert" && callee != "assert.ok" && callee != "console.assert" {
            return None;
        }
        let args = child_by_field(n, "arguments").map_or(Vec::new(), named_children);
        let comparison = args.first().is_some_and(|a| {
            a.kind() == "binary_expression"
                && child_by_field(*a, "operator").is_some_and(|o| {
                    matches!(
                        text(cx, o),
                        "==" | "===" | "!=" | "!==" | "<" | ">" | "<=" | ">="
                    )
                })
        });
        Some(Assertion {
            pos: pos_of(n),
            has_message: args.len() >= 2,
            comparison,
        })
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        if root.kind() != "program" {
            return None;
        }
        let mut sources = Vec::new();
        for child in named_children(root) {
            if child.kind() == "comment" {
                continue;
            }
            if child.kind() == "export_statement" && child_by_field(child, "source").is_some() {
                sources.push(
                    child_by_field(child, "source").map_or("", |s| strip_quotes(text(cx, s))),
                );
                continue;
            }
            return None;
        }
        if sources.is_empty() {
            return None;
        }
        let dir = parent_dir(cx.path);
        let mut targets = Vec::new();
        for src in sources {
            targets.push(ts_targets(dir, src));
        }
        Some(Barrel { targets })
    }
}
