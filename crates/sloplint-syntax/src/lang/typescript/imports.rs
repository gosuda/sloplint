use std::borrow::Cow;

use sloplint_core::facts::{CallKind, ImplRef, Import, ImportKind, Stmt};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_of_kind, last_segment, named_children, pos_of, strip_quotes, text,
};

pub(crate) fn clause_imports<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    for c in named_children(clause) {
        match c.kind() {
            "identifier" => out.push(Import {
                pos: pos_of(c),
                module: Cow::Borrowed(module),
                name: text(cx, c),
                alias: None,
                kind: ImportKind::Named,
                in_test: cx.is_test_file,
            }),
            "named_imports" => import_specifiers(cx, c, module, out),
            "namespace_import" => {
                let name = named_children(c)
                    .into_iter()
                    .find(|g| g.kind() == "identifier")
                    .map_or("", |g| text(cx, g));
                out.push(Import {
                    pos: pos_of(c),
                    module: Cow::Borrowed(module),
                    name,
                    alias: None,
                    kind: ImportKind::Namespace,
                    in_test: cx.is_test_file,
                });
            }
            _ => {}
        }
    }
}

pub(crate) fn import_specifiers<'s>(
    cx: &Cx<'_, 's>,
    list: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    for spec in named_children(list) {
        if spec.kind() != "import_specifier" {
            continue;
        }
        let name = child_by_field(spec, "name").map_or("", |v| text(cx, v));
        let alias = child_by_field(spec, "alias").map_or("", |v| text(cx, v));
        out.push(Import {
            pos: pos_of(spec),
            module: Cow::Borrowed(module),
            name,
            alias: if alias.is_empty() || alias == name {
                None
            } else {
                Some(alias)
            },
            kind: ImportKind::Named,
            in_test: cx.is_test_file,
        });
    }
}
pub(crate) fn export_specs<'a, 's>(
    cx: &Cx<'_, 's>,
    clause: Node<'a>,
) -> Vec<(Node<'a>, &'s str, &'s str)> {
    let mut specs = Vec::new();
    for spec in named_children(clause) {
        if spec.kind() != "export_specifier" {
            continue;
        }
        let name = child_by_field(spec, "name").map_or("", |v| text(cx, v));
        let alias = child_by_field(spec, "alias").map_or("", |v| text(cx, v));
        specs.push((spec, name, alias));
    }
    specs
}
pub(crate) fn heritage_impls<'s>(
    cx: &Cx<'_, 's>,
    heritage: Node<'_>,
    out: &mut Vec<ImplRef<'s>>,
) -> bool {
    let mut found = false;
    for c in named_children(heritage) {
        if c.kind() == "implements_clause" {
            found = true;
            for t in named_children(c) {
                out.push(ts_impl(cx, t));
            }
        } else if c.kind() == "extends_clause" {
            for v in named_children(c) {
                if v.kind() == "type_arguments" {
                    continue;
                }
                out.push(ts_impl(cx, v));
            }
        }
    }
    found
}
pub(crate) fn ts_impl<'s>(cx: &Cx<'_, 's>, t: Node<'_>) -> ImplRef<'s> {
    let base = if t.kind() == "generic_type" {
        child_by_field(t, "name").map_or("", |m| text(cx, m))
    } else {
        text(cx, t)
    };
    ImplRef {
        name: last_segment(base, &[".", "::"]),
        pos: pos_of(t),
    }
}

pub(crate) fn catch_stmt<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    if s.kind() == "throw_statement" {
        return throw_of(cx, s, binding);
    }
    if s.kind() != "expression_statement" {
        return Stmt::Other;
    }
    let Some(inner) = named_children(s).into_iter().next() else {
        return Stmt::Other;
    };
    if inner.kind() == "call_expression" {
        let callee = child_by_field(inner, "function").map_or("", |f| text(cx, f));
        return Stmt::Call {
            callee,
            kind: CallKind::Plain,
        };
    }
    if inner.kind() == "throw_statement" {
        return throw_of(cx, inner, binding);
    }
    Stmt::Other
}

pub(crate) fn throw_of<'s>(cx: &Cx<'_, 's>, s: Node<'_>, binding: &str) -> Stmt<'s> {
    let hit = named_children(s)
        .into_iter()
        .next()
        .is_some_and(|e| e.kind() == "identifier" && text(cx, e) == binding);
    if hit { Stmt::Rethrow } else { Stmt::Other }
}

pub(crate) fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

pub(crate) fn ts_targets(dir: &str, src: &str) -> Vec<String> {
    if !src.starts_with("./") && !src.starts_with("../") {
        return Vec::new();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let mut rest = src;
    while let Some(next) = rest.strip_prefix("../") {
        if parts.pop().is_none() {
            return Vec::new();
        }
        rest = next;
    }
    rest = rest.strip_prefix("./").map_or(rest, |s| s);
    let prefix = parts.join("/");
    let join = |base: String| {
        if prefix.is_empty() {
            base
        } else {
            prefix.clone() + "/" + base.as_str()
        }
    };
    let stem = rest
        .strip_suffix(".jsx")
        .or_else(|| rest.strip_suffix(".mjs"))
        .or_else(|| rest.strip_suffix(".js"))
        .or_else(|| rest.strip_suffix(".cjs"))
        .or_else(|| rest.strip_suffix(".mts"))
        .or_else(|| rest.strip_suffix(".cts"));
    if let Some(stripped) = stem {
        return vec![
            join(stripped.to_owned() + ".ts"),
            join(stripped.to_owned() + ".tsx"),
        ];
    }
    vec![
        join(rest.to_owned() + ".ts"),
        join(rest.to_owned() + ".tsx"),
        join(rest.to_owned() + "/index.ts"),
        join(rest.to_owned() + "/index.tsx"),
    ]
}

pub(crate) fn require_import<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) -> bool {
    let Some(clause) = first_of_kind(n, &["import_require_clause"]) else {
        return false;
    };
    let module = child_by_field(clause, "source").map_or("", |s| strip_quotes(text(cx, s)));
    out.push(Import {
        pos: pos_of(n),
        module: Cow::Borrowed(module),
        name: clause.named_child(0).map_or("", |id| text(cx, id)),
        alias: None,
        kind: ImportKind::Namespace,
        in_test: cx.is_test_file,
    });
    true
}
pub(crate) fn import_stmt<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    if require_import(cx, n, out) {
        return;
    }
    let module = child_by_field(n, "source").map_or("", |s| strip_quotes(text(cx, s)));
    let Some(clause) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "import_clause")
    else {
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(module),
            name: "",
            alias: None,
            kind: ImportKind::SideEffect,
            in_test: cx.is_test_file,
        });
        return;
    };
    clause_imports(cx, clause, module, out);
}
pub(crate) fn export_namespace<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) -> bool {
    let Some(ns) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "namespace_export")
    else {
        return false;
    };
    let name = named_children(ns)
        .into_iter()
        .find(|g| g.kind() == "identifier")
        .map_or("", |g| text(cx, g));
    out.push(Import {
        pos: pos_of(ns),
        module: Cow::Borrowed(module),
        name,
        alias: None,
        kind: ImportKind::Namespace,
        in_test: cx.is_test_file,
    });
    true
}
pub(crate) fn has_export_star(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    for i in 0..n.child_count() {
        if let Some(c) = n.child(i)
            && !c.is_named()
            && text(cx, c) == "*"
        {
            return true;
        }
    }
    false
}
pub(crate) fn push_named_exports<'a, 's>(
    cx: &Cx<'_, 's>,
    specs: Vec<(Node<'a>, &'s str, &'s str)>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    for (spec, name, alias) in specs {
        out.push(Import {
            pos: pos_of(spec),
            module: Cow::Borrowed(module),
            name,
            alias: if alias.is_empty() || alias == name {
                None
            } else {
                Some(alias)
            },
            kind: ImportKind::Named,
            in_test: cx.is_test_file,
        });
    }
}
pub(crate) fn export_stmt<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(src) = child_by_field(n, "source") else {
        return;
    };
    let module = strip_quotes(text(cx, src));
    if export_namespace(cx, n, module, out) {
        return;
    }
    let mut specs = Vec::new();
    if let Some(clause) = named_children(n)
        .into_iter()
        .find(|c| c.kind() == "export_clause")
    {
        specs = export_specs(cx, clause);
    }
    if specs.is_empty() {
        if has_export_star(cx, n) {
            out.push(Import {
                pos: pos_of(n),
                module: Cow::Borrowed(module),
                name: "*",
                alias: None,
                kind: ImportKind::Wildcard,
                in_test: cx.is_test_file,
            });
        }
        return;
    }
    push_named_exports(cx, specs, module, out);
}

#[cfg(test)]
mod tests {
    use super::ts_targets;

    #[test]
    fn module_suffixes_resolve_to_their_typescript_stem() {
        assert_eq!(ts_targets("src", "./x.cjs"), ["src/x.ts", "src/x.tsx"]);
        assert_eq!(ts_targets("src", "../lib/y.mts"), ["lib/y.ts", "lib/y.tsx"]);
        assert_eq!(ts_targets("src", "./z.cts"), ["src/z.ts", "src/z.tsx"]);
    }

    #[test]
    fn parent_segment_past_the_root_resolves_to_nothing() {
        assert!(
            ts_targets("", "../x").is_empty(),
            "an import escaping the root must resolve to nothing"
        );
        assert!(
            ts_targets("a", "../../x").is_empty(),
            "an import escaping the root must resolve to nothing"
        );
        assert_eq!(
            ts_targets("a", "../x"),
            ["x.ts", "x.tsx", "x/index.ts", "x/index.tsx"]
        );
    }
}
