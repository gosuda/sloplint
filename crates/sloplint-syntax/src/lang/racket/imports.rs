//! Racket `require` and `provide` extraction: import facts from
//! relative-string specs (including `only-in`, `prefix-in` and
//! `relative-in` wrappers) and export names from `provide` sub-lists.

use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use crate::hooks::{Cx, last_segment, named_children, pos_of, strip_quotes, text};

use super::head::{Head, lookup_head};
use super::{code_children, define_name, first_symbol};

pub(super) fn push_require_spec<'s>(cx: &Cx<'_, 's>, spec: Node<'_>, out: &mut Vec<Import<'s>>) {
    let mut pending = vec![(spec, String::new())];
    while let Some((spec, anchor)) = pending.pop() {
        match spec.kind() {
            "string" => {
                let raw = strip_quotes(text(cx, spec));
                if raw.is_empty() || matches!(raw, "." | "..") {
                    continue;
                }
                if anchor.is_empty() {
                    push_string_require(cx, spec, raw, out);
                } else {
                    push_rebased_require(cx, spec, &anchor, raw, out);
                }
            }
            "list" => push_list_specs(cx, &anchor, spec, &mut pending),
            _ => {}
        }
    }
}

/// Each queued node occupies a documented require-spec position. Transformer
/// expressions and unrecognized forms never contribute guessed dependencies.
fn push_list_specs<'t>(
    cx: &Cx<'_, '_>,
    anchor: &str,
    spec: Node<'t>,
    pending: &mut Vec<(Node<'t>, String)>,
) {
    let kids = code_children(spec);
    let Some((head, operands)) = kids.split_first() else {
        return;
    };
    if head.kind() != "symbol" {
        return;
    }
    let selected = match text(cx, *head) {
        "relative-in" => {
            push_relative_in(cx, anchor, operands, pending);
            return;
        }
        "only-in" | "except-in" | "rename-in" | "subtract-in" | "submod" => operands.get(..1),
        "prefix-in" | "matching-identifiers-in" | "filtered-in" => operands.get(1..2),
        "for-meta" | "for-space" | "only-meta-in" | "only-space-in" => operands.get(1..),
        "combine-in" | "for-syntax" | "for-template" | "for-label" => Some(operands),
        "file" => operands.first().and_then(|path| {
            let raw = strip_quotes(text(cx, *path));
            (path.kind() == "string" && !raw.starts_with(['/', '~']) && !raw.contains(['\\', ':']))
                .then(|| &operands[..1])
        }),
        _ => None,
    };
    if let Some(selected) = selected {
        pending.extend(selected.iter().rev().map(|&node| (node, anchor.to_owned())));
    }
}

/// `(relative-in base spec ...)` resolves each wrapped spec against the
/// base's directory instead of the importing file. The base is only the
/// resolution anchor, never an import. A base that is not a relative
/// string cannot be rebased, so the wrapped specs stay out instead of
/// resolving against the importing file.
fn push_relative_in<'t>(
    cx: &Cx<'_, '_>,
    outer: &str,
    kids: &[Node<'t>],
    pending: &mut Vec<(Node<'t>, String)>,
) {
    let Some((base, specs)) = kids.split_first() else {
        return;
    };
    if base.kind() != "string" {
        return;
    }
    let base = strip_quotes(text(cx, *base));
    if base.is_empty() || base.starts_with('/') || base.ends_with('/') {
        return;
    }
    let anchor = rebase_relative(outer, base);
    let dir = anchor.rsplit_once('/').map_or("", |(dir, _)| dir);
    pending.extend(specs.iter().rev().map(|&node| (node, dir.to_owned())));
}

/// Joins a `relative-in` anchor directory with a wrapped spec lexically:
/// `..` cancels only a preceding normal segment. Unresolved parents survive
/// until module-key resolution supplies the importing file's directory.
fn rebase_relative(dir: &str, spec: &str) -> String {
    let mut parts = Vec::new();
    for seg in dir.split('/').chain(spec.split('/')) {
        match seg {
            "" | "." => {}
            ".." if parts.last().is_some_and(|part| *part != "..") => {
                parts.pop();
            }
            _ => parts.push(seg),
        }
    }
    parts.join("/")
}

fn push_rebased_require<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    dir: &str,
    raw: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    let rebased = rebase_relative(dir, raw);
    if rebased.is_empty() {
        return;
    }
    out.push(Import {
        pos: pos_of(spec),
        module: Cow::Owned(rebased),
        name: last_segment(raw, &["/"]),
        alias: None,
        kind: ImportKind::Namespace,
        scope: ImportScope::Module,
        in_test: is_test_path(cx.path),
        is_reexport: false,
        external_root: None,
    });
}

fn push_string_require<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    if module.is_empty() {
        return;
    }
    out.push(Import {
        pos: pos_of(spec),
        module: Cow::Borrowed(module),
        name: last_segment(module, &["/"]),
        alias: None,
        kind: ImportKind::Namespace,
        scope: ImportScope::Module,
        in_test: is_test_path(cx.path),
        is_reexport: false,
        external_root: None,
    });
}

fn is_test_path(path: &str) -> bool {
    let mut segments = path.split('/');
    let file = segments.next_back().unwrap_or(path);
    if file.rsplit_once('.').is_some_and(|(stem, extension)| {
        matches!(stem, "test" | "tests") && matches!(extension, "rkt" | "rktl" | "rktd")
    }) {
        return true;
    }
    segments.any(|segment| segment == "test" || segment == "tests")
}

/// Names a `(provide ...)` sub-list exports: `(all-defined-out)`
/// expands to every top-level defined function, `(contract-out
/// [name ...] ...)` to each clause name, `(rename-out [in out] ...)`
/// to each external name. Anything else is skipped.
pub(super) fn provide_list_names<'s>(
    cx: &Cx<'_, 's>,
    root: Node<'_>,
    spec: Node<'_>,
    out: &mut Vec<&'s str>,
) {
    let mut kids = code_children(spec).into_iter();
    let Some(head) = kids.next() else {
        return;
    };
    if head.kind() != "symbol" {
        return;
    }
    match text(cx, head) {
        "all-defined-out" => {
            for child in named_children(root) {
                if child.kind() != "list" {
                    continue;
                }
                let mut inner = code_children(child).into_iter();
                let Some(inner_head) = inner.next() else {
                    continue;
                };
                if inner_head.kind() == "symbol"
                    && matches!(lookup_head(text(cx, inner_head)), Some(Head::Define))
                    && let Some(name) = define_name(cx, child)
                {
                    out.push(name);
                }
            }
        }
        "contract-out" => {
            for clause in kids {
                if clause.kind() == "list"
                    && let Some(name) = first_symbol(cx, clause)
                {
                    out.push(name);
                }
            }
        }
        "rename-out" => {
            // Visibility is resolved against the internal binding, so
            // record the local name — `(rename-out [local alias])`
            // makes `local` an exported function.
            for pair in kids {
                if pair.kind() != "list" {
                    continue;
                }
                let mut inner = code_children(pair).into_iter();
                if let Some(local) = inner.next()
                    && local.kind() == "symbol"
                {
                    out.push(text(cx, local));
                }
            }
        }
        _ => {}
    }
}
