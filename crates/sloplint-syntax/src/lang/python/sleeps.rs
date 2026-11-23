//! Track explicit `time.sleep` imports; ambiguous bindings remain unclassified.

use std::collections::HashSet;

use sloplint_core::facts::Pos;
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use crate::hooks::{Cx, FileSetKind, child_by_field, code_children, named_children, pos_of, text};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportForm {
    Module,
    Function,
}

pub(crate) fn blocking_sleep(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if cx.lang != Lang::Python || n.kind() != "call" {
        return None;
    }
    let function = child_by_field(n, "function")?;
    let (kind, name) = match function.kind() {
        "attribute" => {
            let object = child_by_field(function, "object")?;
            let attribute = child_by_field(function, "attribute")?;
            if object.kind() != "identifier" || text(cx, attribute) != "sleep" {
                return None;
            }
            (ImportForm::Module, text(cx, object))
        }
        "identifier" => (ImportForm::Function, text(cx, function)),
        _ => return None,
    };
    let slot = match kind {
        ImportForm::Module => FileSetKind::SleepModules,
        ImportForm::Function => FileSetKind::SleepFunctions,
    };
    let names = cx.file_names(slot, || imported_names(cx, kind));
    if names.binary_search(&name).is_err() {
        return None;
    }
    let args = code_children(child_by_field(n, "arguments")?);
    if args.len() != 1 || nonpositive_literal(args[0], cx) {
        return None;
    }
    Some(pos_of(n))
}

fn imported_names<'s>(cx: &Cx<'_, 's>, wanted: ImportForm) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut imports = Vec::new();
    for child in named_children(root) {
        if matches!(child.kind(), "import_statement" | "import_from_statement") {
            import_bindings(child, cx, |name, kind| imports.push((name, kind)));
        }
    }
    if !imports.iter().any(|(_, kind)| *kind == Some(wanted)) {
        return Vec::new();
    }
    let modules: HashSet<_> = imports
        .iter()
        .filter_map(|(name, kind)| (*kind == Some(ImportForm::Module)).then_some(*name))
        .collect();
    let mut candidates = HashSet::new();
    let mut rebound = HashSet::new();
    let mut stack = vec![(root, false)];
    while let Some((n, module_scope)) = stack.pop() {
        if patches_sleep(n, cx, &modules) {
            return Vec::new();
        }
        if matches!(n.kind(), "import_statement" | "import_from_statement") {
            let mut wildcard = false;
            import_bindings(n, cx, |name, kind| {
                if name == "*" {
                    wildcard = true;
                } else if module_scope && kind == Some(wanted) {
                    candidates.insert(name);
                } else {
                    rebound.insert(name);
                }
            });
            if wildcard {
                return Vec::new();
            }
            continue;
        }
        bound_names(n, cx, &mut rebound);
        if matches!(n.kind(), "case_pattern" | "type_parameter") {
            continue;
        }
        stack.extend(
            named_children(n)
                .into_iter()
                .map(|child| (child, n.id() == root.id())),
        );
    }
    // A binding anywhere in the file vetoes the imported spelling. This loses
    // some separate-scope calls but never treats a shadowing fake as time.sleep.
    let mut names: Vec<_> = candidates.difference(&rebound).copied().collect();
    names.sort_unstable();
    names
}

fn patches_sleep(n: Node<'_>, cx: &Cx<'_, '_>, modules: &HashSet<&str>) -> bool {
    let target = match n.kind() {
        "assignment" | "augmented_assignment" => child_by_field(n, "left"),
        "delete_statement" => Some(n),
        _ => None,
    };
    if let Some(target) = target {
        let mut stack = vec![target];
        while let Some(node) = stack.pop() {
            if node.kind() == "attribute"
                && child_by_field(node, "attribute").is_some_and(|attr| text(cx, attr) == "sleep")
                && child_by_field(node, "object").is_some_and(|obj| {
                    obj.kind() == "identifier" && modules.contains(text(cx, obj))
                })
            {
                return true;
            }
            stack.extend(named_children(node));
        }
    }
    if n.kind() != "call" {
        return false;
    }
    let Some(arguments) = child_by_field(n, "arguments") else {
        return false;
    };
    let args = code_children(arguments);
    let Some(first) = args.first().copied() else {
        return false;
    };
    // Literal patch targets also cover aliased patch helpers and decorators.
    if plain_string(first, cx) == Some("time.sleep") {
        return true;
    }
    let Some(function) = child_by_field(n, "function") else {
        return false;
    };
    let member = child_by_field(function, "attribute").unwrap_or(function);
    matches!(text(cx, member), "setattr" | "delattr" | "object")
        && first.kind() == "identifier"
        && modules.contains(text(cx, first))
        && args
            .get(1)
            .is_some_and(|arg| plain_string(*arg, cx) == Some("sleep"))
}

fn plain_string<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<&'s str> {
    if n.kind() != "string" {
        return None;
    }
    let raw = text(cx, n);
    raw.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
}

fn import_bindings<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    mut emit: impl FnMut(&'s str, Option<ImportForm>),
) {
    let module = child_by_field(n, "module_name");
    for child in named_children(n) {
        if Some(child.id()) == module.map(|n| n.id()) {
            continue;
        }
        if child.kind() == "wildcard_import" {
            emit("*", None);
            continue;
        }
        let (path, alias) = match child.kind() {
            "dotted_name" => (text(cx, child), None),
            "aliased_import" => {
                let Some(path) = child_by_field(child, "name") else {
                    continue;
                };
                (
                    text(cx, path),
                    child_by_field(child, "alias").map(|n| text(cx, n)),
                )
            }
            _ => continue,
        };
        let (name, kind) = if n.kind() == "import_statement" {
            (
                alias.unwrap_or_else(|| path.split('.').next().unwrap_or(path)),
                (path == "time").then_some(ImportForm::Module),
            )
        } else {
            (
                alias.unwrap_or(path),
                (module.is_some_and(|n| text(cx, n) == "time") && path == "sleep")
                    .then_some(ImportForm::Function),
            )
        };
        emit(name, kind);
    }
}

fn bound_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut HashSet<&'s str>) {
    let target = match n.kind() {
        "assignment"
        | "augmented_assignment"
        | "for_statement"
        | "for_in_clause"
        | "type_alias_statement" => child_by_field(n, "left"),
        "function_definition" | "class_definition" | "named_expression" => {
            child_by_field(n, "name")
        }
        "as_pattern" => child_by_field(n, "alias"),
        "delete_statement" | "case_pattern" | "type_parameter" | "global_statement"
        | "nonlocal_statement" => Some(n),
        "parameters" | "lambda_parameters" => {
            let mut params = Vec::new();
            super::params::push_params(cx, n, &mut params);
            out.extend(params.into_iter().map(|param| param.name));
            None
        }
        _ => None,
    };
    if let Some(target) = target {
        let mut stack = vec![target];
        while let Some(node) = stack.pop() {
            if node.kind() == "identifier" {
                out.insert(text(cx, node));
            }
            stack.extend(named_children(node));
        }
    }
}

fn nonpositive_literal(mut n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let mut negative = false;
    loop {
        match n.kind() {
            "parenthesized_expression" => {
                let Some(inner) = n.named_child(0) else {
                    return false;
                };
                n = inner;
            }
            "unary_operator" => {
                let Some(operator) = child_by_field(n, "operator") else {
                    return false;
                };
                match text(cx, operator) {
                    "-" => negative = !negative,
                    "+" => {}
                    _ => return false,
                }
                let Some(inner) = child_by_field(n, "argument") else {
                    return false;
                };
                n = inner;
            }
            "integer" | "float" => return negative || zero_literal(text(cx, n)),
            "false" => return true,
            _ => return false,
        }
    }
}

fn zero_literal(raw: &str) -> bool {
    let digits = if raw.starts_with("0x")
        || raw.starts_with("0X")
        || raw.starts_with("0o")
        || raw.starts_with("0O")
        || raw.starts_with("0b")
        || raw.starts_with("0B")
    {
        &raw[2..]
    } else {
        raw.split(['e', 'E']).next().unwrap_or(raw)
    };
    digits.bytes().any(|byte| byte == b'0')
        && digits
            .bytes()
            .all(|byte| matches!(byte, b'0' | b'_' | b'.'))
}
