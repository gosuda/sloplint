use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, has_child_kind,
    named_children, narrow, pos_of, preceding_comments, slice, text, trimmed,
};
use crate::spec::Role;

/// Node-kind roles for the Scala grammar.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_expression", Role::If),
    ("match_expression", Role::Match),
    ("while_expression", Role::Loop),
    ("do_while_expression", Role::Loop),
    ("for_expression", Role::Loop),
    ("catch_clause", Role::Catch),
    ("call_expression", Role::Call),
    ("instance_expression", Role::NewCall),
    ("var_definition", Role::Decl),
    ("assignment_expression", Role::Write),
    ("import_declaration", Role::Import),
    ("export_declaration", Role::Export),
    ("class_definition", Role::TypeDef),
    ("object_definition", Role::TypeDef),
    ("trait_definition", Role::TypeDef),
    ("enum_definition", Role::TypeDef),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("case_clause", Role::MatchArm),
    ("field_expression", Role::Member),
];

/// Hooks reading Scala definitions, calls, imports, and handlers.
pub(crate) static HOOKS: ScalaHooks = ScalaHooks;

/// Unit hooks for Scala.
pub(crate) struct ScalaHooks;

const REFLECT_NAMES: &[&str] = &[
    "getMethod",
    "getDeclaredMethod",
    "getField",
    "getDeclaredField",
    "forName",
];
const ASSERT_CMP_OPS: &[&str] = &["==", "===", "!=", "!=="];

fn cut_at(s: &str, ch: char) -> &str {
    match s.find(ch) {
        Some(i) => &s[..i],
        None => s,
    }
}

fn seg_last(t: &str) -> &str {
    let base = cut_at(cut_at(t, '<'), '[');
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

fn func_node(n: Node<'_>) -> Option<Node<'_>> {
    let mut func = child_by_field(n, "function")?;
    if func.kind() == "generic_function" {
        func = child_by_field(func, "function")?;
    }
    Some(func)
}

fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    match n.kind() {
        "identifier" => Some(text(cx, n)),
        "field_expression" => callee_root(cx, child_by_field(n, "value")?),
        _ => None,
    }
}

fn func_root<'s>(cx: &Cx<'_, 's>, f: Node<'_>) -> Option<&'s str> {
    callee_root(cx, func_node(f).unwrap_or(f))
}

fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    match n.kind() {
        "identifier" => Some((text(cx, n), pos_of(n))),
        "field_expression" => write_root(cx, child_by_field(n, "value")?),
        "parenthesized_expression" => write_root(cx, named_children(n).into_iter().next()?),
        _ => None,
    }
}

fn gaps<'s>(cx: &Cx<'_, 's>, n: Node<'_>, gap_kind: &str) -> Vec<&'s str> {
    let t = text(cx, n);
    let open = if t.starts_with("\"\"\"") { 3 } else { 1 };
    let mut cursor = n.start_byte() + open;
    let close_at = n.end_byte().saturating_sub(open);
    let mut segs = Vec::new();
    let mut i = 0u32;
    while i < n.child_count() {
        match n.child(i) {
            Some(c) if c.kind() == gap_kind => {
                segs.push(slice(cx.src, cursor, c.start_byte()));
                cursor = c.end_byte();
            }
            _ => {}
        }
        i += 1;
    }
    segs.push(slice(cx.src, cursor, close_at));
    segs
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> ArgShape<'s> {
    if a.kind() == "string" {
        return ArgShape::Str {
            longest_static: longest_static_chunk(&gaps(cx, a, "escape_sequence")),
        };
    }
    if a.kind() == "interpolated_string_expression" {
        let inner = named_children(a)
            .into_iter()
            .find(|c| c.kind() == "interpolated_string")
            .unwrap_or(a);
        return ArgShape::Str {
            longest_static: longest_static_chunk(&gaps(cx, inner, "interpolation")),
        };
    }
    if a.kind() == "infix_expression" && infix_concat(cx, a) {
        return ArgShape::Concat;
    }
    if a.kind() == "identifier" {
        return ArgShape::Ident(text(cx, a));
    }
    ArgShape::Other
}

fn infix_concat(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if child_by_field(n, "operator")
        .map(|o| text(cx, o))
        .unwrap_or("")
        != "+"
    {
        return false;
    }
    [child_by_field(n, "left"), child_by_field(n, "right")]
        .into_iter()
        .flatten()
        .any(|o| o.kind() == "string" || o.kind() == "interpolated_string_expression")
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    let Some(list) = child_by_field(n, "arguments") else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .map(|a| Arg {
            shape: arg_shape(cx, a),
            start_line: narrow(a.start_position().row) + 1,
            end_line: narrow(a.end_position().row) + 1,
        })
        .collect()
}

fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|l| named_children(l).into_iter().next())
}

fn sole_call_of(body: Node<'_>) -> Option<usize> {
    if body.kind() == "call_expression" {
        return Some(body.id());
    }
    if body.kind() != "block" && body.kind() != "indented_block" {
        return None;
    }
    let mut sole = None;
    let mut count = 0u32;
    for c in named_children(body) {
        if c.kind() == "comment" || c.kind() == "block_comment" {
            continue;
        }
        count += 1;
        sole = Some(c);
    }
    let s = sole.filter(|_| count == 1)?;
    let mut target = s;
    if target.kind() == "return_expression" {
        target = named_children(target).into_iter().next()?;
    }
    (target.kind() == "call_expression").then_some(target.id())
}

fn first_ident<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            return Some((text(cx, cur), pos_of(cur)));
        }
        if cur.kind() != "wildcard" {
            stack.extend(named_children(cur).into_iter().rev());
        }
    }
    None
}

fn collect_idents<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "wildcard" {
        return;
    }
    if n.kind() == "identifier" {
        out.push((text(cx, n), pos_of(n)));
        return;
    }
    for c in named_children(n) {
        collect_idents(cx, c, out);
    }
}

fn binding_of<'s>(cx: &Cx<'_, 's>, pat: Option<Node<'_>>) -> Option<&'s str> {
    let p = pat?;
    if p.kind() == "identifier" {
        return Some(text(cx, p));
    }
    if p.kind() == "capture_pattern" {
        return child_by_field(p, "name").map(|m| text(cx, m));
    }
    if p.kind() != "typed_pattern" {
        return None;
    }
    let inner = child_by_field(p, "pattern")?;
    if inner.kind() == "identifier" {
        Some(text(cx, inner))
    } else {
        binding_of(cx, Some(inner))
    }
}

fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "throw_expression" {
        let hit = named_children(s)
            .into_iter()
            .any(|c| c.kind() == "identifier" && Some(text(cx, c)) == binding);
        return if hit { Stmt::Rethrow } else { Stmt::Other };
    }
    if s.kind() != "call_expression" {
        return Stmt::Other;
    }
    match func_node(s).map(|f| text(cx, f)) {
        Some(callee) => Stmt::Call {
            callee,
            kind: CallKind::Plain,
        },
        None => Stmt::Other,
    }
}

fn body_stmts<'s>(cx: &Cx<'_, 's>, body: Node<'_>, binding: Option<&'s str>) -> Vec<Stmt<'s>> {
    if body.kind() == "indented_block" {
        named_children(body)
            .into_iter()
            .filter(|c| c.kind() != "comment" && c.kind() != "block_comment")
            .map(|s| stmt_of(cx, s, binding))
            .collect()
    } else {
        vec![stmt_of(cx, body, binding)]
    }
}

fn params_from<'s>(cx: &Cx<'_, 's>, c: Node<'_>, out: &mut Vec<Param<'s>>) {
    for p in named_children(c) {
        if p.kind() != "parameter" {
            continue;
        }
        let Some(nm) = child_by_field(p, "name") else {
            continue;
        };
        out.push(Param {
            name: text(cx, nm),
            ty: child_by_field(p, "type").map(|t| text(cx, t)),
            bool_default: child_by_field(p, "default_value").is_some_and(|d| {
                d.kind() == "boolean_literal"
                    && (trimmed(cx, d) == "true" || trimmed(cx, d) == "false")
            }),
        });
    }
}

fn import_path<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<&'s str> {
    let mut segs = Vec::new();
    let mut i = 0u32;
    while i < n.child_count() {
        let Some(c) = n.child(i) else {
            i += 1;
            continue;
        };
        match c.kind() {
            "identifier" | "operator_identifier" => segs.push(text(cx, c)),
            "namespace_selectors"
            | "namespace_wildcard"
            | "as_renamed_identifier"
            | "arrow_renamed_identifier" => break,
            _ => {}
        }
        i += 1;
    }
    segs
}

#[derive(Clone, Copy)]
struct Head {
    pos: Pos,
    in_test: bool,
}

struct Emit<'a, 's> {
    module: String,
    wild: &'s str,
    head: Head,
    out: &'a mut Vec<Import<'s>>,
}

impl<'a, 's> Emit<'a, 's> {
    fn named(&mut self, name: &'s str, alias: Option<&'s str>) {
        self.out.push(Import {
            pos: self.head.pos,
            module: Cow::Owned(self.module.clone()),
            name,
            alias,
            kind: ImportKind::Named,
            in_test: self.head.in_test,
        });
    }
    fn wild(&mut self) {
        self.out.push(Import {
            pos: self.head.pos,
            module: Cow::Owned(self.module.clone()),
            name: self.wild,
            alias: None,
            kind: ImportKind::Wildcard,
            in_test: self.head.in_test,
        });
    }
    fn renamed(&mut self, cx: &Cx<'_, 's>, s: Node<'_>) {
        let name = child_by_field(s, "name").map(|m| text(cx, m));
        let alias = child_by_field(s, "alias").map(|m| text(cx, m));
        match (name, alias) {
            (Some(nn), Some(aa)) => self.named(
                nn,
                if aa == "_" || aa == nn {
                    None
                } else {
                    Some(aa)
                },
            ),
            (Some(nn), None) => self.named(nn, None),
            _ => {}
        }
    }
    fn selectors(&mut self, cx: &Cx<'_, 's>, sel: Node<'_>) {
        for s in named_children(sel) {
            if s.kind() == "identifier" {
                self.named(text(cx, s), None);
            } else if s.kind() == "as_renamed_identifier" {
                self.renamed(cx, s);
            } else if s.kind() == "namespace_wildcard" {
                self.wild();
            }
        }
    }
}

fn extend_info<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> (Vec<ImplRef<'s>>, bool) {
    let mut impls = Vec::new();
    let mut testy = false;
    for t in named_children(c) {
        if t.kind() == "arguments" {
            continue;
        }
        let ttext = trimmed(cx, t);
        testy |= ttext.ends_with("Suite") || ttext.ends_with("Spec");
        impls.push(ImplRef {
            name: seg_last(ttext),
            pos: pos_of(t),
        });
    }
    (impls, testy)
}

impl LangHooks for ScalaHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        if n.kind() != "function_definition" {
            return None;
        }
        let body = child_by_field(n, "body")?;
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let mods = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "modifiers");
        let mut public = !mods.is_some_and(|m| has_child_kind(m, "access_modifier"));
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
        let interface_bound = matches!(
            cx.containers.last(),
            Some(Container::Class {
                implements: true,
                ..
            })
        ) && mods.is_some_and(|m| has_child_kind(m, "override"));
        let in_test = cx.is_test_file || cx.containers.contains(&Container::TestModule);
        let mut params = Vec::new();
        for c in named_children(n) {
            if c.kind() == "parameters" {
                params_from(cx, c, &mut params);
            }
        }
        let return_type = child_by_field(n, "return_type").map(|t| text(cx, t));
        let mut missing = Vec::new();
        for p in &params {
            if p.ty.is_none() {
                missing.push(p.name);
            }
        }
        if return_type.is_none() && name != "this" {
            missing.push("return");
        }
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment", "block_comment"], &[]);
        Some(FunctionHead {
            name,
            name_pos: pos_of(name_node),
            public,
            interface_bound,
            in_test,
            params,
            return_type,
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: sole_call_of(body),
        })
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        if n.kind() != "if_expression" {
            return false;
        }
        let Some(p) = n.parent() else {
            return false;
        };
        if p.kind() != "if_expression" {
            return false;
        }
        p.child_by_field_name("alternative")
            .is_some_and(|a| a.id() == n.id())
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "var_definition" {
            return;
        }
        if let Some(pat) = child_by_field(n, "pattern") {
            collect_idents(cx, pat, out);
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment_expression" {
            return;
        }
        if let Some(l) = child_by_field(n, "left") {
            out.extend(write_root(cx, l));
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "call_expression" {
            let func = child_by_field(n, "function")?;
            return Some(CallHead {
                callee: text(cx, func_node(func).unwrap_or(func)),
                kind: CallKind::Plain,
                callee_root: func_root(cx, func),
                args: call_args(cx, n),
            });
        }
        if n.kind() != "instance_expression" {
            return None;
        }
        let ty = named_children(n).into_iter().find(|c| {
            c.kind() == "type_identifier"
                || c.kind() == "stable_type_identifier"
                || c.kind() == "generic_type"
        })?;
        Some(CallHead {
            callee: cut_at(text(cx, ty), '['),
            kind: CallKind::New,
            callee_root: None,
            args: call_args(cx, n),
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let kind = n.kind();
        if kind != "import_declaration" && kind != "export_declaration" {
            return;
        }
        let head = Head {
            pos: pos_of(n),
            in_test: cx.is_test_file,
        };
        let path = import_path(cx, n);
        let mut emit = Emit {
            module: path.join("."),
            wild: path.last().copied().unwrap_or(""),
            head,
            out,
        };
        let mut handled = false;
        for c in named_children(n) {
            if c.kind() == "namespace_wildcard" {
                handled = true;
                emit.wild();
            } else if c.kind() == "namespace_selectors" {
                handled = true;
                emit.selectors(cx, c);
            } else if c.kind() == "as_renamed_identifier" {
                handled = true;
                emit.renamed(cx, c);
            }
        }
        if handled {
            return;
        }
        let name = path.last().copied().unwrap_or("");
        let lead = if path.is_empty() {
            String::new()
        } else {
            path[..path.len() - 1].join(".")
        };
        emit.out.push(Import {
            pos: emit.head.pos,
            module: Cow::Owned(lead),
            name,
            alias: None,
            kind: ImportKind::Named,
            in_test: emit.head.in_test,
        });
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let kind = n.kind();
        let is_type = kind == "class_definition"
            || kind == "object_definition"
            || kind == "trait_definition"
            || kind == "enum_definition";
        if !is_type {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let restricted = named_children(n).into_iter().any(|c| {
            c.kind() == "access_modifier"
                || (c.kind() == "modifiers" && has_child_kind(c, "access_modifier"))
        });
        let mut public = !restricted;
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
        let abstract_mod = named_children(n)
            .into_iter()
            .any(|c| c.kind() == "modifiers" && has_child_kind(c, "abstract"));
        let abstraction = kind == "trait_definition" || abstract_mod;
        let mut impls = Vec::new();
        let mut testy = false;
        for c in named_children(n) {
            if c.kind() == "extends_clause" {
                let (more, suite) = extend_info(cx, c);
                impls.extend(more);
                testy |= suite;
            }
        }
        let container = if testy {
            Some(Container::TestModule)
        } else if kind == "trait_definition" {
            Some(Container::Trait { public })
        } else {
            Some(Container::Class {
                exported: public,
                implements: !impls.is_empty(),
            })
        };
        Some(TypeHead {
            def: Some(TypeDef {
                name,
                pos: pos_of(name_node),
                public,
                abstraction,
            }),
            impls,
            container,
        })
    }

    fn match_arm(&self, n: Node<'_>, _cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "case_clause" {
            return None;
        }
        let pos = pos_of(n);
        if child_by_field(n, "pattern").is_none_or(|p| p.kind() != "wildcard") {
            return None;
        }
        let Some(parent) = n.parent() else {
            return Some(WildcardArm {
                pos,
                enum_like: false,
            });
        };
        let enum_like = named_children(parent)
            .into_iter()
            .filter(|s| s.id() != n.id() && s.kind() == "case_clause")
            .filter_map(|s| child_by_field(s, "pattern"))
            .any(|p| p.kind() == "stable_identifier" || p.kind() == "case_class_pattern");
        Some(WildcardArm { pos, enum_like })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() == "catch_clause" {
            let binding = binding_of(cx, child_by_field(n, "pattern"));
            let body = child_by_field(n, "body")?;
            return Some(Handler {
                pos: pos_of(n),
                stmts: body_stmts(cx, body, binding),
            });
        }
        if n.kind() != "case_clause" {
            return None;
        }
        let parent = n.parent()?;
        if parent.kind() != "case_block" && parent.kind() != "indented_cases" {
            return None;
        }
        if parent.parent().is_none_or(|g| g.kind() != "catch_clause") {
            return None;
        }
        let binding = binding_of(cx, child_by_field(n, "pattern"));
        let pat_id = child_by_field(n, "pattern").map(|p| p.id());
        let mut stmts = Vec::new();
        for c in named_children(n) {
            if Some(c.id()) != pat_id && c.kind() != "guard" {
                stmts.push(stmt_of(cx, c, binding));
            }
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
        let mut first: Option<&'s str> = None;
        let mut uniform = true;
        for a in &kids {
            let s = struct_field(cx, *a)?;
            match first {
                None => first = Some(s),
                Some(prev) if prev == s => {}
                Some(_) => uniform = false,
            }
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(kids.len()),
            source: if uniform { first } else { None },
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "var_definition" && !in_function {
            if let Some(pat) = child_by_field(n, "pattern") {
                out.extend(
                    first_ident(cx, pat).map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
                );
            }
            return;
        }
        if n.kind() == "field_expression" {
            let (Some(v), Some(f)) = (child_by_field(n, "value"), child_by_field(n, "field"))
            else {
                return;
            };
            if v.kind() == "identifier" && text(cx, v) == "sys" && text(cx, f) == "env" {
                out.push((AmbientKind::EnvRead, "env", pos_of(n)));
            }
            return;
        }
        if n.kind() != "call_expression" {
            return;
        }
        let Some(f) = child_by_field(n, "function") else {
            return;
        };
        if f.kind() != "field_expression" {
            return;
        }
        let (Some(v), Some(m)) = (child_by_field(f, "value"), child_by_field(f, "field")) else {
            return;
        };
        if text(cx, v) == "System" && text(cx, m) == "getenv" {
            let label = first_arg(n)
                .map(|a| text(cx, a))
                .unwrap_or_else(|| text(cx, f));
            out.push((AmbientKind::EnvRead, label, pos_of(n)));
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = func_node(n)?;
        if !REFLECT_NAMES.contains(&seg_last(text(cx, func))) {
            return None;
        }
        if first_arg(n).is_none_or(|a| a.kind() == "string") {
            return None;
        }
        Some("reflection lookup by computed name")
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let func = func_node(n)?;
        match seg_last(text(cx, func)) {
            "assert" => {
                let cmp = first_arg(n).is_some_and(|f| {
                    f.kind() == "infix_expression"
                        && child_by_field(f, "operator")
                            .is_some_and(|o| ASSERT_CMP_OPS.contains(&text(cx, o)))
                });
                let count = narrow(first_arg_count(n));
                Some(Assertion {
                    pos: pos_of(n),
                    has_message: count >= 2,
                    comparison: cmp,
                })
            }
            "assertResult" => Some(Assertion {
                pos: pos_of(n),
                has_message: narrow(first_arg_count(n)) >= 2,
                comparison: true,
            }),
            "assertEquals" | "assertNotEquals" => Some(Assertion {
                pos: pos_of(n),
                has_message: narrow(first_arg_count(n)) >= 3,
                comparison: true,
            }),
            _ => None,
        }
    }
}

fn first_arg_count(n: Node<'_>) -> usize {
    child_by_field(n, "arguments")
        .map(|l| named_children(l).len())
        .unwrap_or(0)
}

fn struct_field<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> Option<&'s str> {
    if a.kind() != "assignment_expression" {
        return None;
    }
    let (Some(l), Some(r)) = (child_by_field(a, "left"), child_by_field(a, "right")) else {
        return None;
    };
    if l.kind() != "identifier" || r.kind() != "field_expression" {
        return None;
    }
    let (Some(v), Some(f)) = (child_by_field(r, "value"), child_by_field(r, "field")) else {
        return None;
    };
    if v.kind() != "identifier" || text(cx, f) != text(cx, l) {
        return None;
    }
    Some(text(cx, v))
}
