use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, Import, ImportKind, Param,
    Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field, last_segment,
    named_children, narrow, pos_of, preceding_comments, string_arg_shape, strip_quotes, text,
};
use crate::spec::Role;

/// Role table for the Go grammar.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function),
    ("method_declaration", Role::Function),
    ("func_literal", Role::Closure),
    ("if_statement", Role::If),
    ("expression_switch_statement", Role::Match),
    ("type_switch_statement", Role::Match),
    ("select_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("call_expression", Role::Call),
    ("var_declaration", Role::Decl),
    ("short_var_declaration", Role::Bind),
    ("assignment_statement", Role::Write),
    ("inc_statement", Role::Write),
    ("dec_statement", Role::Write),
    ("import_declaration", Role::Import),
    ("type_spec", Role::TypeDef),
    ("comment", Role::Comment),
    ("default_case", Role::MatchArm),
    ("composite_literal", Role::StructLit),
];

/// Hooks for the Go grammar.
pub(crate) struct GoHooks;

/// Shared hook table for the Go grammar.
pub(crate) static HOOKS: GoHooks = GoHooks;

fn variadic_ty<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    text(cx, n).find("...").map_or("", |i| &text(cx, n)[i..])
}

fn go_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        if child.kind() == "parameter_declaration" {
            let ty = child_by_field(child, "type").map(|t| text(cx, t));
            let mut any = false;
            for name in named_children(child) {
                if name.kind() != "identifier" {
                    continue;
                }
                any = true;
                out.push(Param {
                    name: text(cx, name),
                    ty,
                    bool_default: false,
                });
            }
            if !any {
                out.push(Param {
                    name: "",
                    ty,
                    bool_default: false,
                });
            }
        } else if child.kind() == "variadic_parameter_declaration" {
            out.push(Param {
                name: child_by_field(child, "name").map_or("", |v| text(cx, v)),
                ty: Some(variadic_ty(cx, child)),
                bool_default: false,
            });
        }
    }
}

fn go_return_expr(n: Node<'_>) -> Option<Node<'_>> {
    let mut kids = named_children(n).into_iter();
    let only = kids.next()?;
    if kids.next().is_some() {
        return None;
    }
    if only.kind() == "expression_list" {
        let mut inner = named_children(only).into_iter();
        let one = inner.next()?;
        if inner.next().is_some() {
            return None;
        }
        Some(one)
    } else {
        Some(only)
    }
}

fn go_single_call(n: Node<'_>) -> Option<usize> {
    match n.kind() {
        "expression_statement" => {
            let inner = named_children(n).into_iter().next()?;
            (inner.kind() == "call_expression").then_some(inner.id())
        }
        "return_statement" => {
            let expr = go_return_expr(n)?;
            (expr.kind() == "call_expression").then_some(expr.id())
        }
        _ => None,
    }
}

fn go_sole(body: Node<'_>) -> Option<usize> {
    let list = named_children(body)
        .into_iter()
        .find(|c| c.kind() == "statement_list")?;
    let mut stmts = named_children(list).into_iter();
    let only = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    go_single_call(only)
}

fn push_go_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if let Some(root) = go_root(cx, n) {
        out.push(root);
    }
}
fn write_target<'s>(cx: &Cx<'_, 's>, c: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if c.kind() == "expression_list" {
        for e in named_children(c) {
            push_go_root(cx, e, out);
        }
    } else {
        push_go_root(cx, c, out);
    }
}
fn go_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some((text(cx, cur), pos_of(cur))),
            "selector_expression" => cur = child_by_field(cur, "operand")?,
            "index_expression" => cur = child_by_field(cur, "operand")?,
            "unary_expression" => cur = child_by_field(cur, "operand")?,
            "parenthesized_expression" => cur = named_children(cur).into_iter().next()?,
            _ => return None,
        }
    }
}

fn go_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "interpreted_string_literal" => {
            string_arg_shape(cx, n, &["interpreted_string_literal_content"])
        }
        "raw_string_literal" => string_arg_shape(cx, n, &["raw_string_literal_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(
            cx,
            n,
            "+",
            &["interpreted_string_literal", "raw_string_literal"],
        ) =>
        {
            ArgShape::Concat
        }
        _ => ArgShape::Other,
    }
}

fn go_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: go_shape(cx, n),
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn version_seg(module: &str) -> &str {
    let last = module.rsplit('/').next().map_or("", |s| s);
    let versioned =
        last.len() > 1 && last.starts_with('v') && last[1..].bytes().all(|b| b.is_ascii_digit());
    if !versioned {
        return last;
    }
    let Some(cut) = module.len().checked_sub(last.len() + 1) else {
        return last;
    };
    module[..cut].rsplit('/').next().map_or(last, |s| s)
}

fn spec_import<'s>(cx: &Cx<'_, 's>, spec: Node<'_>, out: &mut Vec<Import<'s>>) {
    let module = child_by_field(spec, "path").map_or("", |p| strip_quotes(text(cx, p)));
    let seg = version_seg(module);
    let pos = pos_of(spec);
    let test = cx.is_test_file;
    match child_by_field(spec, "name").map(|v| v.kind()) {
        Some("dot") => out.push(Import {
            pos,
            module: Cow::Borrowed(module),
            name: "*",
            alias: None,
            kind: ImportKind::Wildcard,
            in_test: test,
        }),
        Some("blank_identifier") => out.push(Import {
            pos,
            module: Cow::Borrowed(module),
            name: "",
            alias: None,
            kind: ImportKind::SideEffect,
            in_test: test,
        }),
        Some(_) => out.push(Import {
            pos,
            module: Cow::Borrowed(module),
            name: seg,
            alias: child_by_field(spec, "name").map(|v| text(cx, v)),
            kind: ImportKind::Named,
            in_test: test,
        }),
        None => out.push(Import {
            pos,
            module: Cow::Borrowed(module),
            name: seg,
            alias: None,
            kind: ImportKind::Named,
            in_test: test,
        }),
    }
}

fn go_enum(_cx: &Cx<'_, '_>, sw: Node<'_>) -> bool {
    let mut found = false;
    for case in named_children(sw) {
        if case.kind() != "expression_case" {
            continue;
        }
        let Some(vals) = child_by_field(case, "value") else {
            continue;
        };
        for v in named_children(vals) {
            match v.kind() {
                "identifier" | "selector_expression" => found = true,
                "int_literal"
                | "float_literal"
                | "imaginary_literal"
                | "rune_literal"
                | "interpreted_string_literal"
                | "raw_string_literal"
                | "true"
                | "false"
                | "nil" => return false,
                _ => {}
            }
        }
    }
    found
}

fn go_last_expr(n: Node<'_>) -> Option<Node<'_>> {
    let mut cur = named_children(n).into_iter().last()?;
    loop {
        if cur.kind() == "expression_list" {
            cur = named_children(cur).into_iter().last()?;
        } else {
            return Some(cur);
        }
    }
}

fn go_handler_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, bound: &str) -> Stmt<'s> {
    if s.kind() == "expression_statement" {
        if let Some(v) = named_children(s).into_iter().next()
            && v.kind() == "call_expression"
        {
            let callee = child_by_field(v, "function").map_or("", |f| text(cx, f));
            return Stmt::Call {
                callee,
                kind: CallKind::Plain,
            };
        }
        return Stmt::Other;
    }
    if s.kind() == "return_statement"
        && go_last_expr(s).is_some_and(|e| e.kind() == "identifier" && text(cx, e) == bound)
    {
        return Stmt::Rethrow;
    }
    Stmt::Other
}

fn go_keyed<'s>(cx: &Cx<'_, 's>, el: Node<'_>) -> Option<&'s str> {
    let key = unwrap_literal_element(child_by_field(el, "key")?);
    if key.kind() != "identifier" {
        return None;
    }
    let val = unwrap_literal_element(child_by_field(el, "value")?);
    if val.kind() != "selector_expression" {
        return None;
    }
    let obj = child_by_field(val, "operand")?;
    if obj.kind() != "identifier" {
        return None;
    }
    if child_by_field(val, "field").map_or("", |f| text(cx, f)) != text(cx, key) {
        return None;
    }
    Some(text(cx, obj))
}

fn unwrap_literal_element(n: Node<'_>) -> Node<'_> {
    if n.kind() == "literal_element" && n.named_child_count() == 1 {
        return n.named_child(0).unwrap_or(n);
    }
    n
}

fn is_sentinel(cx: &Cx<'_, '_>, spec: Node<'_>) -> bool {
    child_by_field(spec, "value").is_some_and(|v| {
        let expr = if v.kind() == "expression_list" {
            v.named_child(0).unwrap_or(v)
        } else {
            v
        };
        expr.kind() == "call_expression"
            && child_by_field(expr, "function").is_some_and(|f| {
                let t = text(cx, f);
                t == "errors.New" || t == "fmt.Errorf"
            })
    })
}

fn tail_two(callee: &str) -> &str {
    let mut first = callee.len();
    let mut second = callee.len();
    for (i, c) in callee.char_indices() {
        if c == '.' || c == '/' {
            second = first;
            first = i + 1;
        }
    }
    if second == callee.len() {
        callee
    } else {
        callee.split_at(second).1
    }
}

impl LangHooks for GoHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let mut params = Vec::new();
        if let Some(list) = child_by_field(n, "parameters") {
            go_params(cx, list, &mut params);
        }
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public: name.chars().next().is_some_and(|c| c.is_uppercase()),
            interface_bound: n.kind() == "method_declaration",
            in_test: cx.is_test_file
                && ["Test", "Benchmark", "Fuzz", "Example"]
                    .iter()
                    .any(|p| name.starts_with(*p)),
            params,
            return_type: child_by_field(n, "result").map(|t| text(cx, t)),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: go_sole(child_by_field(n, "body")?),
        })
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent()
            .and_then(|p| p.child_by_field_name("alternative"))
            .is_some_and(|a| a.id() == n.id())
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "var_declaration" {
            let mut specs = Vec::new();
            for child in named_children(n) {
                if child.kind() == "var_spec" {
                    specs.push(child);
                } else {
                    specs.extend(
                        named_children(child)
                            .into_iter()
                            .filter(|s| s.kind() == "var_spec"),
                    );
                }
            }
            for spec in specs {
                for c in named_children(spec) {
                    if c.kind() == "identifier" && text(cx, c) != "_" {
                        out.push((text(cx, c), pos_of(c)));
                    }
                }
            }
            return;
        }
        let Some(left) = child_by_field(n, "left") else {
            return;
        };
        for id in named_children(left) {
            if id.kind() == "identifier" && text(cx, id) != "_" {
                out.push((text(cx, id), pos_of(id)));
            }
        }
        if left.kind() == "identifier" && text(cx, left) != "_" {
            out.push((text(cx, left), pos_of(left)));
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "assignment_statement" {
            let right = child_by_field(n, "right");
            for c in named_children(n) {
                if right.is_some_and(|r| r.id() == c.id()) {
                    continue;
                }
                write_target(cx, c, out);
            }
            return;
        }
        if let Some(child) = named_children(n).into_iter().next()
            && let Some(root) = go_root(cx, child)
        {
            out.push(root);
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let args = child_by_field(n, "arguments").map_or(Vec::new(), |a| {
            named_children(a)
                .into_iter()
                .map(|c| go_arg(cx, c))
                .collect()
        });
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: None,
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        for child in named_children(n) {
            if child.kind() == "import_spec" {
                spec_import(cx, child, out);
            } else {
                for spec in named_children(child) {
                    if spec.kind() == "import_spec" {
                        spec_import(cx, spec, out);
                    }
                }
            }
        }
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        if n.kind() != "type_spec" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        Some(TypeHead {
            def: Some(TypeDef {
                name,
                pos: pos_of(name_node),
                public: name.chars().next().is_some_and(|c| c.is_uppercase()),
                abstraction: child_by_field(n, "type")
                    .is_some_and(|t| t.kind() == "interface_type"),
            }),
            impls: Vec::new(),
            container: None,
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let parent = n.parent()?;
        if parent.kind() != "expression_switch_statement" {
            return None;
        }
        Some(WildcardArm {
            pos: pos_of(n),
            enum_like: go_enum(cx, parent),
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "if_statement" {
            return None;
        }
        let cond = child_by_field(n, "condition")?;
        if cond.kind() != "binary_expression" {
            return None;
        }
        let left = child_by_field(cond, "left")?;
        if left.kind() != "identifier" {
            return None;
        }
        if child_by_field(cond, "operator").map_or("", |o| text(cx, o)) != "!=" {
            return None;
        }
        let right = child_by_field(cond, "right")?;
        if right.kind() != "nil" {
            return None;
        }
        let bound = text(cx, left);
        let body = child_by_field(n, "consequence")?;
        let mut stmts = Vec::new();
        if let Some(list) = named_children(body)
            .into_iter()
            .find(|c| c.kind() == "statement_list")
        {
            for s in named_children(list) {
                if s.kind() == "comment" {
                    continue;
                }
                stmts.push(go_handler_stmt(cx, s, bound));
            }
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "composite_literal" {
            return None;
        }
        let body = child_by_field(n, "body")?;
        let mut fields = 0u32;
        let mut source: Option<&'s str> = None;
        let mut poisoned = false;
        for el in named_children(body) {
            if el.kind() != "keyed_element" {
                poisoned = true;
                continue;
            }
            fields += 1;
            match (source, go_keyed(cx, el)) {
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
        if n.kind() == "var_declaration" {
            if in_function || n.parent().is_some_and(|p| p.kind() != "source_file") {
                return;
            }
            let mut specs = Vec::new();
            for child in named_children(n) {
                if child.kind() == "var_spec" {
                    specs.push(child);
                } else {
                    specs.extend(
                        named_children(child)
                            .into_iter()
                            .filter(|s| s.kind() == "var_spec"),
                    );
                }
            }
            for spec in specs {
                if is_sentinel(cx, spec) {
                    continue;
                }
                for c in named_children(spec) {
                    if c.kind() == "identifier" {
                        out.push((AmbientKind::GlobalMutable, text(cx, c), pos_of(c)));
                    }
                }
            }
            return;
        }
        if n.kind() == "call_expression" {
            let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
            if callee == "os.Getenv" || callee == "os.LookupEnv" || callee == "os.Environ" {
                out.push((AmbientKind::EnvRead, callee, pos_of(n)));
            }
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "call_expression" {
            return None;
        }
        let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
        let last = last_segment(callee, &[".", "/"]);
        if last != "MethodByName" && last != "FieldByName" {
            return None;
        }
        let first =
            child_by_field(n, "arguments").and_then(|a| named_children(a).into_iter().next());
        if first.is_some_and(|f| {
            f.kind() == "interpreted_string_literal" || f.kind() == "raw_string_literal"
        }) {
            return None;
        }
        Some("reflection by name")
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
        let tail = tail_two(callee);
        if tail != "assert.True"
            && tail != "assert.False"
            && tail != "require.True"
            && tail != "require.False"
        {
            return None;
        }
        let count = child_by_field(n, "arguments").map_or(0, |a| named_children(a).len());
        Some(Assertion {
            pos: pos_of(n),
            has_message: count >= 3,
            comparison: false,
        })
    }
}
