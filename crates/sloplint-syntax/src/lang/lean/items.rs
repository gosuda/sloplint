//! Lean declaration, import, type, and match-arm heads.

use std::borrow::Cow;

use sloplint_core::facts::{
    Import, ImportKind, ImportScope, Pos, ScrutineeKind, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use super::{FUNCTION_KINDS, LEAN_STDLIB, lean_visibility};
use crate::hooks::{Cx, TypeHead, child_by_field, named_children, narrow, pos_of, text, trimmed};

pub(crate) fn decl_names_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if !matches!(n.kind(), "let" | "have") {
        return;
    }
    if let Some(name) = child_by_field(n, "name")
        && name.kind() == "identifier"
    {
        out.push((text(cx, name), pos_of(name)));
    }
}

pub(crate) fn write_roots_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "block_assign" {
        return;
    }
    if let Some(name) = child_by_field(n, "name")
        && name.kind() == "identifier"
    {
        out.push((text(cx, name), pos_of(name)));
    }
}

fn lean_external_root(callee: &str) -> Option<&str> {
    let root = callee.split('.').next().unwrap_or("");
    (!root.is_empty() && !LEAN_STDLIB.contains(&root)).then_some(root)
}

pub(crate) fn imports_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let pos = pos_of(n);
    match n.kind() {
        "import" => {
            let Some(name) = child_by_field(n, "name") else {
                return;
            };
            let raw = trimmed(cx, name);
            let last = raw.rsplit('.').next().unwrap_or("");
            let external_root = lean_external_root(raw).map(|root| {
                let end = raw.find(root).unwrap_or(0) + root.len();
                &raw[..end]
            });
            out.push(Import {
                pos,
                module: Cow::Borrowed(raw),
                name: last,
                alias: None,
                kind: ImportKind::Namespace,
                scope: ImportScope::Module,
                in_test: false,
                is_reexport: false,
                external_root,
            });
        }
        "open" | "export" => {
            // `open`/`export` bring names into scope without loading a
            // package, so they carry no `external_root` (import fan-out is
            // for `import` only). `open Nat (succ)` / `export Nat (succ)`
            // name their selections via the `only` field — not wildcards.
            let mut cursor = n.walk();
            let explicit = n
                .children_by_field_name("only", &mut cursor)
                .next()
                .is_some();
            let kind = if explicit {
                ImportKind::Named
            } else {
                ImportKind::Wildcard
            };
            let mut namespaces: Vec<Node> =
                n.children_by_field_name("namespace", &mut cursor).collect();
            namespaces.extend(n.children_by_field_name("scoped", &mut cursor));
            let mut skip = false;
            for ns in namespaces {
                let raw = trimmed(cx, ns);
                // The grammar lexes `open A hiding B`-style open modifiers
                // as identifiers; the keyword and what follows it are not
                // namespaces.
                if matches!(raw, "hiding" | "renaming" | "only" | "as") {
                    skip = true;
                }
                if skip {
                    continue;
                }
                out.push(Import {
                    pos: pos_of(ns),
                    module: Cow::Borrowed(raw),
                    name: "",
                    alias: None,
                    kind,
                    scope: ImportScope::Module,
                    in_test: false,
                    is_reexport: n.kind() == "export",
                    external_root: None,
                });
            }
        }
        _ => {}
    }
}

pub(crate) fn typedef_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if !matches!(n.kind(), "inductive" | "structure") {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    Some(TypeHead {
        def: Some(TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            visibility: lean_visibility(cx, n),
            abstraction: false,
        }),
        impls: Vec::new(),
        container: None,
    })
}

fn pattern_binds_none_some<'s>(cx: &Cx<'_, 's>, pattern: Node<'_>) -> bool {
    let mut stack = vec![pattern];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" {
            let name = text(cx, node);
            if name == "none" || name == "some" {
                return true;
            }
        }
        stack.extend(named_children(node));
    }
    false
}

pub(crate) fn match_arm_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "match_alt" {
        return None;
    }
    let pattern = child_by_field(n, "pattern")?;
    if pattern.kind() != "hole" {
        return None;
    }
    let parent = cx.parent()?;
    let mut variants = 0;
    let mut nullable = false;
    for sibling in named_children(parent) {
        if sibling.kind() != "match_alt" || sibling.id() == n.id() {
            continue;
        }
        if let Some(pat) = child_by_field(sibling, "pattern") {
            variants += 1;
            nullable = nullable || pattern_binds_none_some(cx, pat);
        }
    }
    variants += 1;
    let scrutinee = if parent.kind() == "match" {
        match child_by_field(parent, "scrutinee").map(|s| s.kind()) {
            Some("identifier") => ScrutineeKind::Ident,
            Some("proj" | "dot_ident" | "field") => ScrutineeKind::Field,
            _ => ScrutineeKind::Other,
        }
    } else if FUNCTION_KINDS.contains(&parent.kind()) {
        // Equation-compiler fallback arms match on the function arguments.
        ScrutineeKind::Ident
    } else {
        // Tactic `induction ... with` arms: the target above decides.
        ScrutineeKind::Other
    };
    Some(WildcardArm {
        pos: pos_of(n),
        enum_like: variants >= 2,
        scrutinee,
        variant_count: narrow(variants),
        has_nullable_ctor: nullable,
    })
}
