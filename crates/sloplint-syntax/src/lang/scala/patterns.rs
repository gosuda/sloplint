use tree_sitter::Node;

use super::items::unquote;
use crate::hooks::{Cx, child_by_field, first_named_child, named_children, text};

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
pub(crate) fn is_given_tuple_type<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> bool {
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
pub(crate) fn given_extractor_binders<'s>(cx: &Cx<'_, 's>, node: Node<'_>) -> Option<Vec<&'s str>> {
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

pub(crate) fn is_pattern_binder(cx: &Cx<'_, '_>, node: Node<'_>) -> bool {
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
pub(crate) fn is_value_name(node: Node<'_>) -> bool {
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