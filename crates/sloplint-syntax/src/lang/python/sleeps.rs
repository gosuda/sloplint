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

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatchImport {
    Function,
    MockModule,
    UnittestModule,
    BuiltinsModule,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MutationTarget {
    Object,
    DottedOrObject,
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
            import_bindings(child, cx, |name, kind, patch| {
                imports.push((name, kind, patch))
            });
        }
    }
    if !imports.iter().any(|(_, kind, _)| *kind == Some(wanted)) {
        return Vec::new();
    }
    let modules: HashSet<_> = imports
        .iter()
        .filter_map(|(name, kind, _)| (*kind == Some(ImportForm::Module)).then_some(*name))
        .collect();
    let mut builtins: HashSet<_> = imports
        .iter()
        .filter_map(|(name, _, patch)| {
            (*patch == Some(PatchImport::BuiltinsModule)).then_some(*name)
        })
        .collect();
    let mut candidates = HashSet::new();
    let mut rebound = HashSet::new();
    let mut bindings = HashSet::new();
    let mut patches = Vec::new();
    let mut calls = Vec::new();
    let mut stack = vec![(root, false)];
    while let Some((n, module_scope)) = stack.pop() {
        if writes_sleep(n, cx, &modules) {
            return Vec::new();
        }
        if n.kind() == "call" {
            calls.push(n);
        }
        if matches!(n.kind(), "import_statement" | "import_from_statement") {
            let mut wildcard = false;
            import_bindings(n, cx, |name, kind, patch| {
                if !module_scope || patch != Some(PatchImport::BuiltinsModule) {
                    builtins.remove(name);
                }
                if let Some(patch) = patch {
                    patches.push((name, patch));
                }
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
        bound_names(n, cx, &mut bindings);
        if matches!(n.kind(), "case_pattern" | "type_parameter") {
            continue;
        }
        stack.extend(
            named_children(n)
                .into_iter()
                .map(|child| (child, n.id() == root.id())),
        );
    }
    builtins.retain(|name| !bindings.contains(name));
    rebound.extend(bindings);
    if calls
        .into_iter()
        .any(|call| patches_sleep(call, cx, &modules, &patches, &builtins))
    {
        return Vec::new();
    }
    // A binding anywhere in the file vetoes the imported spelling. This loses
    // some separate-scope calls but never treats a shadowing fake as time.sleep.
    let mut names: Vec<_> = candidates.difference(&rebound).copied().collect();
    names.sort_unstable();
    names
}

fn writes_sleep(n: Node<'_>, cx: &Cx<'_, '_>, modules: &HashSet<&str>) -> bool {
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
            // Nested attributes and subscripts read their object/index. Only
            // the outer assignment target can replace time.sleep itself.
            if !matches!(node.kind(), "attribute" | "subscript") {
                stack.extend(named_children(node));
            }
        }
    }
    false
}

fn patches_sleep(
    n: Node<'_>,
    cx: &Cx<'_, '_>,
    modules: &HashSet<&str>,
    patches: &[(&str, PatchImport)],
    builtins: &HashSet<&str>,
) -> bool {
    let Some(function) = child_by_field(n, "function") else {
        return false;
    };
    let mutator = mutation_helper(function, cx, builtins);
    let patch = patch_function(function, cx, patches);
    let object_patch = child_by_field(function, "attribute")
        .is_some_and(|member| text(cx, member) == "object")
        && child_by_field(function, "object")
            .is_some_and(|object| patch_function(object, cx, patches));
    if mutator.is_none() && !patch && !object_patch {
        return false;
    }
    let Some(arguments) = child_by_field(n, "arguments") else {
        return false;
    };
    let args = code_children(arguments);
    let Some(target) = argument(&args, 0, "target", cx) else {
        return false;
    };
    // Only monkeypatch accepts a dotted string here; built-in setattr/delattr
    // treat their first argument as the object itself.
    let dotted_target = patch || mutator == Some(MutationTarget::DottedOrObject);
    if dotted_target && plain_string(target, cx) == Some("time.sleep") {
        return true;
    }
    (mutator.is_some() || object_patch)
        && target.kind() == "identifier"
        && modules.contains(text(cx, target))
        && argument(
            &args,
            1,
            if object_patch { "attribute" } else { "name" },
            cx,
        )
        .is_some_and(|attribute| plain_string(attribute, cx) == Some("sleep"))
}

fn argument<'t>(
    args: &[Node<'t>],
    position: usize,
    keyword: &str,
    cx: &Cx<'_, '_>,
) -> Option<Node<'t>> {
    args.iter()
        .find(|arg| {
            arg.kind() == "keyword_argument"
                && child_by_field(**arg, "name").is_some_and(|name| text(cx, name) == keyword)
        })
        .and_then(|arg| child_by_field(*arg, "value"))
        .or_else(|| {
            args.iter()
                .filter(|arg| arg.kind() != "keyword_argument")
                .nth(position)
                .copied()
        })
}

fn patch_function(function: Node<'_>, cx: &Cx<'_, '_>, patches: &[(&str, PatchImport)]) -> bool {
    if function.kind() == "identifier" {
        return patches.contains(&(text(cx, function), PatchImport::Function));
    }
    if !child_by_field(function, "attribute").is_some_and(|member| text(cx, member) == "patch") {
        return false;
    }
    let Some(object) = child_by_field(function, "object") else {
        return false;
    };
    if object.kind() == "identifier" {
        return text(cx, object) == "mocker"
            || patches.contains(&(text(cx, object), PatchImport::MockModule));
    }
    child_by_field(object, "attribute").is_some_and(|member| text(cx, member) == "mock")
        && child_by_field(object, "object").is_some_and(|root| {
            root.kind() == "identifier"
                && patches.contains(&(text(cx, root), PatchImport::UnittestModule))
        })
}

fn mutation_helper(
    function: Node<'_>,
    cx: &Cx<'_, '_>,
    builtins: &HashSet<&str>,
) -> Option<MutationTarget> {
    if function.kind() == "identifier" {
        return matches!(text(cx, function), "setattr" | "delattr")
            .then_some(MutationTarget::Object);
    }
    let member = child_by_field(function, "attribute")?;
    if !matches!(text(cx, member), "setattr" | "delattr") {
        return None;
    }
    let object = child_by_field(function, "object")?;
    if object.kind() != "identifier" {
        return None;
    }
    if builtins.contains(text(cx, object)) {
        Some(MutationTarget::Object)
    } else {
        (text(cx, object) == "monkeypatch").then_some(MutationTarget::DottedOrObject)
    }
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
    mut emit: impl FnMut(&'s str, Option<ImportForm>, Option<PatchImport>),
) {
    let module = child_by_field(n, "module_name");
    for child in named_children(n) {
        if Some(child.id()) == module.map(|n| n.id()) {
            continue;
        }
        if child.kind() == "wildcard_import" {
            emit("*", None, None);
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
        let patch = match (module.map(|module| text(cx, module)), path, alias.is_some()) {
            (Some("unittest.mock"), "patch", _) => Some(PatchImport::Function),
            (Some("unittest"), "mock", _) | (None, "unittest.mock", true) => {
                Some(PatchImport::MockModule)
            }
            (None, "unittest" | "unittest.mock", _) => Some(PatchImport::UnittestModule),
            (None, "builtins", _) => Some(PatchImport::BuiltinsModule),
            _ => None,
        };
        emit(name, kind, patch);
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
            // Object and index expressions in assignment/delete targets only
            // read their names; destructuring targets can introduce bindings.
            if !matches!(node.kind(), "attribute" | "subscript") {
                stack.extend(named_children(node));
            }
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
