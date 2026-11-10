use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, any_named_child, child_by_field, first_named_child_matching, has_child_kind,
    named_children, pos_of, strip_quotes, text,
};

fn export_uri<'s>(cx: &Cx<'_, 's>, c: Node<'_>) -> Option<&'s str> {
    let u = child_by_field(c, "uri")?;
    let inner = if u.kind() == "configurable_uri" {
        first_named_child_matching(u, &["uri"])?
    } else {
        u
    };
    let lit = first_named_child_matching(inner, &["string_literal"])?;
    Some(strip_quotes(text(cx, lit)))
}

fn has_show(spec: Node<'_>) -> bool {
    any_named_child(spec, |c| {
        c.kind() == "combinator" && has_child_kind(c, "show")
    })
}

fn join_rel(dir: &str, uri: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in uri.split('/') {
        if seg == "." || seg.is_empty() {
            continue;
        } else if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    parts.join("/")
}

fn package_path(rest: &str) -> String {
    match rest.find('/') {
        Some(i) => format!("lib/{}", &rest[i + 1..]),
        None => format!("lib/{rest}"),
    }
}
fn is_barrel_edge(kind: &str) -> bool {
    kind == "library_import"
        || kind == "library_name"
        || kind == "part_directive"
        || kind == "part_of_directive"
        || kind == "script_tag"
        || kind == "comment"
        || kind == "block_comment"
        || kind == "documentation_block_comment"
}

fn dart_external_root(module: &str) -> Option<&str> {
    if module.starts_with("dart:") {
        return Some(module);
    }
    let rest = module.strip_prefix("package:")?;
    let end = rest.find('/').unwrap_or(rest.len());
    let pkg = &rest[..end];
    (!pkg.is_empty()).then_some(pkg)
}

struct ImportEmit<'a, 's> {
    module: &'s str,
    root: Option<&'s str>,
    pos: Pos,
    out: &'a mut Vec<Import<'s>>,
}

impl<'a, 's> ImportEmit<'a, 's> {
    fn namespace(&mut self, name: &'s str, alias: Option<&'s str>) {
        self.out.push(Import {
            pos: self.pos,
            module: Cow::Borrowed(self.module),
            name,
            alias,
            kind: ImportKind::Namespace,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: self.root,
        });
    }
    fn named(&mut self, name: &'s str) {
        self.out.push(Import {
            pos: self.pos,
            module: Cow::Borrowed(self.module),
            name,
            alias: None,
            kind: ImportKind::Named,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: self.root,
        });
    }
    fn show_one(&mut self, cx: &Cx<'_, 's>, comb: Node<'_>) {
        for id in named_children(comb) {
            if id.kind() == "identifier" {
                self.named(text(cx, id));
            }
        }
    }
    fn show_all(&mut self, cx: &Cx<'_, 's>, spec: Node<'_>) {
        for c in named_children(spec) {
            if c.kind() == "combinator" && has_child_kind(c, "show") {
                self.show_one(cx, c);
            }
        }
    }
}

pub(crate) fn imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let kind = n.kind();
    if kind != "library_import" && kind != "library_export" {
        return;
    }
    let spec = if kind == "library_import" {
        match first_named_child_matching(n, &["import_specification"]) {
            Some(s) => s,
            None => return,
        }
    } else {
        n
    };
    let Some(module) = export_uri(cx, spec) else {
        return;
    };
    let mut emit = ImportEmit {
        module,
        root: dart_external_root(module),
        pos: pos_of(n),
        out,
    };
    if has_show(spec) {
        emit.show_all(cx, spec);
        return;
    }
    let alias = child_by_field(spec, "alias").map(|a| text(cx, a));
    let mut segs = module;
    if let Some(at) = module.rfind('/') {
        segs = &module[at + 1..];
    } else if let Some(at) = module.rfind(':') {
        segs = &module[at + 1..];
    }
    let stem = segs
        .strip_suffix(".dart")
        .filter(|stem| !stem.is_empty())
        .unwrap_or(segs);
    emit.namespace(stem, alias);
}

pub(crate) fn barrel(cx: &Cx<'_, '_>, root: Node<'_>) -> Option<sloplint_core::facts::Barrel> {
    if root.kind() != "source_file" {
        return None;
    }
    let mut exports = 0u32;
    let mut uris = Vec::new();
    for c in named_children(root) {
        let directive = if c.kind() == "import_or_export" {
            first_named_child_matching(c, &["library_export", "library_import"]).unwrap_or(c)
        } else {
            c
        };
        if directive.kind() == "library_export" {
            exports += 1;
            uris.extend(export_uri(cx, directive));
        } else if !is_barrel_edge(directive.kind()) {
            return None;
        }
    }
    if exports == 0 {
        return None;
    }
    let dir = match cx.path.rfind('/') {
        Some(at) => &cx.path[..at],
        None => "",
    };
    let mut targets = Vec::new();
    for uri in uris {
        if uri.starts_with("dart:") {
            continue;
        }
        if let Some(rest) = uri.strip_prefix("package:") {
            targets.push(vec![package_path(rest)]);
        } else {
            targets.push(vec![join_rel(dir, uri)]);
        }
    }
    Some(sloplint_core::facts::Barrel { targets })
}
