use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use super::funcs::{inside_fn, is_test_path};
use crate::hooks::{
    Cx, child_by_field, find_named_child, first_named_child_matching, named_children, pos_of,
    strip_quotes, text,
};

fn use_clauses(n: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        if child.kind() == "namespace_use_clause" {
            out.push(child);
        } else if child.kind() == "namespace_use_group" {
            for sub in named_children(child) {
                if sub.kind() == "namespace_use_clause" {
                    out.push(sub);
                }
            }
        }
    }
    out
}

fn external_root_of<'s>(prefix: &'s str, path: &'s str) -> Option<&'s str> {
    let top = if prefix.is_empty() { path } else { prefix };
    if !top.contains('\\') {
        return None;
    }
    top.split('\\').find(|seg| !seg.is_empty())
}

fn include_module<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    for child in named_children(n) {
        if child.kind() == "string" {
            return strip_quotes(text(cx, child));
        }
    }
    ""
}

fn clause_import<'s>(
    cx: &Cx<'_, 's>,
    prefix: &'s str,
    clause: Node<'_>,
    scope: ImportScope,
    in_test: bool,
    out: &mut Vec<Import<'s>>,
) {
    let alias_node = child_by_field(clause, "alias");
    let path = find_named_child(clause, |c| Some(c.id()) != alias_node.map(|a| a.id()))
        .map(|c| text(cx, c))
        .unwrap_or("");
    if path.is_empty() {
        return;
    }
    let cut = path.rfind('\\').map(|at| at + 1).unwrap_or(0);
    let (prior, name) = path.split_at(cut);
    let prior = prior.strip_suffix('\\').unwrap_or(prior);
    let module: Cow<'s, str> = if prefix.is_empty() {
        Cow::Borrowed(prior)
    } else if prior.is_empty() {
        Cow::Borrowed(prefix)
    } else {
        Cow::Owned(format!("{prefix}\\{prior}"))
    };
    out.push(Import {
        pos: pos_of(clause),
        module,
        name,
        alias: alias_node.map(|a| text(cx, a)),
        kind: ImportKind::Named,
        scope,
        in_test,
        is_reexport: false,
        external_root: external_root_of(prefix, path),
    });
}

pub(crate) fn push_imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let is_use = n.kind() == "namespace_use_declaration";
    let is_include = matches!(
        n.kind(),
        "include_expression"
            | "include_once_expression"
            | "require_expression"
            | "require_once_expression"
    );
    if !is_use && !is_include {
        return;
    }
    let scope = if inside_fn(cx, n) {
        ImportScope::FunctionLocal
    } else {
        ImportScope::Module
    };
    let in_test = is_test_path(cx.path);
    if is_use {
        let prefix = first_named_child_matching(n, &["namespace_name"])
            .map(|c| text(cx, c))
            .unwrap_or("");
        for clause in use_clauses(n) {
            clause_import(cx, prefix, clause, scope, in_test, out);
        }
        return;
    }
    if is_include {
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(include_module(cx, n)),
            name: "",
            alias: None,
            kind: ImportKind::SideEffect,
            scope,
            in_test,
            is_reexport: false,
            external_root: None,
        });
    }
}
