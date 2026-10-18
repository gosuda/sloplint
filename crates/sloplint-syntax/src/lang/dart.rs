use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field,
    has_child_kind, named_children, narrow, pos_of, preceding_comments, strip_quotes, text,
    trimmed,
};
use crate::spec::Role;

/// Node-kind roles for the Dart grammar.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function),
    ("method_declaration", Role::Function),
    ("local_function_declaration", Role::Function),
    ("function_expression", Role::Closure),
    ("if_statement", Role::If),
    ("switch_statement", Role::Match),
    ("switch_expression", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("do_statement", Role::Loop),
    ("catch_clause", Role::Catch),
    ("conditional_expression", Role::Ternary),
    ("call_expression", Role::Call),
    ("new_expression", Role::NewCall),
    ("const_object_expression", Role::NewCall),
    ("local_variable_declaration", Role::Decl),
    ("top_level_variable_declaration", Role::StaticItem),
    ("assignment_expression", Role::Write),
    ("postfix_expression", Role::Write),
    ("unary_expression", Role::Write),
    ("library_import", Role::Import),
    ("library_export", Role::Export),
    ("class_declaration", Role::TypeDef),
    ("mixin_declaration", Role::TypeDef),
    ("enum_declaration", Role::TypeDef),
    ("extension_type_declaration", Role::TypeDef),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("documentation_block_comment", Role::Comment),
    ("switch_statement_case", Role::MatchArm),
    ("switch_statement_default", Role::MatchArm),
    ("switch_expression_case", Role::MatchArm),
    ("member_expression", Role::Member),
    ("assert_statement", Role::Assert),
];

/// Hooks reading Dart declarations, calls, imports, and handlers.
pub(crate) static HOOKS: DartHooks = DartHooks;

/// Unit hooks for Dart.
pub(crate) struct DartHooks;

fn has_anon(n: Node<'_>, kind: &str) -> bool {
    (0..n.child_count())
        .filter_map(|i| n.child(i))
        .any(|c| !c.is_named() && c.kind() == kind)
}

fn cut_generics(t: &str) -> &str {
    match t.find('<') {
        Some(i) => &t[..i],
        None => t,
    }
}

fn last_ident(t: &str) -> &str {
    let base = cut_generics(t);
    let bytes = base.as_bytes();
    let mut end = base.len();
    while end > 0 && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    &base[start..end]
}

fn annotation_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let t = trimmed(cx, n);
    let t = match t.strip_prefix('@') {
        Some(s) => s,
        None => t,
    };
    match t.find('(') {
        Some(i) => &t[..i],
        None => t,
    }
}

fn signature_of(n: Node<'_>) -> Option<Node<'_>> {
    if n.kind() == "function_declaration" {
        let s = child_by_field(n, "signature")?;
        return (s.kind() == "function_signature").then_some(s);
    }
    if n.kind() == "method_declaration" {
        let ms = child_by_field(n, "signature")?;
        if ms.kind() != "method_signature" {
            return None;
        }
        return named_children(ms)
            .into_iter()
            .find(|c| c.kind() == "function_signature");
    }
    named_children(n)
        .into_iter()
        .find(|c| c.kind() == "function_signature")
}

fn param_from<'s>(cx: &Cx<'_, 's>, p: Node<'_>, next: Option<Node<'_>>, out: &mut Vec<Param<'s>>) {
    let Some(nm) = child_by_field(p, "name") else {
        return;
    };
    out.push(Param {
        name: text(cx, nm),
        ty: named_children(p)
            .into_iter()
            .find(|c| c.kind() == "type")
            .map(|t| text(cx, t)),
        bool_default: next.is_some_and(|nx| nx.kind() == "true" || nx.kind() == "false"),
    });
}

fn params_from_list<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    let kids = named_children(list);
    let mut i = 0;
    while i < kids.len() {
        if kids[i].kind() == "formal_parameter" {
            param_from(cx, kids[i], kids.get(i + 1).copied(), out);
        } else if kids[i].kind() == "optional_formal_parameters" {
            params_from_list(cx, kids[i], out);
        }
        i += 1;
    }
}

fn sole_call_of(body: Node<'_>) -> Option<usize> {
    let kids: Vec<Node> = named_children(body)
        .into_iter()
        .filter(|c| c.kind() != "comment" && c.kind() != "block_comment")
        .collect();
    if kids.len() != 1 {
        return None;
    }
    if kids[0].kind() == "call_expression" {
        return Some(kids[0].id());
    }
    if kids[0].kind() != "block" {
        return None;
    }
    let stmts: Vec<Node> = named_children(kids[0])
        .into_iter()
        .filter(|c| c.kind() != "comment" && c.kind() != "block_comment")
        .collect();
    if stmts.len() != 1 {
        return None;
    }
    if stmts[0].kind() != "return_statement" && stmts[0].kind() != "expression_statement" {
        return None;
    }
    named_children(stmts[0])
        .into_iter()
        .find(|c| c.kind() == "call_expression")
        .map(|c| c.id())
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    match n.kind() {
        "identifier" | "this" | "super" => Some((text(cx, n), pos_of(n))),
        "member_expression" | "index_expression" => write_root(cx, child_by_field(n, "object")?),
        "parenthesized_expression" => write_root(cx, named_children(n).into_iter().next()?),
        _ => None,
    }
}

fn callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    match func.kind() {
        "identifier" | "this" | "super" => Some(text(cx, func)),
        "member_expression" => callee_root(cx, child_by_field(func, "object")?),
        _ => None,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> ArgShape<'s> {
    if e.kind() == "string_literal" {
        let mut segs = Vec::new();
        for c in named_children(e) {
            if c.kind().starts_with("template_chars") {
                segs.push(text(cx, c));
            }
        }
        return ArgShape::Str {
            longest_static: longest_static_chunk(&segs),
        };
    }
    if e.kind() == "additive_expression" && binary_concat(cx, e, "+", &["string_literal"]) {
        return ArgShape::Concat;
    }
    if e.kind() == "identifier" {
        return ArgShape::Ident(text(cx, e));
    }
    ArgShape::Other
}

fn arg_value(arg: Node<'_>) -> Node<'_> {
    if arg.kind() == "named_argument" {
        named_children(arg).into_iter().last().unwrap_or(arg)
    } else {
        arg
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    let Some(list) = child_by_field(n, "arguments") else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .map(|a| {
            let e = arg_value(a);
            Arg {
                shape: arg_shape(cx, e),
                start_line: narrow(e.start_position().row) + 1,
                end_line: narrow(e.end_position().row) + 1,
            }
        })
        .collect()
}

fn export_uri<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> Option<&'s str> {
    let u = child_by_field(c, "uri")?;
    let inner = if u.kind() == "configurable_uri" {
        named_children(u).into_iter().find(|x| x.kind() == "uri")?
    } else {
        u
    };
    let lit = named_children(inner)
        .into_iter()
        .find(|x| x.kind() == "string_literal")?;
    Some(strip_quotes(text(cx, lit)))
}

fn label_text<'s>(cx: &Cx<'_, 's>, lab: Node<'_>) -> &'s str {
    named_children(lab)
        .into_iter()
        .next()
        .map(|id| text(cx, id))
        .unwrap_or_else(|| text(cx, lab))
}

fn arm_pattern_member(n: Node<'_>) -> bool {
    named_children(n).into_iter().any(|c| {
        c.kind() == "member_expression"
            || (!c.kind().contains("statement")
                && c.kind() != "block"
                && named_children(c)
                    .into_iter()
                    .any(|g| g.kind() == "member_expression"))
    })
}

fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "rethrow_statement" {
        return Stmt::Rethrow;
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    named_children(s)
        .into_iter()
        .find_map(|c| stmt_call(cx, c, binding))
        .unwrap_or(Stmt::Other)
}

fn stmt_call<'s>(cx: &Cx<'_, 's>, c: Node<'_>, binding: Option<&'s str>) -> Option<Stmt<'s>> {
    if c.kind() == "call_expression" {
        return child_by_field(c, "function").map(|f| Stmt::Call {
            callee: cut_generics(text(cx, f)),
            kind: CallKind::Plain,
        });
    }
    if c.kind() == "throw_expression" {
        let hit = named_children(c)
            .into_iter()
            .any(|v| v.kind() == "identifier" && Some(text(cx, v)) == binding);
        if hit {
            return Some(Stmt::Rethrow);
        }
    }
    None
}

fn top_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    named_children(n)
        .into_iter()
        .filter(|c| {
            c.kind() == "initialized_identifier_list" || c.kind() == "static_final_declaration_list"
        })
        .flat_map(named_children)
        .filter(|e| e.kind() == "initialized_identifier" || e.kind() == "static_final_declaration")
        .filter_map(|e| child_by_field(e, "name"))
        .next()
        .map(|nm| (text(cx, nm), pos_of(nm)))
}

fn iface_impls<'s>(cx: &Cx<'_, 's>, c: Node<'_>, impls: &mut Vec<ImplRef<'s>>) {
    for t in named_children(c) {
        if t.kind() == "type" {
            impls.push(ImplRef {
                name: last_ident(text(cx, t)),
                pos: pos_of(t),
            });
        }
    }
}

fn super_impls<'s>(cx: &Cx<'_, 's>, c: Node<'_>, impls: &mut Vec<ImplRef<'s>>) {
    for t in named_children(c) {
        if t.kind() == "type" {
            impls.push(ImplRef {
                name: last_ident(text(cx, t)),
                pos: pos_of(t),
            });
        } else if t.kind() == "mixins" {
            iface_impls(cx, t, impls);
        }
    }
}

fn is_reason_arg(cx: &Cx<'_, '_>, k: Node<'_>) -> bool {
    named_children(k)
        .into_iter()
        .next()
        .is_some_and(|l| l.kind() == "label" && label_text(cx, l) == "reason")
}

fn join_rel(dir: &str, uri: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in uri.split('/') {
        if seg == "." || seg.is_empty() {
            continue;
        } else if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    parts.join("/")
}

struct ImportEmit<'a, 's> {
    module: &'s str,
    pos: Pos,
    in_test: bool,
    out: &'a mut Vec<Import<'s>>,
}

impl<'a, 's> ImportEmit<'a, 's> {
    fn namespace(&mut self, name: &'s str, alias: Option<&'s str>) {
        self.out.push(Import {
            pos: self.pos,
            module: Cow::Borrowed(self.module),
            name,
            alias,
            kind: ImportKind::Namespace,
            in_test: self.in_test,
        });
    }
    fn named(&mut self, name: &'s str) {
        self.out.push(Import {
            pos: self.pos,
            module: Cow::Borrowed(self.module),
            name,
            alias: None,
            kind: ImportKind::Named,
            in_test: self.in_test,
        });
    }
    fn show_one(&mut self, cx: &Cx<'_, 's>, comb: Node<'_>) {
        for id in named_children(comb) {
            if id.kind() == "identifier" {
                self.named(text(cx, id));
            }
        }
    }
    fn show_all(&mut self, cx: &Cx<'_, 's>, spec: Node<'_>) {
        for c in named_children(spec) {
            if c.kind() == "combinator" && has_child_kind(c, "show") {
                self.show_one(cx, c);
            }
        }
    }
}

fn has_show(spec: Node<'_>) -> bool {
    named_children(spec)
        .into_iter()
        .any(|c| c.kind() == "combinator" && has_child_kind(c, "show"))
}

impl LangHooks for DartHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let kind = n.kind();
        let is_fn = kind == "function_declaration"
            || kind == "method_declaration"
            || kind == "local_function_declaration";
        if !is_fn {
            return None;
        }
        let body = child_by_field(n, "body")?;
        if body.kind() != "function_body" {
            return None;
        }
        let sig = signature_of(n)?;
        let name_node = child_by_field(sig, "name")?;
        let name = text(cx, name_node);
        let mut public = !name.starts_with('_');
        match cx.containers.last() {
            None => {}
            Some(Container::Class { exported, .. }) => {
                public = public && *exported;
            }
            Some(Container::Trait { public: tp }) => {
                public = public && *tp;
            }
            Some(_) => {}
        }
        let overridden = kind == "method_declaration"
            && named_children(n)
                .into_iter()
                .any(|c| c.kind() == "annotation" && annotation_name(cx, c) == "override");
        let interface_bound = matches!(
            cx.containers.last(),
            Some(Container::Class {
                implements: true,
                ..
            })
        ) || overridden;
        let mut params = Vec::new();
        for c in named_children(sig) {
            if c.kind() == "formal_parameter_list" {
                params_from_list(cx, c, &mut params);
            }
        }
        let mut missing = Vec::new();
        for p in &params {
            if p.ty.is_none() {
                missing.push(p.name);
            }
        }
        let return_type = child_by_field(sig, "return_type").map(|t| text(cx, t));
        if return_type.is_none() {
            missing.push("return");
        }
        let (doc, doc_pos) = preceding_comments(
            cx,
            n,
            &["comment", "block_comment", "documentation_block_comment"],
            &[],
        );
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public,
            interface_bound,
            in_test: cx.is_test_file,
            params,
            return_type,
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: sole_call_of(body),
        })
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        if n.kind() != "if_statement" {
            return false;
        }
        let Some(p) = n.parent() else {
            return false;
        };
        if p.kind() != "if_statement" {
            return false;
        }
        p.child_by_field_name("alternative")
            .is_some_and(|a| a.id() == n.id())
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "local_variable_declaration" {
            return;
        }
        for c in named_children(n) {
            if c.kind() == "initialized_variable_definition"
                && !has_anon(c, "final")
                && !has_anon(c, "const")
                && let Some(nm) = child_by_field(c, "name")
            {
                out.push((text(cx, nm), pos_of(nm)));
            }
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "assignment_expression" {
            if let Some(l) = child_by_field(n, "left") {
                out.extend(write_root(cx, l));
            }
            return;
        }
        if n.kind() == "postfix_expression" {
            if let Some(a) = child_by_field(n, "argument") {
                out.extend(write_root(cx, a));
            }
            return;
        }
        if n.kind() != "unary_expression" {
            return;
        }
        let first_anon = (0..n.child_count())
            .filter_map(|i| n.child(i))
            .find(|c| !c.is_named());
        if !first_anon.is_some_and(|c| c.kind() == "++" || c.kind() == "--") {
            return;
        }
        for c in named_children(n) {
            if c.kind() == "assignable_expression" {
                out.extend(write_root(cx, c));
            }
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "call_expression" {
            let func = child_by_field(n, "function")?;
            return Some(CallHead {
                callee: cut_generics(text(cx, func)),
                kind: CallKind::Plain,
                callee_root: callee_root(cx, func),
                args: call_args(cx, n),
            });
        }
        if n.kind() != "new_expression" && n.kind() != "const_object_expression" {
            return None;
        }
        let ty = child_by_field(n, "type")?;
        Some(CallHead {
            callee: cut_generics(text(cx, ty)),
            kind: CallKind::New,
            callee_root: None,
            args: call_args(cx, n),
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let kind = n.kind();
        if kind != "library_import" && kind != "library_export" {
            return;
        }
        let spec = if kind == "library_import" {
            match named_children(n)
                .into_iter()
                .find(|c| c.kind() == "import_specification")
            {
                Some(s) => s,
                None => return,
            }
        } else {
            n
        };
        let Some(module) = export_uri(cx, spec) else {
            return;
        };
        let mut emit = ImportEmit {
            module,
            pos: pos_of(n),
            in_test: cx.is_test_file,
            out,
        };
        if has_show(spec) {
            emit.show_all(cx, spec);
            return;
        }
        let alias = child_by_field(spec, "alias").map(|a| text(cx, a));
        let mut segs = module;
        if let Some(at) = module.rfind('/') {
            segs = &module[at + 1..];
        } else if let Some(at) = module.rfind(':') {
            segs = &module[at + 1..];
        }
        emit.namespace(segs, alias);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        let is_type = kind == "class_declaration"
            || kind == "mixin_declaration"
            || kind == "enum_declaration"
            || kind == "extension_type_declaration";
        if !is_type {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let mut public = !name.starts_with('_');
        match cx.containers.last() {
            None => {}
            Some(Container::Class { exported, .. }) => {
                public = public && *exported;
            }
            Some(Container::Trait { public: tp }) => {
                public = public && *tp;
            }
            Some(_) => {}
        }
        let abstraction = kind == "class_declaration"
            && (has_child_kind(n, "abstract") || has_child_kind(n, "interface"));
        let mut impls = Vec::new();
        for c in named_children(n) {
            if c.kind() == "interfaces" {
                iface_impls(cx, c, &mut impls);
            } else if c.kind() == "superclass" {
                super_impls(cx, c, &mut impls);
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
                exported: !name.starts_with('_'),
                implements: has_child_kind(n, "interfaces"),
            }),
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let kind = n.kind();
        let is_arm = kind == "switch_statement_case"
            || kind == "switch_statement_default"
            || kind == "switch_expression_case";
        if !is_arm {
            return None;
        }
        let pos = pos_of(n);
        if kind == "switch_statement_default" {
            return Some(WildcardArm {
                pos,
                enum_like: sibling_member(n),
            });
        }
        let first = named_children(n).into_iter().next()?;
        if trimmed(cx, first) != "_" {
            return None;
        }
        Some(WildcardArm {
            pos,
            enum_like: sibling_member(n),
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_clause" {
            return None;
        }
        let binding = child_by_field(n, "exception").map(|e| text(cx, e));
        let mut sib = n.next_sibling();
        let body = loop {
            match sib {
                None => return None,
                Some(s) if s.is_named() => break s,
                Some(s) => sib = s.next_sibling(),
            }
        };
        if body.kind() != "block" {
            return None;
        }
        let mut stmts = Vec::new();
        for s in named_children(body) {
            stmts.push(stmt_of(cx, s, binding));
        }
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "call_expression" {
            return None;
        }
        let args_node = child_by_field(n, "arguments")?;
        let kids = named_children(args_node);
        if kids.is_empty() {
            return None;
        }
        let mut source: Option<&'s str> = None;
        for a in &kids {
            let s = struct_arg(cx, *a)?;
            match source {
                None => source = Some(s),
                Some(prev) if prev == s => {}
                Some(_) => return None,
            }
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(kids.len()),
            source,
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        _in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "top_level_variable_declaration" {
            if has_anon(n, "final") || has_anon(n, "const") {
                return;
            }
            if child_by_field(n, "modifier").is_some_and(|m| {
                let t = text(cx, m);
                t == "final" || t == "const"
            }) {
                return;
            }
            if let Some((name, pos)) = top_name(cx, n) {
                out.push((AmbientKind::GlobalMutable, name, pos));
            }
            return;
        }
        if n.kind() != "member_expression" {
            return;
        }
        let (Some(o), Some(p)) = (child_by_field(n, "object"), child_by_field(n, "property"))
        else {
            return;
        };
        if o.kind() == "identifier"
            && text(cx, o) == "Platform"
            && p.kind() == "identifier"
            && text(cx, p) == "environment"
        {
            out.push((AmbientKind::EnvRead, "environment", pos_of(n)));
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        if func.kind() != "identifier" || text(cx, func) != "Symbol" {
            return None;
        }
        let args = child_by_field(n, "arguments")?;
        let first = named_children(args).into_iter().next()?;
        if arg_value(first).kind() == "string_literal" {
            return None;
        }
        Some("symbol built at runtime")
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() == "assert_statement" {
            let a = named_children(n)
                .into_iter()
                .find(|c| c.kind() == "assertion")?;
            let kids = named_children(a);
            return Some(Assertion {
                pos: pos_of(n),
                has_message: kids.len() >= 2,
                comparison: kids
                    .first()
                    .is_some_and(|k| k.kind() == "equality_expression"),
            });
        }
        if n.kind() != "call_expression" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        if func.kind() != "identifier" || text(cx, func) != "expect" {
            return None;
        }
        let args_node = child_by_field(n, "arguments")?;
        let mut has_message = false;
        let mut positional = 0u32;
        let mut second = None;
        for k in named_children(args_node) {
            if k.kind() == "named_argument" {
                has_message |= is_reason_arg(cx, k);
                continue;
            }
            positional += 1;
            if positional == 2 {
                second = Some(k);
            }
        }
        let comparison = second.is_some_and(|s| {
            s.kind() == "call_expression"
                && child_by_field(s, "function").is_some_and(|f| {
                    let c = cut_generics(text(cx, f));
                    c == "equals" || c == "same"
                })
        });
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison,
        })
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<sloplint_core::facts::Barrel> {
        if root.kind() != "source_file" {
            return None;
        }
        let mut exports = 0u32;
        let mut uris = Vec::new();
        for c in named_children(root) {
            if c.kind() == "library_export" {
                exports += 1;
                uris.extend(export_uri(cx, c));
            } else if !is_barrel_edge(c.kind()) {
                return None;
            }
        }
        if exports == 0 {
            return None;
        }
        let dir = match cx.path.rfind('/') {
            Some(at) => &cx.path[..at],
            None => "",
        };
        let mut targets = Vec::new();
        for uri in uris {
            if uri.starts_with("dart:") {
                continue;
            }
            if let Some(rest) = uri.strip_prefix("package:") {
                targets.push(vec![package_path(rest)]);
            } else {
                targets.push(vec![join_rel(dir, uri)]);
            }
        }
        Some(sloplint_core::facts::Barrel { targets })
    }
}

fn sibling_member(n: Node<'_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    named_children(parent)
        .into_iter()
        .filter(|s| s.id() != n.id())
        .any(arm_pattern_member)
}

fn struct_arg<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> Option<&'s str> {
    if a.kind() != "named_argument" {
        return None;
    }
    let parts = named_children(a);
    if parts.len() != 2 || parts[0].kind() != "label" {
        return None;
    }
    let label = label_text(cx, parts[0]);
    let val = parts[1];
    if val.kind() != "member_expression" {
        return None;
    }
    let (Some(o), Some(p)) = (
        child_by_field(val, "object"),
        child_by_field(val, "property"),
    ) else {
        return None;
    };
    if o.kind() != "identifier" || p.kind() != "identifier" || text(cx, p) != label {
        return None;
    }
    Some(text(cx, o))
}

fn is_barrel_edge(kind: &str) -> bool {
    kind == "library_import"
        || kind == "library_name"
        || kind == "part_directive"
        || kind == "part_of_directive"
        || kind == "comment"
        || kind == "block_comment"
        || kind == "documentation_block_comment"
}

fn package_path(rest: &str) -> String {
    match rest.find('/') {
        Some(i) => format!("lib/{}", &rest[i + 1..]),
        None => format!("lib/{rest}"),
    }
}
