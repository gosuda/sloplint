use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope, Pos};
use tree_sitter::Node;

use super::funcs::in_test_dir;
use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};

pub(crate) fn import_path<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<&'s str> {
    let mut segs = Vec::new();
    let mut i = 0u32;
    while i < n.child_count() {
        let Some(c) = n.child(i) else {
            i += 1;
            continue;
        };
        match c.kind() {
            "identifier" | "operator_identifier" => segs.push(text(cx, c)),
            "namespace_selectors"
            | "namespace_wildcard"
            | "as_renamed_identifier"
            | "arrow_renamed_identifier" => break,
            _ => {}
        }
        i += 1;
    }
    segs
}

#[derive(Clone, Copy)]
struct Head {
    pos: Pos,
}

struct Emit<'a, 's> {
    module: String,
    wild: &'s str,
    head: Head,
    test_flag: bool,
    reexport: bool,
    root: Option<&'s str>,
    out: &'a mut Vec<Import<'s>>,
}

impl<'a, 's> Emit<'a, 's> {
    fn named(&mut self, name: &'s str, alias: Option<&'s str>) {
        self.out.push(Import {
            pos: self.head.pos,
            module: Cow::Owned(self.module.clone()),
            name,
            alias,
            kind: ImportKind::Named,
            scope: ImportScope::Module,
            in_test: self.test_flag,
            is_reexport: self.reexport,
            external_root: self.root,
        });
    }
    fn wild(&mut self) {
        self.out.push(Import {
            pos: self.head.pos,
            module: Cow::Owned(self.module.clone()),
            name: self.wild,
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: self.test_flag,
            is_reexport: self.reexport,
            external_root: self.root,
        });
    }
    fn renamed(&mut self, cx: &Cx<'_, 's>, s: Node<'_>) {
        let name = child_by_field(s, "name").map(|m| text(cx, m));
        let alias = child_by_field(s, "alias").map(|m| text(cx, m));
        match (name, alias) {
            (Some(nn), Some(aa)) => self.named(
                nn,
                if aa == "_" || aa == nn {
                    None
                } else {
                    Some(aa)
                },
            ),
            (Some(nn), None) => self.named(nn, None),
            _ => {}
        }
    }
    fn selector(&mut self, cx: &Cx<'_, 's>, s: Node<'_>) {
        if s.kind() == "identifier" {
            self.named(text(cx, s), None);
        } else if s.kind() == "as_renamed_identifier" || s.kind() == "arrow_renamed_identifier" {
            self.renamed(cx, s);
        } else if s.kind() == "namespace_wildcard" {
            self.wild();
        }
    }
    fn selectors(&mut self, cx: &Cx<'_, 's>, sel: Node<'_>) {
        for s in named_children(sel) {
            self.selector(cx, s);
        }
    }
    fn child(&mut self, cx: &Cx<'_, 's>, c: Node<'_>) -> bool {
        if c.kind() == "namespace_wildcard" {
            self.wild();
            true
        } else if c.kind() == "namespace_selectors" {
            self.selectors(cx, c);
            true
        } else if c.kind() == "as_renamed_identifier" || c.kind() == "arrow_renamed_identifier" {
            self.renamed(cx, c);
            true
        } else {
            false
        }
    }
}

pub(crate) fn push_imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let kind = n.kind();
    if kind != "import_declaration" && kind != "export_declaration" {
        return;
    }
    let head = Head { pos: pos_of(n) };
    let path = import_path(cx, n);
    let root = path
        .first()
        .copied()
        .filter(|first| !matches!(*first, "scala" | "java" | "javax"));
    let mut emit = Emit {
        module: path.join("."),
        wild: path.last().copied().unwrap_or(""),
        head,
        test_flag: in_test_dir(cx.path),
        reexport: kind == "export_declaration",
        root,
        out,
    };
    let mut handled = false;
    for c in named_children(n) {
        handled |= emit.child(cx, c);
    }
    if handled {
        return;
    }
    let name = path.last().copied().unwrap_or("");
    let lead = if path.is_empty() {
        String::new()
    } else {
        path[..path.len() - 1].join(".")
    };
    emit.out.push(Import {
        pos: emit.head.pos,
        module: Cow::Owned(lead),
        name,
        alias: None,
        kind: ImportKind::Named,
        scope: ImportScope::Module,
        in_test: emit.test_flag,
        is_reexport: emit.reexport,
        external_root: root,
    });
}
