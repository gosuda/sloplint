use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    last_segment, named_children, narrow, pos_of, preceding_comments, string_arg_shape, text,
};
use crate::spec::Role;

/// Role table for the Python grammar.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("lambda", Role::Closure),
    ("if_statement", Role::If),
    ("elif_clause", Role::ElseIf),
    ("match_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("except_clause", Role::Catch),
    ("with_statement", Role::With),
    ("conditional_expression", Role::Ternary),
    ("call", Role::Call),
    ("assignment", Role::Bind),
    ("augmented_assignment", Role::Write),
    ("import_statement", Role::Import),
    ("import_from_statement", Role::Import),
    ("class_definition", Role::TypeDef),
    ("comment", Role::Comment),
    ("case_clause", Role::MatchArm),
    ("subscript", Role::Subscript),
    ("attribute", Role::Member),
    ("assert_statement", Role::Assert),
    ("global_statement", Role::Global),
];

/// Hooks for the Python grammar.
pub(crate) struct PythonHooks;

/// Shared hook table for the Python grammar.
pub(crate) static HOOKS: PythonHooks = PythonHooks;

fn enclosing_exported(cx: &Cx<'_, '_>) -> bool {
    cx.containers
        .iter()
        .rev()
        .find_map(|c| match c {
            Container::Class { exported, .. } => Some(*exported),
            _ => None,
        })
        .unwrap_or(true)
}

fn py_one_param<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Param<'s>>) {
    match n.kind() {
        "identifier" => out.push(Param {
            name: text(cx, n),
            ty: None,
            bool_default: false,
        }),
        "typed_parameter" => {
            let name = named_children(n)
                .into_iter()
                .find(|c| c.kind() == "identifier")
                .map_or("", |c| text(cx, c));
            out.push(Param {
                name,
                ty: child_by_field(n, "type").map(|t| text(cx, t)),
                bool_default: false,
            });
        }
        "default_parameter" => {
            let name = child_by_field(n, "name").map_or("", |v| text(cx, v));
            let def = child_by_field(n, "value").is_some_and(|v| {
                let t = text(cx, v);
                t == "true" || t == "false"
            });
            out.push(Param {
                name,
                ty: None,
                bool_default: def,
            });
        }
        "typed_default_parameter" => {
            let name = child_by_field(n, "name").map_or("", |v| text(cx, v));
            let def = child_by_field(n, "value").is_some_and(|v| {
                let t = text(cx, v);
                t == "true" || t == "false"
            });
            out.push(Param {
                name,
                ty: child_by_field(n, "type").map(|t| text(cx, t)),
                bool_default: def,
            });
        }
        "list_splat_pattern" | "dictionary_splat_pattern" => {
            let name = named_children(n)
                .into_iter()
                .find(|c| c.kind() == "identifier")
                .map_or("", |c| text(cx, c));
            out.push(Param {
                name,
                ty: None,
                bool_default: false,
            });
        }
        _ => {}
    }
}

fn py_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        if child.kind() == "parameter" {
            for sub in named_children(child) {
                py_one_param(cx, sub, out);
            }
        } else {
            py_one_param(cx, child, out);
        }
    }
}

fn docstring(n: Node<'_>) -> Option<Node<'_>> {
    let body = child_by_field(n, "body")?;
    let first = named_children(body).into_iter().next()?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let kids = named_children(first);
    if kids.len() != 1 || kids.first().is_some_and(|c| c.kind() != "string") {
        return None;
    }
    kids.into_iter().next()
}

fn is_doc_expr(n: Node<'_>) -> bool {
    if n.kind() != "expression_statement" {
        return false;
    }
    let kids = named_children(n);
    kids.len() == 1 && kids.first().is_some_and(|c| c.kind() == "string")
}

fn py_single_call(n: Node<'_>) -> Option<usize> {
    match n.kind() {
        "expression_statement" | "return_statement" => {
            let inner = named_children(n).into_iter().next()?;
            (inner.kind() == "call").then_some(inner.id())
        }
        _ => None,
    }
}

fn py_sole(body: Node<'_>) -> Option<usize> {
    let mut stmts = named_children(body).into_iter();
    let first = stmts.next()?;
    let target = if is_doc_expr(first) {
        stmts.next()?
    } else {
        first
    };
    if stmts.next().is_some() {
        return None;
    }
    py_single_call(target)
}

fn py_pat_names<'s>(cx: &Cx<'_, 's>, left: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![left];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" => out.push((text(cx, cur), pos_of(cur))),
            "pattern_list" | "tuple_pattern" | "list_pattern" => {
                stack.extend(named_children(cur));
            }
            _ => {}
        }
    }
}

fn py_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some((text(cx, cur), pos_of(cur))),
            "attribute" => cur = child_by_field(cur, "object")?,
            "subscript" => cur = child_by_field(cur, "value")?,
            "parenthesized_expression" => cur = named_children(cur).into_iter().next()?,
            _ => return None,
        }
    }
}

fn py_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "concatenated_string" => ArgShape::Concat,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(cx, n, "+", &["string"]) => ArgShape::Concat,
        _ => ArgShape::Other,
    }
}

fn py_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: py_shape(cx, n),
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn plain_import<'s>(
    cx: &Cx<'_, 's>,
    imp: (Node<'_>, &'s str, &'s str, Option<&'s str>),
    out: &mut Vec<Import<'s>>,
) {
    let (node, module, name, alias) = imp;
    out.push(Import {
        pos: pos_of(node),
        module: Cow::Borrowed(module),
        name,
        alias,
        kind: ImportKind::Named,
        in_test: cx.is_test_file,
    });
}

fn strip_as(value: &str) -> &str {
    let rest = value.trim_start();
    if rest.len() > 2 && rest.starts_with("as") && rest[2..].starts_with(char::is_whitespace) {
        rest[2..].trim_start()
    } else {
        value
    }
}

fn py_raise<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    let mut kids = named_children(s).into_iter();
    let Some(first) = kids.next() else {
        return Stmt::Rethrow;
    };
    if kids.next().is_some() {
        return Stmt::Other;
    }
    if first.kind() == "identifier" && text(cx, first) == binding {
        Stmt::Rethrow
    } else {
        Stmt::Other
    }
}

fn raise_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    if s.kind() == "raise_statement" {
        return py_raise(cx, s, binding);
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    let Some(inner) = named_children(s).into_iter().next() else {
        return Stmt::Other;
    };
    if inner.kind() == "call" {
        let callee = child_by_field(inner, "function").map_or("", |f| text(cx, f));
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    if inner.kind() == "raise_statement" {
        return py_raise(cx, inner, binding);
    }
    Stmt::Other
}

fn is_meta_abc(cx: &Cx<'_, '_>, base: Node<'_>) -> bool {
    let meta = child_by_field(base, "name").map_or("", |v| text(cx, v));
    let val = child_by_field(base, "value").map_or("", |v| text(cx, v));
    meta == "metaclass" && last_segment(val, &["."]) == "ABCMeta"
}
fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

fn join_path(base: &str, rest: String) -> String {
    if base.is_empty() {
        rest
    } else {
        base.to_owned() + "/" + rest.as_str()
    }
}

fn py_targets(cx: &Cx<'_, '_>, dir: &str, n: Node<'_>) -> Vec<String> {
    let module = child_by_field(n, "module_name").map_or("", |m| text(cx, m));
    let dots = module.bytes().take_while(|b| *b == b'.').count();
    if dots == 0 {
        return Vec::new();
    }
    let rest = module[dots..].replace('.', "/");
    let mut base = dir.to_owned();
    for _ in 1..dots {
        match base.rfind('/') {
            Some(i) => base.truncate(i),
            None => base.clear(),
        }
    }
    if rest.is_empty() {
        let mod_id = child_by_field(n, "module_name").map(|m| m.id());
        let mut out = Vec::new();
        for c in named_children(n) {
            if Some(c.id()) == mod_id || c.kind() == "wildcard_import" {
                continue;
            }
            let nm = if c.kind() == "aliased_import" {
                child_by_field(c, "name").map_or("", |v| text(cx, v))
            } else {
                text(cx, c)
            };
            let seg = last_segment(nm, &["."]);
            out.push(join_path(&base, seg.to_owned() + ".py"));
            out.push(join_path(&base, seg.to_owned() + "/__init__.py"));
        }
        return out;
    }
    vec![
        join_path(&base, rest.clone() + ".py"),
        join_path(&base, rest + "/__init__.py"),
    ]
}

impl LangHooks for PythonHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let method = matches!(cx.containers.last(), Some(Container::Class { .. }));
        let dunder = name.len() > 4 && name.starts_with("__") && name.ends_with("__");
        let public = dunder || (!name.starts_with('_') && (!method || enclosing_exported(cx)));
        let mut params = Vec::new();
        if let Some(list) = child_by_field(n, "parameters") {
            py_params(cx, list, &mut params);
        }
        if method
            && params
                .first()
                .is_some_and(|p| p.name == "self" || p.name == "cls")
        {
            params.remove(0);
        }
        let ret = child_by_field(n, "return_type").map(|t| text(cx, t));
        let mut missing: Vec<&'s str> = params
            .iter()
            .filter(|p| p.ty.is_none())
            .map(|p| p.name)
            .collect();
        if ret.is_none() && name != "__init__" {
            missing.push("return");
        }
        let anchor = n
            .parent()
            .filter(|p| p.kind() == "decorated_definition")
            .unwrap_or(n);
        let (mut doc, mut doc_pos) = preceding_comments(cx, anchor, &["comment"], &[]);
        if let Some(ds) = docstring(n) {
            if doc_pos.is_none() {
                doc_pos = Some(pos_of(ds));
            }
            doc.push(text(cx, ds));
        }
        let body = child_by_field(n, "body")?;
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public,
            interface_bound: dunder,
            in_test: cx.is_test_file && name.starts_with("test"),
            params,
            return_type: ret,
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: py_sole(body),
        })
    }

    fn is_else_if(&self, _: Node<'_>) -> bool {
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment" {
            return;
        }
        if let Some(left) = child_by_field(n, "left") {
            py_pat_names(cx, left, out);
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment" && n.kind() != "augmented_assignment" {
            return;
        }
        let Some(left) = child_by_field(n, "left") else {
            return;
        };
        if n.kind() == "augmented_assignment" || left.kind() != "identifier" {
            let plain = left.kind() == "identifier" || left.kind() == "attribute";
            if plain || left.kind() == "subscript" {
                if let Some(root) = py_root(cx, left) {
                    out.push(root);
                }
            } else if n.kind() == "augmented_assignment"
                && let Some(root) = py_root(cx, left)
            {
                out.push(root);
            }
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "call" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let mut args = Vec::new();
        if let Some(list) = child_by_field(n, "arguments") {
            for c in named_children(list) {
                if c.kind() == "keyword_argument" {
                    if let Some(v) = child_by_field(c, "value") {
                        args.push(py_arg(cx, v));
                    }
                } else {
                    args.push(py_arg(cx, c));
                }
            }
        }
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: None,
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() == "import_statement" {
            for c in named_children(n) {
                if c.kind() == "dotted_name" {
                    let full = text(cx, c);
                    plain_import(cx, (c, full, last_segment(full, &["."]), None), out);
                } else if c.kind() == "aliased_import" {
                    let path = child_by_field(c, "name").map_or("", |v| text(cx, v));
                    let alias = child_by_field(c, "alias").map_or("", |v| text(cx, v));
                    let named = (c, path, last_segment(path, &["."]), Some(alias));
                    plain_import(cx, named, out);
                }
            }
            return;
        }
        if n.kind() == "import_from_statement" {
            let module = child_by_field(n, "module_name").map_or("", |m| text(cx, m));
            let mod_id = child_by_field(n, "module_name").map(|m| m.id());
            for c in named_children(n) {
                if Some(c.id()) == mod_id {
                    continue;
                }
                if c.kind() == "wildcard_import" {
                    out.push(Import {
                        pos: pos_of(c),
                        module: Cow::Borrowed(module),
                        name: "*",
                        alias: None,
                        kind: ImportKind::Wildcard,
                        in_test: cx.is_test_file,
                    });
                } else if c.kind() == "aliased_import" {
                    let path = child_by_field(c, "name").map_or("", |v| text(cx, v));
                    let alias = child_by_field(c, "alias").map_or("", |v| text(cx, v));
                    let named = (c, module, last_segment(path, &["."]), Some(alias));
                    plain_import(cx, named, out);
                } else if c.kind() == "dotted_name" {
                    let nm = text(cx, c);
                    plain_import(cx, (c, module, last_segment(nm, &["."]), None), out);
                }
            }
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        if n.kind() != "class_definition" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let public = !name.starts_with('_');
        let mut abstraction = false;
        let mut impls = Vec::new();
        if let Some(sup) = child_by_field(n, "superclasses") {
            for base in named_children(sup) {
                if base.kind() == "keyword_argument" {
                    abstraction = abstraction || is_meta_abc(cx, base);
                    continue;
                }
                let raw = if base.kind() == "subscript" {
                    child_by_field(base, "value").map_or("", |v| text(cx, v))
                } else {
                    text(cx, base)
                };
                let seg = last_segment(raw, &["."]);
                if seg == "Protocol" || seg == "ABC" {
                    abstraction = true;
                } else if seg != "object" && seg != "Generic" {
                    impls.push(ImplRef {
                        name: seg,
                        pos: pos_of(base),
                    });
                }
            }
        }
        Some(TypeHead {
            def: Some(TypeDef {
                name,
                pos: pos_of(name_node),
                public,
                abstraction,
            }),
            impls,
            container: Some(Container::Class {
                exported: public,
                implements: false,
            }),
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "case_clause" {
            return None;
        }
        if child_by_field(n, "guard").is_some() {
            return None;
        }
        let pat = named_children(n).into_iter().next()?;
        if text(cx, pat) != "_" {
            return None;
        }
        let enum_like = n.parent().is_some_and(|p| {
            named_children(p)
                .into_iter()
                .filter(|c| c.kind() == "case_clause" && c.id() != n.id())
                .any(|arm| {
                    named_children(arm)
                        .into_iter()
                        .next()
                        .and_then(|pat| {
                            if pat.kind() == "case_pattern" {
                                named_children(pat).into_iter().next()
                            } else {
                                Some(pat)
                            }
                        })
                        .is_some_and(|q| {
                            (q.kind() == "dotted_name" && named_children(q).len() >= 2)
                                || q.kind() == "class_pattern"
                        })
                })
        });
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like,
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "except_clause" {
            return None;
        }
        let binding = child_by_field(n, "alias").map_or("", |a| strip_as(text(cx, a)));
        let block = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "block")?;
        let mut stmts = Vec::new();
        for s in named_children(block) {
            if s.kind() == "pass_statement" {
                continue;
            }
            if s.kind() == "expression_statement" {
                let inner = named_children(s).into_iter().next();
                if inner.is_some_and(|v| v.kind() == "ellipsis") {
                    continue;
                }
            }
            stmts.push(raise_stmt(cx, s, binding));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "call" {
            return None;
        }
        let list = child_by_field(n, "arguments")?;
        let args = named_children(list);
        if args.is_empty() {
            return None;
        }
        let mut fields = 0u32;
        let mut source: Option<&'s str> = None;
        let mut poisoned = false;
        for a in &args {
            if a.kind() != "keyword_argument" {
                poisoned = true;
                continue;
            }
            fields += 1;
            let want = child_by_field(*a, "name").map_or("", |x| text(cx, x));
            let cand = child_by_field(*a, "value").and_then(|v| {
                if v.kind() != "attribute" {
                    return None;
                }
                let obj = child_by_field(v, "object")?;
                if obj.kind() != "identifier" {
                    return None;
                }
                if child_by_field(v, "attribute").map_or("", |x| text(cx, x)) != want {
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
        if n.kind() == "global_statement" {
            if !in_function {
                return;
            }
            for c in named_children(n) {
                if c.kind() == "identifier" {
                    out.push((AmbientKind::GlobalWrite, text(cx, c), pos_of(c)));
                }
            }
            return;
        }
        if n.kind() == "attribute" {
            let obj = child_by_field(n, "object").map_or("", |o| text(cx, o));
            let attr = child_by_field(n, "attribute").map_or("", |a| text(cx, a));
            if obj == "os" && attr == "environ" {
                out.push((AmbientKind::EnvRead, "os.environ", pos_of(n)));
            }
            return;
        }
        if n.kind() == "call" {
            let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
            if callee == "os.getenv" || callee == "getenv" {
                out.push((AmbientKind::EnvRead, "os.getenv", pos_of(n)));
            }
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() == "call" {
            let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
            let last = last_segment(callee, &["."]);
            let args = child_by_field(n, "arguments").map_or(Vec::new(), named_children);
            let mut positional = args.into_iter().filter(|a| a.kind() != "keyword_argument");
            if last == "getattr" || last == "setattr" || last == "hasattr" || last == "delattr" {
                let second = positional.nth(1);
                if second.is_some_and(|s| s.kind() != "string") {
                    return match last {
                        "getattr" => Some("getattr with computed name"),
                        "setattr" => Some("setattr with computed name"),
                        "hasattr" => Some("hasattr with computed name"),
                        _ => Some("delattr with computed name"),
                    };
                }
                return None;
            }
            if last == "eval" {
                return Some("eval");
            }
            if last == "exec" {
                return Some("exec");
            }
            if last == "__import__" {
                return Some("__import__");
            }
            if callee == "importlib.import_module" {
                return positional
                    .next()
                    .is_some_and(|f| f.kind() != "string")
                    .then_some("dynamic import");
            }
            return None;
        }
        if n.kind() == "subscript"
            && let Some(v) = child_by_field(n, "value")
            && v.kind() == "call"
        {
            let f = child_by_field(v, "function").map_or("", |x| text(cx, x));
            let last = last_segment(f, &["."]);
            if last == "globals" || last == "locals" {
                return Some("globals() lookup");
            }
        }
        None
    }

    fn assertion(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "assert_statement" {
            return None;
        }
        let kids = named_children(n);
        Some(Assertion {
            pos: pos_of(n),
            has_message: kids.len() >= 2,
            comparison: kids
                .first()
                .is_some_and(|f| f.kind() == "comparison_operator"),
        })
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        if !cx.path.ends_with("__init__.py") {
            return None;
        }
        if root.kind() != "module" {
            return None;
        }
        let mut found_import = false;
        for child in named_children(root) {
            match child.kind() {
                "comment" => {}
                "import_from_statement" => found_import = true,
                "expression_statement" => {
                    if !is_doc_expr(child) {
                        return None;
                    }
                }
                "assignment" => {
                    if child_by_field(child, "left").map_or("", |l| text(cx, l)) != "__all__" {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        if !found_import {
            return None;
        }
        let dir = parent_dir(cx.path);
        let mut targets = Vec::new();
        for child in named_children(root) {
            if child.kind() != "import_from_statement" {
                continue;
            }
            targets.push(py_targets(cx, dir, child));
        }
        Some(Barrel { targets })
    }
}
