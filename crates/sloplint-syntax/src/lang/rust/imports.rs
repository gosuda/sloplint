use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use super::attrs::{has_bench_attr, has_test_attr, mod_is_test};
use crate::hooks::{Cx, child_by_field, has_child_kind, named_children, pos_of, text};

pub(super) struct UseFacts<'s> {
    pub scope: ImportScope,
    pub in_test: bool,
    pub is_reexport: bool,
    pub file_mods: Vec<&'s str>,
}

fn encloses_fn(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        match ancestor.kind() {
            "function_item" | "closure_expression" => return true,
            "source_file" => return false,
            _ => {}
        }
    }
    false
}

fn test_attr_on_ancestor(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        match ancestor.kind() {
            "mod_item" => {
                if mod_is_test(cx, ancestor) {
                    return true;
                }
            }
            "function_item" => {
                if has_test_attr(cx, ancestor) || has_bench_attr(cx, ancestor) {
                    return true;
                }
            }
            "source_file" => return false,
            _ => {}
        }
    }
    false
}

fn file_mods<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    named_children(root)
        .into_iter()
        .filter(|c| c.kind() == "mod_item")
        .filter_map(|m| child_by_field(m, "name"))
        .map(|name| text(cx, name))
        .collect()
}

pub(super) fn use_facts<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> UseFacts<'s> {
    UseFacts {
        scope: if encloses_fn(cx) {
            ImportScope::FunctionLocal
        } else {
            ImportScope::Module
        },
        in_test: test_attr_on_ancestor(cx),
        is_reexport: has_child_kind(n, "visibility_modifier"),
        file_mods: file_mods(cx),
    }
}

fn first_segment_of(path: &str) -> &str {
    path.trim_start_matches(':')
        .split("::")
        .next()
        .unwrap_or("")
}

fn external_root_of(first: &str) -> Option<&str> {
    match first {
        "" | "std" | "core" | "alloc" | "crate" | "super" | "self" => None,
        root => Some(root),
    }
}

fn enclosing_mod_public(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        if ancestor.kind() == "mod_item" {
            return has_child_kind(ancestor, "visibility_modifier");
        }
        if ancestor.kind() == "source_file" {
            return false;
        }
    }
    false
}

fn is_prelude_path(parts: &[&str], path: &str) -> bool {
    parts
        .iter()
        .flat_map(|p| p.split("::"))
        .chain(path.split("::"))
        .any(|seg| seg == "prelude" || seg.ends_with("_prelude"))
}

fn same_file_child<'s>(parts: &[&'s str], path: &'s str, mods: &[&'s str]) -> bool {
    let mut segs: Vec<&'s str> = parts
        .iter()
        .flat_map(|p| p.split("::"))
        .chain(path.split("::"))
        .filter(|s| !s.is_empty())
        .collect();
    if segs.first().is_some_and(|s| *s == "crate" || *s == "self") {
        segs.remove(0);
    }
    segs.first().is_some_and(|first| mods.contains(first))
}

fn starts_with_super(parts: &[&str], path: &str) -> bool {
    if let Some(first) = parts.first() {
        return first_segment_of(first) == "super";
    }
    first_segment_of(path) == "super"
}

fn root_first<'s>(parts: &[&'s str], path: &'s str) -> &'s str {
    if let Some(first) = parts.first() {
        return first_segment_of(first);
    }
    first_segment_of(path)
}

fn wildcard_kind<'s>(
    cx: &Cx<'_, '_>,
    parts: &[&'s str],
    path: &'s str,
    facts: &UseFacts<'s>,
) -> (ImportKind, bool, Option<&'s str>) {
    let root = external_root_of(root_first(parts, path));
    if facts.scope == ImportScope::FunctionLocal || is_prelude_path(parts, path) {
        return (ImportKind::Named, facts.in_test, root);
    }
    if same_file_child(parts, path, &facts.file_mods) {
        return (ImportKind::Named, false, None);
    }
    if starts_with_super(parts, path) && !enclosing_mod_public(cx) {
        return (ImportKind::Named, false, None);
    }
    (ImportKind::Wildcard, facts.in_test, root)
}

fn joined_module<'s>(parts: &[&str], head: &'s str, fallback: &'s str) -> Cow<'s, str> {
    let joined = if parts.is_empty() {
        Cow::Borrowed(head.trim_end_matches(':'))
    } else {
        Cow::Owned(parts.join("::") + "::" + head)
    };
    let trimmed = joined.trim_end_matches(':');
    if trimmed.is_empty() {
        Cow::Borrowed(fallback)
    } else if trimmed.len() == joined.len() {
        joined
    } else {
        Cow::Owned(trimmed.to_owned())
    }
}

fn last_segment(value: &str) -> &str {
    value.rsplit("::").next().unwrap_or("")
}

fn leaf_parts<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    parts: &[&'s str],
) -> Option<(&'s str, Option<&'s str>, Vec<&'s str>)> {
    let (path, alias) = if n.kind() == "use_as_clause" {
        let alias = child_by_field(n, "alias").map_or("", |a| text(cx, a));
        if alias == "_" {
            return None;
        }
        (
            child_by_field(n, "path").map_or("", |p| text(cx, p)),
            Some(alias),
        )
    } else {
        (text(cx, n), None)
    };
    let own = parts.last().copied().unwrap_or("");
    if path == "self" && !own.is_empty() {
        let mut base = parts.to_vec();
        if base.last().is_some_and(|last| *last == own) {
            base.pop();
        }
        Some((own, alias, base))
    } else {
        Some((path, alias, parts.to_vec()))
    }
}

fn push_leaf<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    parts: &[&'s str],
    facts: &UseFacts<'s>,
    out: &mut Vec<Import<'s>>,
) {
    let Some((path, alias, base)) = leaf_parts(cx, n, parts) else {
        return;
    };
    let name = last_segment(path);
    let head = path[..path.len() - name.len()].trim_end_matches(':');
    let module = joined_module(&base, head, name);
    out.push(Import {
        pos: pos_of(n),
        module,
        name,
        alias,
        kind: ImportKind::Named,
        scope: facts.scope,
        in_test: facts.in_test,
        is_reexport: facts.is_reexport,
        external_root: external_root_of(root_first(&base, path)),
    });
}

fn push_wildcard<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    parts: &[&'s str],
    facts: &UseFacts<'s>,
    out: &mut Vec<Import<'s>>,
) {
    let path = n.named_child(0).map_or("", |c| text(cx, c));
    let module = joined_module(parts, path, "*");
    let (kind, in_test, external_root) = wildcard_kind(cx, parts, path, facts);
    out.push(Import {
        pos: pos_of(n),
        module,
        name: "*",
        alias: None,
        kind,
        scope: facts.scope,
        in_test,
        is_reexport: facts.is_reexport,
        external_root,
    });
}

enum UseStep<'t> {
    Visit(Node<'t>),
    Restore(usize),
}

pub(super) fn push_use<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    parts: &mut Vec<&'s str>,
    facts: &UseFacts<'s>,
    out: &mut Vec<Import<'s>>,
) {
    let mut steps = vec![UseStep::Visit(n)];
    while let Some(step) = steps.pop() {
        match step {
            UseStep::Restore(len) => parts.truncate(len),
            UseStep::Visit(node) => visit_use(cx, node, parts, facts, out, &mut steps),
        }
    }
}

fn visit_use<'s, 't>(
    cx: &Cx<'_, 's>,
    n: Node<'t>,
    parts: &mut Vec<&'s str>,
    facts: &UseFacts<'s>,
    out: &mut Vec<Import<'s>>,
    steps: &mut Vec<UseStep<'t>>,
) {
    match n.kind() {
        "scoped_use_list" => {
            if let Some(path) = child_by_field(n, "path") {
                let base = parts.len();
                parts.push(text(cx, path));
                steps.push(UseStep::Restore(base));
                steps.extend(child_by_field(n, "list").map(UseStep::Visit));
            }
        }
        "use_list" => steps.extend(named_children(n).into_iter().rev().map(UseStep::Visit)),
        "use_wildcard" => push_wildcard(cx, n, parts, facts, out),
        _ => push_leaf(cx, n, parts, facts, out),
    }
}
