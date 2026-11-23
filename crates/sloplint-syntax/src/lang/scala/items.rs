use sloplint_core::facts::{
    AmbientKind, CallKind, FieldCopy, Handler, ImplRef, Pos, ScrutineeKind, Stmt, TypeDef,
    Visibility, WildcardArm,
};
use tree_sitter::Node;

use super::calls::{call_identity, func_node};
use super::funcs::{in_test_dir, seg_last};
use crate::hooks::{
    Container, Cx, TypeHead, any_named_child, child_by_field, first_named_child, has_child_kind,
    named_children, narrow, pos_of, text, trimmed,
};

pub(crate) fn first_ident<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            return Some((unquote(text(cx, cur)), pos_of(cur)));
        }
        if cur.kind() != "wildcard" {
            stack.extend(named_children(cur).into_iter().rev());
        }
    }
    None
}

/// `` `name` `` binds the same variable as `name`: strip the backquotes so a
/// backquoted binder and an unquoted spelling resolve alike.
pub(crate) fn unquote(name: &str) -> &str {
    name.strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or(name)
}

pub(crate) fn collect_idents<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending = vec![n];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "wildcard" => {}
            // A val/var pattern's top-level identifier is always a binder,
            // even spelled `Count` or `` `a-b` ``; the case/backquote rules
            // only govern nested positions. In a comma-separated list the
            // direct children of the `identifiers` pattern are all top-level
            // binders too. Only binder positions count: `counters` in
            // `Some(counters.Zero)` is a stable reference, not a new local.
            "identifier" | "operator_identifier"
                if !is_value_name(node)
                    && (node.id() == n.id()
                        || (n.kind() == "identifiers"
                            && node.parent().is_some_and(|p| p.id() == n.id()))
                        || is_pattern_binder(cx, node)) =>
            {
                out.push((unquote(text(cx, node)), pos_of(node)))
            }
            "identifier" | "operator_identifier" => {}
            _ => pending.extend(named_children(node).into_iter().rev()),
        }
    }
}

fn binding_of<'s>(cx: &Cx<'_, 's>, pat: Option<Node<'_>>) -> Option<&'s str> {
    let mut current = pat?;
    loop {
        if current.kind() == "identifier" {
            return Some(unquote(text(cx, current)));
        }
        if current.kind() == "capture_pattern" {
            return child_by_field(current, "name").map(|m| unquote(text(cx, m)));
        }
        if current.kind() != "typed_pattern" {
            return None;
        }
        let inner = child_by_field(current, "pattern")?;
        if inner.kind() == "identifier" {
            return Some(unquote(text(cx, inner)));
        }
        current = inner;
    }
}

fn stmt_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: Option<&'s str>) -> Stmt<'s> {
    if s.kind() == "throw_expression" {
        let hit = any_named_child(s, |c| {
            c.kind() == "identifier" && Some(unquote(text(cx, c))) == binding
        });
        return if hit { Stmt::Rethrow } else { Stmt::Other };
    }
    if s.kind() != "call_expression" {
        return Stmt::Other;
    }
    match call_identity(cx, s).map(|(callee, _, _)| callee) {
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

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "var_definition" && n.kind() != "val_definition" {
        return;
    }
    // `var whole, Some(count) = …` repeats the `pattern` field, and the
    // field iteration also yields the anonymous `,` separators — so walk
    // every named field child instead of `child_by_field`'s first match.
    let mut cursor = n.walk();
    for pat in n
        .children_by_field_name("pattern", &mut cursor)
        .filter(|pat| pat.is_named())
    {
        collect_idents(cx, pat, out);
    }
}

pub(crate) fn shadow_decl_kind(kind: &str) -> bool {
    matches!(kind, "for_expression" | "case_clause")
}

// Whether `name` is a type alias visible where `node` sits: some enclosing
// scope defines `type <name>` before the node. Term and type namespaces
// coexist, so a `var status` beside `type status` does not hide the alias.
fn names_visible_type_alias(node: Node<'_>, cx: &Cx<'_, '_>, name: &str) -> bool {
    let mut scope = node.parent();
    while let Some(s) = scope {
        scope = s.parent();
        for c in named_children(s) {
            if c.kind() != "type_definition" {
                continue;
            }
            let Some(alias) = child_by_field(c, "name") else {
                continue;
            };
            if unquote(text(cx, alias)) == name && c.start_byte() < node.start_byte() {
                return true;
            }
        }
    }
    false
}

// `case given (Int, String)` is a given of tuple type, not a `given(...)`
// extractor: every pattern child is a type name, so nothing binds and the
// arm is not a variant constructor. `case given(x, y)` and
// `case given[Int](box, _)` are extractor applications whose lowercase
// arguments bind locals — unless the name is a visible type alias, which
// capitalization alone cannot tell from a fresh binder.
fn is_given_tuple_type<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> bool {
    if node.kind() != "case_class_pattern"
        || !child_by_field(node, "type").is_some_and(|t| text(cx, t) == "given")
    {
        return false;
    }
    let mut cursor = node.walk();
    let patterns: Vec<_> = node
        .children_by_field_name("pattern", &mut cursor)
        .filter(|p| p.is_named())
        .collect();
    if patterns.is_empty() {
        // `case given()` applies an empty extractor: it binds nothing, but
        // the arm still is a variant constructor. A tuple type needs at
        // least one member, so emptiness can never be a tuple.
        return false;
    }
    if patterns.len() == 1 && patterns[0].kind() == "tuple_type" {
        return true;
    }
    patterns.iter().all(|p| {
        if p.kind() == "type_identifier" {
            return true;
        }
        if p.kind() != "identifier" {
            return false;
        }
        let name = unquote(text(cx, *p));
        name.chars().next().is_some_and(|f| f.is_uppercase())
            || names_visible_type_alias(*p, cx, name)
    })
}

// `case given(x, y)` is an extractor application in Scala 2: the parser
// reads every `given(...)` as a `given_pattern` (upstream prefers that
// reading), but an adjacent paren holding only lowercase or
// underscore-prefixed, non-alias names is the old extractor call, whose
// arguments bind locals. A wildcard member binds nothing and is skipped, so
// `case given(x, _)` still binds `x`. A spaced paren, a plain type, or any
// other member is a tuple or plain type and binds nothing — the same rule
// as `is_given_tuple_type`.
fn given_extractor_binders<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> Option<Vec<&'s str>> {
    if node.kind() != "given_pattern" {
        return None;
    }
    let t = child_by_field(node, "type")?;
    if t.kind() != "tuple_type" {
        return None;
    }
    // An adjacent paren applies the extractor (`given(`); a spaced paren
    // ascribes a tuple type (`given (`).
    let kw_end = node
        .child(0)
        .filter(|k| k.kind() == "given")
        .map(|k| k.end_byte())?;
    if t.start_byte() != kw_end {
        return None;
    }
    let mut out = Vec::new();
    for p in named_children(t) {
        if p.kind() == "wildcard" {
            continue;
        }
        if p.kind() != "type_identifier" {
            return None;
        }
        let raw = text(cx, p);
        if raw.starts_with('`') {
            return None;
        }
        let name = unquote(raw);
        let first = name.chars().next()?;
        if !(first.is_lowercase() || first == '_') || names_visible_type_alias(p, cx, name) {
            return None;
        }
        out.push(name);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

fn push_pattern_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<&'s str>) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if is_given_tuple_type(cx, node) {
            // `case given (Int, String)` is a given of tuple type, not a
            // `given(...)` extractor; its members bind no locals. A real
            // extractor — `case given(x, y)` or `case given[Int](box, _)` —
            // keeps its lowercase arguments as binders.
            continue;
        }
        // `case given T` is a given-pattern matching an implicit instance —
        // nothing inside the type binds a local. `case given(x, y)` is the
        // Scala 2 extractor application and binds its lowercase arguments.
        if node.kind() == "given_pattern" {
            if let Some(names) = given_extractor_binders(cx, node) {
                out.extend(names);
            }
            continue;
        }
        if node.kind() == "identifier" {
            if is_pattern_binder(cx, node) {
                out.push(text(cx, node));
            }
        } else if node.kind() == "named_pattern" {
            // `Doc(_id = id)`: the label selects a field — it is not a
            // binder, so only the right-hand pattern is walked.
            pending.extend(named_children(node).into_iter().skip(1).rev());
        } else if node.kind() != "wildcard" {
            pending.extend(named_children(node).into_iter().rev());
        }
    }
}

fn is_pattern_binder(cx: &Cx<'_, '_>, node: Node<'_>) -> bool {
    // A name before `@` (`x @ Some(y)`, `` `x` @ ``, `X @ ``) is always a
    // binder — the capture's name is never a stable reference.
    if node.parent().is_some_and(|p| {
        p.kind() == "capture_pattern"
            && child_by_field(p, "name").is_some_and(|m| m.id() == node.id())
    }) {
        return true;
    }
    let name = text(cx, node);
    if name.starts_with('`')
        || !name
            .chars()
            .next()
            .is_some_and(|first| first.is_lowercase() || first == '_')
    {
        return false;
    }
    // The raw tree parent: `stable_identifier` and friends are not tracked
    // levels, so `cx.parent_of` would surface an ancestor instead.
    let Some(parent) = node.parent() else {
        return true;
    };
    !matches!(
        parent.kind(),
        "type_identifier" | "stable_identifier" | "stable_type_identifier"
    )
}

// An identifier that names a value slot but binds nothing: the interpolator
// head of `s"..."` and a word-form infix operator (`a foo b`). Neither is a
// local, and registering them steals writes from real variables.
fn is_value_name(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if matches!(
        parent.kind(),
        "interpolated_string" | "interpolated_string_expression"
    ) {
        return true;
    }
    parent.kind() == "infix_expression"
        && child_by_field(parent, "operator").is_some_and(|o| o.id() == node.id())
}

fn push_generator_pattern<'s>(cx: &Cx<'_, 's>, enumerator: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(head) = first_named_child(enumerator) else {
        return;
    };
    if head.kind() == "guard" {
        return;
    }
    push_pattern_names(cx, head, out);
}

fn push_for_binders<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    for holder in named_children(n) {
        if holder.kind() != "enumerators" {
            continue;
        }
        for enumerator in named_children(holder) {
            if enumerator.kind() != "enumerator" {
                continue;
            }
            push_generator_pattern(cx, enumerator, out);
        }
    }
}

pub(crate) fn shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "for_expression" => push_for_binders(cx, n, out),
        "case_clause" => {
            if let Some(pat) = child_by_field(n, "pattern") {
                push_pattern_names(cx, pat, out);
            }
        }
        _ => {}
    }
}

fn extend_info<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> (Vec<ImplRef<'s>>, bool) {
    let mut impls = Vec::new();
    let mut testy = in_test_dir(cx.path);
    for t in named_children(c) {
        if t.kind() == "arguments" {
            continue;
        }
        let ttext = trimmed(cx, t);
        testy |= super::funcs::is_suite_base(seg_last(ttext));
        impls.push(ImplRef {
            name: seg_last(ttext),
            pos: pos_of(t),
            is_generic: ttext.contains('['),
            child_is_abstract: false,
        });
    }
    (impls, testy)
}

fn type_vis<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Visibility {
    let restricted = any_named_child(n, |c| {
        c.kind() == "access_modifier"
            || (c.kind() == "modifiers" && has_child_kind(c, "access_modifier"))
    });
    if !restricted {
        return Visibility::Exported;
    }
    let scoped = any_named_child(n, |c| {
        (c.kind() == "access_modifier" && text(cx, c).contains('['))
            || (c.kind() == "modifiers"
                && named_children(c)
                    .iter()
                    .any(|m| m.kind() == "access_modifier" && text(cx, *m).contains('[')))
    });
    if scoped {
        Visibility::Restricted
    } else {
        Visibility::Private
    }
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
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
    let mut vis = type_vis(cx, n);
    match cx.innermost_container() {
        None => {}
        Some(Container::Class { exported, .. }) if !exported => vis = Visibility::Private,
        Some(Container::Trait { public }) if !public => vis = Visibility::Private,
        _ => {}
    }
    let abstraction = kind == "trait_definition"
        || any_named_child(n, |c| {
            c.kind() == "modifiers" && has_child_kind(c, "abstract")
        });
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
        Some(Container::Trait {
            public: vis != Visibility::Private,
        })
    } else {
        Some(Container::Class {
            exported: vis == Visibility::Exported,
            implements: !impls.is_empty(),
        })
    };
    Some(TypeHead {
        def: Some(TypeDef {
            name,
            pos: pos_of(name_node),
            visibility: vis,
            abstraction,
        }),
        impls,
        container,
    })
}

fn match_scrutinee(cx: &Cx<'_, '_>, _n: Node<'_>) -> ScrutineeKind {
    if cx.parent().is_none() {
        return ScrutineeKind::Other;
    }
    let Some(m) = cx.ancestors().nth(1) else {
        return ScrutineeKind::Other;
    };
    if m.kind() != "match_expression" {
        return ScrutineeKind::Other;
    }
    match child_by_field(m, "value").map(|v| v.kind()) {
        Some("identifier") => ScrutineeKind::Ident,
        Some("field_expression") => ScrutineeKind::Field,
        _ => ScrutineeKind::Other,
    }
}

fn bump_prefix<'s>(groups: &mut Vec<(&'s str, u32)>, prefix: &'s str) {
    if let Some(entry) = groups.iter_mut().find(|(held, _)| *held == prefix) {
        entry.1 += 1;
    } else {
        groups.push((prefix, 1));
    }
}

pub(crate) fn match_arm<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<WildcardArm> {
    if n.kind() != "case_clause" {
        return None;
    }
    let pos = pos_of(n);
    if child_by_field(n, "pattern").is_none_or(|p| p.kind() != "wildcard") {
        return None;
    }
    let Some(parent) = cx.parent() else {
        return Some(WildcardArm {
            pos,
            enum_like: false,
            scrutinee: ScrutineeKind::Other,
            variant_count: 0,
            has_nullable_ctor: false,
        });
    };
    let mut variants = 0u32;
    let mut case_class_count = 0u32;
    let mut stable_groups: Vec<(&'s str, u32)> = Vec::new();
    let mut nullable = false;
    for s in named_children(parent) {
        if s.id() == n.id() || s.kind() != "case_clause" {
            continue;
        }
        let Some(p) = child_by_field(s, "pattern") else {
            continue;
        };
        if p.kind() == "stable_identifier" {
            variants += 1;
            let prefix = trimmed(cx, p)
                .rsplit_once('.')
                .map(|(head, _)| head)
                .unwrap_or("");
            bump_prefix(&mut stable_groups, prefix);
        } else if p.kind() == "case_class_pattern" && !is_given_tuple_type(cx, p) {
            // A `given (Int, String)` arm matches a tuple-typed instance;
            // it is not a variant constructor and must not inflate the
            // enum-like variant count.
            variants += 1;
            case_class_count += 1;
        } else if p.kind() == "given_pattern" && given_extractor_binders(cx, p).is_some() {
            // `case given(value)` is a Scala 2 extractor application, not a
            // given of tuple type: it is a variant constructor like a case
            // class pattern. Genuine Scala 3 given types (`case given T`,
            // `case given (Int, String)`) bind nothing and stay out of the
            // count.
            variants += 1;
            case_class_count += 1;
        }
        if matches!(seg_last(trimmed(cx, p)), "Some" | "None" | "Ok" | "Err") {
            nullable = true;
        }
    }
    let stable_best = stable_groups
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0);
    Some(WildcardArm {
        pos,
        enum_like: case_class_count >= 2 || stable_best >= 2,
        scrutinee: match_scrutinee(cx, n),
        variant_count: variants,
        has_nullable_ctor: nullable,
    })
}

pub(crate) fn handler_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() == "catch_clause" {
        let binding = binding_of(cx, child_by_field(n, "pattern"));
        let body = child_by_field(n, "body")?;
        return Some(Handler {
            pos: pos_of(n),
            stmts: body_stmts(cx, body, binding),
            has_else: false,
            try_imports_only: false,
            in_cleanup: false,
        });
    }
    if n.kind() != "case_clause" {
        return None;
    }
    let parent = cx.parent()?;
    if parent.kind() != "case_block" && parent.kind() != "indented_cases" {
        return None;
    }
    if cx
        .ancestors()
        .nth(1)
        .is_none_or(|g| g.kind() != "catch_clause")
    {
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
        has_else: false,
        try_imports_only: false,
        in_cleanup: false,
    })
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

pub(crate) fn struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func_node(func).unwrap_or(func).kind() != "identifier" {
        return None;
    }
    let kids = super::calls::call_arg_nodes(n);
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

pub(crate) fn ambient_head<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
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
    if in_function
        && (n.kind() == "assignment_expression" || super::writes::is_compound_write(cx, n))
    {
        super::writes::report_assign_write(cx, n, out);
        return;
    }
    if n.kind() == "field_expression" {
        let (Some(v), Some(f)) = (child_by_field(n, "value"), child_by_field(n, "field")) else {
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
        let label = super::calls::call_first_arg(n)
            .map(|a| text(cx, a))
            .unwrap_or_else(|| text(cx, f));
        out.push((AmbientKind::EnvRead, label, pos_of(n)));
    }
}
