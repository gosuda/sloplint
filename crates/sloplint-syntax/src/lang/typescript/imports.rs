//! Import and export facts for TypeScript.

use std::borrow::Cow;

use sloplint_core::facts::{ImplRef, Import, ImportKind, ImportScope, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_named_child_matching, first_of_kind, last_segment, named_children,
    pos_of, strip_quotes, text,
};
use crate::lang::javascript::{external_root_of, is_test_path};

pub(crate) struct TsSite<'s> {
    pos: Pos,
    module: &'s str,
    scope: ImportScope,
    in_test: bool,
    is_reexport: bool,
    external: Option<&'s str>,
}

impl<'s> TsSite<'s> {
    fn imported(cx: &Cx<'_, 's>, n: Node<'_>, module: &'s str) -> Self {
        Self::build(cx, n, module, false)
    }

    fn reexported(cx: &Cx<'_, 's>, n: Node<'_>, module: &'s str) -> Self {
        Self::build(cx, n, module, true)
    }

    fn build(cx: &Cx<'_, 's>, n: Node<'_>, module: &'s str, is_reexport: bool) -> Self {
        TsSite {
            pos: pos_of(n),
            module,
            scope: import_scope(cx),
            in_test: is_test_path(cx.path),
            is_reexport,
            external: external_root_of(module),
        }
    }

    fn at(
        &self,
        n: Node<'_>,
        name: &'s str,
        alias: Option<&'s str>,
        kind: ImportKind,
    ) -> Import<'s> {
        Import {
            pos: pos_of(n),
            module: Cow::Borrowed(self.module),
            name,
            alias,
            kind,
            scope: self.scope,
            in_test: self.in_test,
            is_reexport: self.is_reexport,
            external_root: self.external,
        }
    }

    fn here(&self, name: &'s str, alias: Option<&'s str>, kind: ImportKind) -> Import<'s> {
        Import {
            pos: self.pos,
            module: Cow::Borrowed(self.module),
            name,
            alias,
            kind,
            scope: self.scope,
            in_test: self.in_test,
            is_reexport: self.is_reexport,
            external_root: self.external,
        }
    }
}

fn import_scope(cx: &Cx<'_, '_>) -> ImportScope {
    for node in cx.ancestors() {
        if node.kind() == "program" {
            return ImportScope::Module;
        }
        if local_scope(node.kind()) {
            return ImportScope::FunctionLocal;
        }
    }
    ImportScope::Module
}

fn local_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "generator_function_declaration"
            | "function_expression"
            | "generator_function"
            | "arrow_function"
            | "method_definition"
            | "module"
            | "internal_module"
    )
}

fn alias_or_none<'s>(alias: &'s str, name: &str) -> Option<&'s str> {
    if alias.is_empty() || alias == name {
        None
    } else {
        Some(alias)
    }
}

pub(crate) fn clause_imports<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    site: &TsSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    for c in named_children(clause) {
        match c.kind() {
            "identifier" => out.push(site.at(c, text(cx, c), None, ImportKind::Named)),
            "named_imports" => import_specifiers(cx, c, site, out),
            "namespace_import" => {
                let name =
                    first_named_child_matching(c, &["identifier"]).map_or("", |g| text(cx, g));
                out.push(site.at(c, name, None, ImportKind::Namespace));
            }
            _ => {}
        }
    }
}

pub(crate) fn import_specifiers<'s>(
    cx: &Cx<'_, 's>,
    list: Node<'_>,
    site: &TsSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    for spec in named_children(list) {
        if spec.kind() != "import_specifier" {
            continue;
        }
        let name = child_by_field(spec, "name").map_or("", |v| text(cx, v));
        let alias = child_by_field(spec, "alias").map_or("", |v| text(cx, v));
        out.push(site.at(spec, name, alias_or_none(alias, name), ImportKind::Named));
    }
}

fn impl_of<'s>(cx: &Cx<'_, 's>, t: Node<'_>, abstract_child: bool) -> ImplRef<'s> {
    let generic = t.kind() == "generic_type";
    let base = if generic {
        child_by_field(t, "name").map_or("", |m| text(cx, m))
    } else {
        text(cx, t)
    };
    ImplRef {
        name: last_segment(base, &[".", "::"]),
        pos: pos_of(t),
        is_generic: generic,
        child_is_abstract: abstract_child,
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
    child_abstract: bool,
    out: &mut Vec<ImplRef<'s>>,
) -> bool {
    let mut found = false;
    for c in named_children(heritage) {
        if c.kind() == "implements_clause" {
            found = true;
            for t in named_children(c) {
                out.push(impl_of(cx, t, child_abstract));
            }
        } else if c.kind() == "extends_clause" {
            for v in named_children(c) {
                if v.kind() == "type_arguments" {
                    continue;
                }
                out.push(impl_of(cx, v, child_abstract));
            }
        }
    }
    found
}

pub(crate) fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

pub(crate) fn ts_targets(dir: &str, src: &str) -> Vec<String> {
    if !src.starts_with("./") && !src.starts_with("../") {
        return Vec::new();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let climbed = src.trim_start_matches("../");
    let Some(kept) = parts.len().checked_sub((src.len() - climbed.len()) / 3) else {
        return Vec::new();
    };
    parts.truncate(kept);
    let rest = climbed.strip_prefix("./").unwrap_or(climbed);
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
    let site = TsSite::imported(cx, n, module);
    out.push(site.here(
        clause.named_child(0).map_or("", |id| text(cx, id)),
        None,
        ImportKind::Namespace,
    ));
    true
}

pub(crate) fn import_stmt<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    if require_import(cx, n, out) {
        return;
    }
    let module = child_by_field(n, "source").map_or("", |s| strip_quotes(text(cx, s)));
    let site = TsSite::imported(cx, n, module);
    let Some(clause) = first_named_child_matching(n, &["import_clause"]) else {
        out.push(site.here("", None, ImportKind::SideEffect));
        return;
    };
    clause_imports(cx, clause, &site, out);
}

pub(crate) fn export_namespace<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    site: &TsSite<'s>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) -> bool {
    let Some(ns) = first_named_child_matching(n, &["namespace_export"]) else {
        return false;
    };
    let name = first_named_child_matching(ns, &["identifier"]).map_or("", |g| text(cx, g));
    let placed = TsSite {
        pos: pos_of(ns),
        module,
        scope: site.scope,
        in_test: site.in_test,
        is_reexport: site.is_reexport,
        external: site.external,
    };
    out.push(placed.here(name, None, ImportKind::Namespace));
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
    specs: Vec<(Node<'a>, &'s str, &'s str)>,
    site: &TsSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    for (spec, name, alias) in specs {
        out.push(site.at(spec, name, alias_or_none(alias, name), ImportKind::Named));
    }
}

pub(crate) fn export_stmt<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(src) = child_by_field(n, "source") else {
        return;
    };
    let module = strip_quotes(text(cx, src));
    let site = TsSite::reexported(cx, n, module);
    if export_namespace(cx, n, &site, module, out) {
        return;
    }
    let mut specs = Vec::new();
    if let Some(clause) = first_named_child_matching(n, &["export_clause"]) {
        specs = export_specs(cx, clause);
    }
    if specs.is_empty() {
        if has_export_star(cx, n) {
            out.push(site.here("*", None, ImportKind::Wildcard));
        }
        return;
    }
    push_named_exports(specs, &site, out);
}
