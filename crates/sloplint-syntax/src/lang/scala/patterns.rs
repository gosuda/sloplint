use tree_sitter::Node;

use super::items::unquote;
use crate::hooks::{Cx, child_by_field, first_named_child, named_children, text};

// Whether `name` is a type alias visible where `node` sits: some enclosing
// scope defines `type <name>` before the node, or imports it from a
// same-file `object`/`class`/`trait`. Term and type namespaces coexist, so
// a `var status` beside `type status` does not hide the alias.
fn names_visible_type_alias(node: Node<'_>, cx: &Cx<'_, '_>, name: &str) -> bool {
    let mut scope = node.parent();
    while let Some(s) = scope {
        scope = s.parent();
        for c in named_children(s) {
            if c.kind() == "type_definition" {
                let Some(alias) = child_by_field(c, "name") else {
                    continue;
                };
                if unquote(text(cx, alias)) == name && c.start_byte() < node.start_byte() {
                    return true;
                }
            } else if c.kind() == "import_declaration" && c.start_byte() < node.start_byte() {
                if import_brings_type_alias(c, cx, name) {
                    return true;
                }
            }
        }
    }
    false
}

// Whether an `import` declaration brings `name` in as a type from a
// container defined in the same file. Each comma-separated expression
// resolves on its own: the segment before `.*`, `.{...}` or
// `.member as name` names a sibling `object`/`class`/`trait` whose `type`
// members are checked, and a bare `Types.name` covers its last segment.
// An import the same file cannot resolve keeps the old behavior at the
// use site.
fn import_brings_type_alias(import: Node<'_>, cx: &Cx<'_, '_>, name: &str) -> bool {
    let mut qualifier: Vec<&str> = Vec::new();
    let mut cursor = import.walk();
    for child in import.children(&mut cursor) {
        let declares = |member: &str| {
            qualifier
                .last()
                .is_some_and(|container| container_declares_type(import, cx, container, member))
        };
        let found = match child.kind() {
            "identifier" | "operator_identifier" => {
                qualifier.push(unquote(text(cx, child)));
                continue;
            }
            "." => continue,
            "namespace_wildcard" => text(cx, child) != "given" && declares(name),
            "namespace_selectors" => selectors_import_type(cx, child, name, declares),
            "as_renamed_identifier" => rename_parts(cx, child)
                .is_some_and(|(original, alias)| alias == name && declares(original)),
            // The `import` keyword or a comma ends an expression; a bare
            // member import stops at its last segment.
            _ => bare_member_import(import, cx, &qualifier, name),
        };
        if found {
            return true;
        }
        qualifier.clear();
    }
    bare_member_import(import, cx, &qualifier, name)
}

// A bare member import (`import Types._status`) covers its last segment
// when the segment before it names a container that declares the type.
fn bare_member_import(import: Node<'_>, cx: &Cx<'_, '_>, qualifier: &[&str], name: &str) -> bool {
    matches!(qualifier, [.., container, member]
        if *member == name && container_declares_type(import, cx, container, member))
}

// Whether a selector list `{...}` imports `name` as a type: a listed
// name, a rename whose alias is `name` (the original is the member to
// look up), or a wildcard. Renaming or hiding `name` itself (`name => _`)
// takes it out of the wildcard; a `given` selector brings no types.
fn selectors_import_type(
    cx: &Cx<'_, '_>,
    selectors: Node<'_>,
    name: &str,
    declares: impl Fn(&str) -> bool,
) -> bool {
    let mut wildcard = false;
    for sel in named_children(selectors) {
        match sel.kind() {
            "identifier" | "operator_identifier" if unquote(text(cx, sel)) == name => {
                return declares(name);
            }
            "namespace_wildcard" => wildcard |= text(cx, sel) != "given",
            "as_renamed_identifier" | "arrow_renamed_identifier" => match rename_parts(cx, sel) {
                Some((original, alias)) if alias == name => return declares(original),
                Some((original, _)) if original == name => return false,
                _ => {}
            },
            _ => {}
        }
    }
    wildcard && declares(name)
}

// The original member and the local alias of a rename (`a as b`,
// `a => b`); a hide (`a => _`) has the wildcard as its alias.
fn rename_parts<'s>(cx: &Cx<'_, 's>, rename: Node<'_>) -> Option<(&'s str, &'s str)> {
    let original = child_by_field(rename, "name")?;
    let alias = child_by_field(rename, "alias")?;
    Some((unquote(text(cx, original)), unquote(text(cx, alias))))
}

// Whether a sibling `object`/`class`/`trait` named `container`, visible
// where `import` sits, declares `type <member>` in its own body.
fn container_declares_type(
    import: Node<'_>,
    cx: &Cx<'_, '_>,
    container: &str,
    member: &str,
) -> bool {
    let mut scope = import.parent();
    while let Some(s) = scope {
        scope = s.parent();
        for c in named_children(s) {
            if matches!(
                c.kind(),
                "object_definition" | "class_definition" | "trait_definition"
            ) && child_by_field(c, "name").is_some_and(|n| unquote(text(cx, n)) == container)
                && body_declares_type(cx, c, member)
            {
                return true;
            }
        }
    }
    false
}

// Whether the container's own `template_body`, a direct child, declares
// `type <name>`. A template body inside the parents clause (an anonymous
// class argument) belongs to another type.
fn body_declares_type(cx: &Cx<'_, '_>, container: Node<'_>, name: &str) -> bool {
    named_children(container)
        .into_iter()
        .filter(|b| b.kind() == "template_body")
        .flat_map(named_children)
        .any(|m| {
            m.kind() == "type_definition"
                && child_by_field(m, "name").is_some_and(|alias| unquote(text(cx, alias)) == name)
        })
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
// `case given(x, _)` still binds `x`, and an all-wildcard `given(_, _)`
// still applies the extractor. A spaced paren, a plain type, or an
// all-alias member list is a tuple or plain type and binds nothing, the
// same rule as `is_given_tuple_type`.
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
    // A member that cannot be a type proves the extractor reading: a
    // lowercase or underscore-prefixed name with no visible alias binds.
    // An uppercase or alias-visible member reads as a tuple type. Only an
    // all-tuple member list stays a type; any binder marks the arm an
    // extractor application, which is a variant constructor even when the
    // binder list is empty.
    let mut out = Vec::new();
    let mut extractor = false;
    let mut tuple = false;
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
        if (first.is_lowercase() || first == '_') && !names_visible_type_alias(p, cx, name) {
            out.push(name);
            extractor = true;
        } else {
            tuple = true;
        }
    }
    if tuple && !extractor {
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
