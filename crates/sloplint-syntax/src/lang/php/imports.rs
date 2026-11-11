use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use super::funcs::{inside_fn, is_test_path};
use crate::hooks::{
    Cx, FileSetKind, child_by_field, find_named_child, first_named_child_matching, named_children,
    pos_of, strip_quotes, text,
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

fn file_namespace<'s>(cx: &Cx<'_, 's>) -> Option<&'s str> {
    cx.file_names(FileSetKind::Module, || {
        let mut found: Vec<&'s str> = Vec::new();
        if let Some(root) = cx.root() {
            let namespace = find_named_child(root, |child| child.kind() == "namespace_definition");
            if let Some(name) = namespace.and_then(|ns| child_by_field(ns, "name")) {
                found.push(text(cx, name));
            }
        }
        found
    })
    .first()
    .copied()
}

fn same_package(prefix: &str, prior: &str, namespace: &str) -> bool {
    let mut segments = prefix
        .split('\\')
        .chain(prior.split('\\'))
        .filter(|seg| !seg.is_empty());
    namespace
        .split('\\')
        .filter(|seg| !seg.is_empty())
        .all(|part| segments.next() == Some(part))
}

fn external_root_of<'s>(
    prefix: &'s str,
    prior: &'s str,
    namespace: Option<&str>,
) -> Option<&'s str> {
    let top = if prefix.is_empty() { prior } else { prefix };
    if top.is_empty() {
        return None;
    }
    if let Some(ns) = namespace
        && same_package(prefix, prior, ns)
    {
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

#[derive(Clone, Copy)]
struct UseContext<'s> {
    prefix: &'s str,
    namespace: Option<&'s str>,
}

fn clause_import<'s>(
    cx: &Cx<'_, 's>,
    context: UseContext<'s>,
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
    let module: Cow<'s, str> = if context.prefix.is_empty() {
        Cow::Borrowed(prior)
    } else if prior.is_empty() {
        Cow::Borrowed(context.prefix)
    } else {
        Cow::Owned(format!("{}\\{prior}", context.prefix))
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
        external_root: external_root_of(context.prefix, prior, context.namespace),
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
        let context = UseContext {
            prefix: first_named_child_matching(n, &["namespace_name"])
                .map(|c| text(cx, c))
                .unwrap_or(""),
            namespace: file_namespace(cx),
        };
        for clause in use_clauses(n) {
            clause_import(cx, context, clause, scope, in_test, out);
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
