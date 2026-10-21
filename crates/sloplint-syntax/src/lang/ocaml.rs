use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, first_of_kind,
    has_child_kind, last_segment, named_children, narrow, pos_of, preceding_comments,
    string_arg_shape, text, trimmed,
};
use crate::spec::Role;

/// Role table mapping OCaml node kinds to walker roles; both grammars share these kinds.
pub(crate) static TABLE: crate::lang::Table = &[
    ("let_binding", Role::Function),
    ("method_definition", Role::Function),
    ("value_specification", Role::Function),
    ("fun_expression", Role::Closure),
    ("function_expression", Role::Closure),
    ("if_expression", Role::If),
    ("else_clause", Role::ElseClause),
    ("match_expression", Role::Match),
    ("while_expression", Role::Loop),
    ("for_expression", Role::Loop),
    ("try_expression", Role::Catch),
    ("application_expression", Role::Call),
    ("new_expression", Role::NewCall),
    ("value_definition", Role::Decl),
    ("infix_expression", Role::Write),
    ("open_module", Role::Import),
    ("include_module", Role::Import),
    ("module_binding", Role::Import),
    ("type_binding", Role::TypeDef),
    ("module_type_definition", Role::TypeDef),
    ("structure", Role::Impl),
    ("comment", Role::Comment),
    ("match_case", Role::MatchArm),
    ("record_expression", Role::StructLit),
    ("assert_expression", Role::Assert),
];

/// OCaml language hooks.
pub(crate) static HOOKS: OcamlHooks = OcamlHooks;

/// OCaml adapter.
pub(crate) struct OcamlHooks;

fn ocaml_public(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if cx.lang != Lang::Ocaml || cx.has_interface {
        return false;
    }
    let Some(parent) = n.parent() else {
        return false;
    };
    if parent.kind() == "compilation_unit" || parent.kind() == "structure" {
        return true;
    }
    parent
        .parent()
        .is_some_and(|g| g.kind() == "compilation_unit" || g.kind() == "structure")
}

fn impl_bound(cx: &Cx<'_, '_>) -> bool {
    cx.nearest_impl_interface_bound().unwrap_or(false)
}

fn is_parameter(c: &Node<'_>) -> bool {
    c.kind() == "parameter"
}

fn is_let_binding(c: &Node<'_>) -> bool {
    c.kind() == "let_binding"
}

fn is_true_or_false(cx: &Cx<'_, '_>, d: Node<'_>) -> bool {
    text(cx, d) == "true" || text(cx, d) == "false"
}

fn ocaml_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let pat = child_by_field(p, "pattern")?;
    if pat.kind() == "typed_pattern" {
        let name = child_by_field(pat, "pattern")
            .map(|q| text(cx, q))
            .unwrap_or("");
        let ty = child_by_field(pat, "type").map(|t| text(cx, t));
        return Some(param_at(cx, p, name, ty));
    }
    let ty = child_by_field(p, "type").map(|t| text(cx, t));
    Some(param_at(cx, p, text(cx, pat), ty))
}

fn param_at<'s>(cx: &Cx<'_, 's>, p: Node<'_>, name: &'s str, ty: Option<&'s str>) -> Param<'s> {
    let default = child_by_field(p, "default");
    let constant = default.is_some_and(|d| is_true_or_false(cx, d));
    Param {
        name,
        ty,
        bool_default: constant,
    }
}

fn ocaml_params<'s>(cx: &Cx<'_, 's>, holder: Node<'_>) -> Vec<Param<'s>> {
    named_children(holder)
        .into_iter()
        .filter(is_parameter)
        .filter_map(|p| ocaml_param(cx, p))
        .collect()
}

fn ocaml_header<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<&'s str>, Option<Pos>) {
    if let Some(parent) = n.parent()
        && parent.kind() == "value_definition"
    {
        let first = named_children(parent).into_iter().find(is_let_binding);
        if first.is_some_and(|f| f.id() != n.id()) {
            return (Vec::new(), None);
        }
        return preceding_comments(cx, parent, &["comment"], &[]);
    }
    preceding_comments(cx, n, &["comment"], &[])
}

fn ocaml_sole(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    let mut inner = body;
    if inner.kind() == "parenthesized_expression" {
        inner = named_children(inner).first().copied()?;
    }
    (inner.kind() == "application_expression").then_some(inner.id())
}

fn missing_of<'s>(params: &[Param<'s>], return_type: Option<&str>) -> Vec<&'s str> {
    let mut missing: Vec<&'s str> = params
        .iter()
        .filter(|p| p.ty.is_none() && p.name != "()")
        .map(|p| p.name)
        .collect();
    if return_type.is_none() {
        missing.push("return");
    }
    missing
}

fn let_head<'s>(
    cx: &Cx<'_, 's>,
    lb: Node<'_>,
    params_src: Node<'_>,
    body: Node<'_>,
) -> FunctionHead<'s> {
    let name_node = child_by_field(lb, "pattern");
    let params = ocaml_params(cx, params_src);
    let return_type = child_by_field(lb, "type").map(|t| text(cx, t));
    let missing = missing_of(&params, return_type);
    let (doc, doc_pos) = ocaml_header(cx, lb);
    FunctionHead {
        name: name_node.map(|p| text(cx, p)).unwrap_or(""),
        name_pos: name_node.map(pos_of).unwrap_or_else(|| pos_of(lb)),
        public: ocaml_public(cx, lb),
        interface_bound: impl_bound(cx),
        in_test: cx.is_test_file,
        params,
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: ocaml_sole(cx, body),
    }
}

fn spec_signature<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<Param<'s>>, Option<&'s str>) {
    let Some(mut current) = child_by_field(n, "type") else {
        return (Vec::new(), None);
    };
    if current.kind() == "polymorphic_type" {
        return (Vec::new(), Some(text(cx, current)));
    }
    let mut params = Vec::new();
    loop {
        if current.kind() != "function_type" {
            return (params, Some(text(cx, current)));
        }
        let domain = child_by_field(current, "domain");
        let labeled = domain.is_some_and(|d| d.kind() == "labeled_argument_type");
        let name = match domain {
            Some(found) if labeled => label_of(cx, found),
            _ => "",
        };
        let ty = domain.map(|d| text(cx, d));
        params.push(Param {
            name,
            ty,
            bool_default: false,
        });
        match child_by_field(current, "codomain") {
            Some(next) => current = next,
            None => return (params, None),
        }
    }
}

fn label_of<'s>(cx: &Cx<'_, 's>, domain: Node<'_>) -> &'s str {
    child_by_field(domain, "label_name")
        .map(|l| text(cx, l))
        .unwrap_or("")
}

fn pattern_ident<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> Option<(&'s str, Pos)> {
    if matches!(pat.kind(), "identifier" | "value_name") {
        return Some((text(cx, pat), pos_of(pat)));
    }
    let target = if pat.kind() == "value_pattern" {
        pat
    } else {
        first_of_kind(pat, &["value_pattern"])?
    };
    let id = named_children(target)
        .into_iter()
        .find(|c| matches!(c.kind(), "identifier" | "value_name"));
    match id {
        Some(id) => Some((text(cx, id), pos_of(id))),
        None if target.named_child_count() == 0 => Some((text(cx, target), pos_of(target))),
        None => None,
    }
}

fn is_ref_callee(cx: &Cx<'_, '_>, f: Node<'_>) -> bool {
    if f.kind() != "value_path" {
        return false;
    }
    text(cx, f) == "ref" || text(cx, f) == "Stdlib.ref"
}

fn is_ref_binding(cx: &Cx<'_, '_>, lb: Node<'_>) -> bool {
    if named_children(lb).iter().any(is_parameter) {
        return false;
    }
    let body = child_by_field(lb, "body");
    if !body.is_some_and(|b| b.kind() == "application_expression") {
        return false;
    }
    body.and_then(|b| child_by_field(b, "function"))
        .is_some_and(|f| is_ref_callee(cx, f))
}

fn push_first_binding<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    for lb in named_children(n) {
        if lb.kind() != "let_binding" {
            continue;
        }
        if let Some(pat) = child_by_field(lb, "pattern")
            && let Some((name, pos)) = pattern_ident(cx, pat)
        {
            let global = (AmbientKind::GlobalMutable, name, pos);
            out.push(global);
            return;
        }
    }
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        let kind = current.kind();
        if kind == "identifier" {
            return Some((text(cx, current), pos_of(current)));
        } else if kind == "field_get_expression" {
            current = child_by_field(current, "record")?;
        } else if kind == "parenthesized_expression" {
            current = child_by_field(current, "expression")?;
        } else if kind == "value_path" {
            if has_child_kind(current, "module_path") {
                return None;
            }
            let name = first_of_kind(current, &["value_name"])?;
            return Some((text(cx, name), pos_of(name)));
        } else {
            return None;
        }
    }
}

fn infix_write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let assign = child_by_field(n, "operator");
    if !assign.is_some_and(|op| op.kind() == "assign_operator") {
        return None;
    }
    write_root(cx, child_by_field(n, "left")?)
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: arg_shape(cx, n),
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn concat_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let operator = child_by_field(n, "operator");
    let caret = operator.is_some_and(|op| op.kind() == "concat_operator" && text(cx, op) == "^");
    if !caret {
        return ArgShape::Other;
    }
    let quoted = named_children(n)
        .iter()
        .any(|c| c.kind() == "string" || c.kind() == "quoted_string");
    if quoted {
        ArgShape::Concat
    } else {
        ArgShape::Other
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "quoted_string" => string_arg_shape(cx, n, &["quoted_string_content"]),
        "infix_expression" => concat_shape(cx, n),
        "value_path" => {
            if has_child_kind(n, "module_path") {
                ArgShape::Other
            } else {
                ArgShape::Ident(text(cx, n))
            }
        }
        _ => ArgShape::Other,
    }
}

fn is_rel_compare(cx: &Cx<'_, '_>, op: Node<'_>) -> bool {
    if op.kind() != "rel_operator" {
        return false;
    }
    let symbol = text(cx, op);
    symbol == "=" || symbol == "==" || symbol == "<>" || symbol == "!="
}

fn is_raise(cx: &Cx<'_, '_>, func: Option<Node<'_>>) -> bool {
    func.is_some_and(|f| {
        let callee = text(cx, f);
        callee == "raise" || callee == "raise_notrace" || callee == "Stdlib.raise"
    })
}

fn case_stmt<'s>(cx: &Cx<'_, 's>, case: Node<'_>) -> Stmt<'s> {
    let kids = named_children(case);
    let Some(body) = kids.last().copied() else {
        return Stmt::Other;
    };
    if body.kind() != "application_expression" {
        return Stmt::Other;
    }
    let func = child_by_field(body, "function");
    if !is_raise(cx, func) {
        let callee = func.map(|f| trimmed(cx, f)).unwrap_or("");
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    let bound = child_by_field(case, "pattern")
        .and_then(|p| pattern_ident(cx, p))
        .map(|(name, _)| name)
        .unwrap_or("");
    for arg in named_children(body) {
        if Some(arg.id()) == func.map(|f| f.id()) {
            continue;
        }
        if text(cx, arg) == bound {
            return Stmt::Rethrow;
        }
        return Stmt::Other;
    }
    Stmt::Other
}

fn field_source<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> Option<&'s str> {
    let path = child_by_field(field, "field_path")?;
    let body = child_by_field(field, "body")?;
    if body.kind() != "field_get_expression" {
        return None;
    }
    let record = child_by_field(body, "record")?;
    if !matches!(record.kind(), "identifier" | "value_path" | "value_name") {
        return None;
    }
    let target = child_by_field(body, "field")
        .map(|f| text(cx, f))
        .unwrap_or("");
    if target != text(cx, path) {
        return None;
    }
    Some(text(cx, record))
}

fn sibling_enum(n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    named_children(parent)
        .into_iter()
        .filter(|s| s.kind() == "match_case" && s.id() != n.id())
        .any(|s| {
            child_by_field(s, "pattern")
                .is_some_and(|q| q.kind() == "constructor_pattern" || q.kind() == "tag_pattern")
        })
}

fn typedef_at(name: &str, pos: Pos, public: bool, abstraction: bool) -> TypeDef<'_> {
    TypeDef {
        name,
        pos,
        public,
        abstraction,
    }
}

fn is_match_case(c: &Node<'_>) -> bool {
    c.kind() == "match_case"
}

impl LangHooks for OcamlHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        match n.kind() {
            "let_binding" => {
                if cx.lang != Lang::Ocaml {
                    return None;
                }
                let body = child_by_field(n, "body")?;
                let closure =
                    body.kind() == "fun_expression" || body.kind() == "function_expression";
                let typed = named_children(n).iter().any(is_parameter);
                if closure || !typed {
                    return None;
                }
                Some(let_head(cx, n, n, body))
            }
            "method_definition" => {
                let name_node = child_by_field(n, "method_name")?;
                let body = child_by_field(n, "body")?;
                let mut head = let_head(cx, n, n, body);
                head.name = text(cx, name_node);
                head.name_pos = pos_of(name_node);
                head.public = true;
                Some(head)
            }
            "value_specification" => {
                let name_node = child_by_field(n, "value_name")?;
                let (params, return_type) = spec_signature(cx, n);
                let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
                Some(FunctionHead {
                    name: text(cx, name_node),
                    name_pos: pos_of(name_node),
                    public: true,
                    interface_bound: false,
                    in_test: false,
                    params,
                    return_type,
                    missing_annotations: Vec::new(),
                    doc,
                    doc_pos,
                    sole_call: None,
                })
            }
            _ => None,
        }
    }

    fn closure_as_function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        if n.kind() != "fun_expression" && n.kind() != "function_expression" {
            return None;
        }
        let parent = n.parent()?;
        if parent.kind() != "let_binding" {
            return None;
        }
        let body = child_by_field(parent, "body");
        if body.is_some_and(|b| b.id() != n.id()) {
            return None;
        }
        Some(let_head(cx, parent, n, child_by_field(n, "body")?))
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent().is_some_and(|p| p.kind() == "else_clause")
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "value_definition" {
            return;
        }
        let mutable = has_child_kind(n, "mutable");
        let bindings: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(is_let_binding)
            .collect();
        for lb in bindings {
            if !mutable && !is_ref_binding(cx, lb) {
                continue;
            }
            if let Some(pat) = child_by_field(lb, "pattern")
                && let Some(found) = pattern_ident(cx, pat)
            {
                out.push(found);
            }
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "infix_expression" {
            if let Some(root) = infix_write_root(cx, n) {
                out.push(root);
            }
            return;
        }
        if n.kind() != "application_expression" {
            return;
        }
        if !is_incr(cx, n) {
            return;
        }
        let Some(arg) = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "argument")
        else {
            return;
        };
        let target = named_children(arg).first().copied().unwrap_or(arg);
        if let Some(root) = write_root(cx, target) {
            out.push(root);
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "new_expression" {
            let class = child_by_field(n, "class_path")?;
            let created = CallHead {
                callee: text(cx, class),
                kind: CallKind::New,
                callee_root: None,
                args: Vec::new(),
            };
            return Some(created);
        }
        if n.kind() != "application_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let callee = trimmed(cx, func);
        let mut cursor = n.walk();
        let mut args = Vec::new();
        for arg in n.children_by_field_name("argument", &mut cursor) {
            if arg.kind() == "labeled_argument" {
                match child_by_field(arg, "expression") {
                    Some(expr) => args.push(make_arg(cx, expr)),
                    None => args.push(make_arg(cx, arg)),
                }
            } else {
                args.push(make_arg(cx, arg));
            }
        }
        Some(CallHead {
            callee,
            kind: CallKind::Plain,
            callee_root: callee.split('.').next(),
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let pos = pos_of(n);
        if n.kind() == "open_module" || n.kind() == "include_module" {
            let Some(module) = child_by_field(n, "module") else {
                return;
            };
            let module = Cow::Borrowed(text(cx, module));
            let opened = Import {
                pos,
                module,
                name: "",
                alias: None,
                kind: ImportKind::Wildcard,
                in_test: false,
            };
            out.push(opened);
            return;
        }
        if n.kind() != "module_binding" {
            return;
        }
        let Some(body) = child_by_field(n, "body") else {
            return;
        };
        if body.kind() != "extended_module_path" {
            return;
        }
        let path = text(cx, body);
        let name = last_segment(path, &["."]);
        let found = child_by_field(n, "module_name").map(|m| text(cx, m));
        let alias = match found {
            Some(candidate) if candidate != name => Some(candidate),
            _ => None,
        };
        let module = Cow::Borrowed(path);
        let aliased = Import {
            pos,
            module,
            name,
            alias,
            kind: ImportKind::Namespace,
            in_test: false,
        };
        out.push(aliased);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        if n.kind() == "module_type_definition" {
            let name_node = child_by_field(n, "module_type_name")?;
            let def = typedef_at(
                text(cx, name_node),
                pos_of(name_node),
                cx.lang == Lang::OcamlInterface || ocaml_public(cx, n),
                true,
            );
            return Some(TypeHead {
                def: Some(def),
                impls: Vec::new(),
                container: None,
            });
        }
        if n.kind() != "type_binding" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let no_body = child_by_field(n, "body").is_none();
        let no_synonym = child_by_field(n, "synonym").is_none();
        let def = typedef_at(
            text(cx, name_node),
            pos_of(name_node),
            cx.lang == Lang::OcamlInterface || ocaml_public(cx, n),
            no_body && no_synonym,
        );
        Some(TypeHead {
            def: Some(def),
            impls: Vec::new(),
            container: None,
        })
    }

    fn container(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<Container> {
        if n.kind() != "structure" {
            return None;
        }
        let parent = n.parent()?;
        if parent.kind() != "module_binding" {
            return Some(Container::Impl {
                interface_bound: false,
            });
        }
        let bound = child_by_field(parent, "module_type").is_some();
        Some(Container::Impl {
            interface_bound: bound,
        })
    }

    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "match_case" {
            return None;
        }
        let pattern = child_by_field(n, "pattern");
        if !pattern.is_some_and(|p| p.kind() == "any_pattern") {
            return None;
        }
        let arm = WildcardArm {
            pos: pos_of(n),
            enum_like: sibling_enum(n),
        };
        Some(arm)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "try_expression" {
            return None;
        }
        let mut stmts = Vec::new();
        for case in named_children(n) {
            if !is_match_case(&case) {
                continue;
            }
            stmts.push(case_stmt(cx, case));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "record_expression" {
            return None;
        }
        let fields: Vec<Node<'_>> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() == "field_expression")
            .collect();
        if fields.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        let mut complete = true;
        for field in &fields {
            match field_source(cx, *field) {
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
        if n.kind() == "value_definition" {
            if in_function {
                return;
            }
            let mutable = has_child_kind(n, "mutable");
            let stored = named_children(n)
                .into_iter()
                .filter(is_let_binding)
                .any(|lb| is_ref_binding(cx, lb));
            if mutable || stored {
                push_first_binding(cx, n, out);
            }
            return;
        }
        if n.kind() != "application_expression" {
            return;
        }
        let Some(func) = child_by_field(n, "function") else {
            return;
        };
        let callee = text(cx, func);
        if is_env_callee(callee) {
            let read = (AmbientKind::EnvRead, callee, pos_of(n));
            out.push(read);
        }
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() == "assert_expression" {
            let comparison = child_by_field(n, "expression").is_some_and(|e| {
                e.kind() == "infix_expression"
                    && child_by_field(e, "operator").is_some_and(|op| is_rel_compare(cx, op))
            });
            let checked = Assertion {
                pos: pos_of(n),
                has_message: false,
                comparison,
            };
            return Some(checked);
        }
        if n.kind() != "application_expression" {
            return None;
        }
        let callee = child_by_field(n, "function")
            .map(|f| text(cx, f))
            .unwrap_or("");
        if callee != "Alcotest.check" && !callee.starts_with("Alcotest.(check") {
            return None;
        }
        let has_message = named_children(n).iter().any(|c| is_string_arg(cx, *c));
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison: false,
        })
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        if root.kind() != "compilation_unit" {
            return None;
        }
        let kids: Vec<Node<'_>> = named_children(root)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .collect();
        if kids.is_empty() || !kids.iter().all(|c| c.kind() == "include_module") {
            return None;
        }
        let dir = match cx.path.rfind('/') {
            Some(at) => &cx.path[..at],
            None => "",
        };
        let mut targets = Vec::new();
        for child in kids {
            let Some(module) = child_by_field(child, "module") else {
                continue;
            };
            let name = text(cx, module);
            if name.contains('.') {
                continue;
            }
            let lower = name.to_lowercase();
            if dir.is_empty() {
                targets.push(vec![format!("{lower}.ml"), format!("{lower}.mli")]);
            } else {
                targets.push(vec![
                    format!("{dir}/{lower}.ml"),
                    format!("{dir}/{lower}.mli"),
                ]);
            }
        }
        (!targets.is_empty()).then_some(Barrel { targets })
    }
}

fn is_incr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "function").is_some_and(|f| is_incr_callee(cx, f))
}

fn is_incr_callee(cx: &Cx<'_, '_>, f: Node<'_>) -> bool {
    text(cx, f) == "incr" || text(cx, f) == "decr"
}

fn is_env_callee(callee: &str) -> bool {
    callee == "Sys.getenv"
        || callee == "Sys.getenv_opt"
        || callee == "Unix.getenv"
        || callee == "Unix.environment"
}

fn is_string_arg(_cx: &Cx<'_, '_>, c: Node<'_>) -> bool {
    if c.kind() == "string" || c.kind() == "quoted_string" {
        return true;
    }
    if c.kind() != "labeled_argument" {
        return false;
    }
    child_by_field(c, "expression")
        .is_some_and(|e| e.kind() == "string" || e.kind() == "quoted_string")
}
