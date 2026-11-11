//! Go import facts: module paths, external roots, and test scope.

use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use super::funcs::go_is_test_file;
use crate::hooks::{Cx, child_by_field, named_children, pos_of, strip_quotes, text};

pub(crate) fn go_imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let in_test = go_is_test_file(cx.path);
    for child in named_children(n) {
        if child.kind() == "import_spec" {
            spec_import(cx, child, in_test, out);
        } else {
            for spec in named_children(child) {
                if spec.kind() == "import_spec" {
                    spec_import(cx, spec, in_test, out);
                }
            }
        }
    }
}

fn spec_import<'s>(cx: &Cx<'_, 's>, spec: Node<'_>, in_test: bool, out: &mut Vec<Import<'s>>) {
    let module = child_by_field(spec, "path").map_or("", |p| strip_quotes(text(cx, p)));
    let seg = version_seg(module);
    let external_root = go_external_root(module);
    let pos = pos_of(spec);
    let (name, alias, kind) = match child_by_field(spec, "name").map(|v| v.kind()) {
        Some("dot") => ("*", None, ImportKind::Wildcard),
        Some("blank_identifier") => ("", None, ImportKind::SideEffect),
        Some(_) => (
            seg,
            child_by_field(spec, "name").map(|v| text(cx, v)),
            ImportKind::Named,
        ),
        None => (seg, None, ImportKind::Named),
    };
    out.push(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias,
        kind,
        scope: ImportScope::Module,
        in_test,
        is_reexport: false,
        external_root,
    });
}

fn go_external_root(module: &str) -> Option<&str> {
    let mut take = 0usize;
    for (i, seg) in module.split('/').enumerate() {
        if i == 0 && !seg.contains('.') {
            return None;
        }
        if i >= 3 {
            break;
        }
        take += seg.len() + 1;
    }
    Some(&module[..take - 1])
}

fn version_seg(module: &str) -> &str {
    let last = module.rsplit('/').next().map_or("", |s| s);
    let versioned =
        last.len() > 1 && last.starts_with('v') && last[1..].bytes().all(|b| b.is_ascii_digit());
    if !versioned {
        return last;
    }
    let Some(cut) = module.len().checked_sub(last.len() + 1) else {
        return last;
    };
    module[..cut].rsplit('/').next().map_or(last, |s| s)
}
#[cfg(test)]
mod tests {
    use super::go_external_root;

    #[test]
    fn truncates_roots_to_three_segments() {
        assert_eq!(
            go_external_root("github.com/uber-go/zap/zapcore"),
            Some("github.com/uber-go/zap")
        );
        assert_eq!(go_external_root("go.uber.org/zap"), Some("go.uber.org/zap"));
        assert_eq!(
            go_external_root("gopkg.in/yaml.v3"),
            Some("gopkg.in/yaml.v3")
        );
        assert_eq!(go_external_root("fmt"), None);
    }
}
