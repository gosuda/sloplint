//! Recognize imported wall-clock sleeps without guessing from method names.

use std::collections::HashSet;

use sloplint_core::facts::Pos;
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, named_children, pos_of, strip_quotes, text,
};

pub(crate) fn blocking_sleep(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if cx.lang != Lang::Go || n.kind() != "call_expression" {
        return None;
    }
    let (module, member) = selector(child_by_field(n, "function")?, cx)?;
    if member != "Sleep" {
        return None;
    }
    let modules = cx.file_names(FileSetKind::SleepModules, || imported_modules(cx, "time"));
    if modules.binary_search(&module).is_err() || in_virtual_time(cx) {
        return None;
    }
    let args = code_children(child_by_field(n, "arguments")?);
    if args.len() != 1 || nonpositive_literal(args[0], cx) {
        return None;
    }
    Some(pos_of(n))
}

fn selector<'s>(mut n: Node<'_>, cx: &Cx<'_, 's>) -> Option<(&'s str, &'s str)> {
    while n.kind() == "parenthesized_expression" {
        n = n.named_child(0)?;
    }
    if n.kind() != "selector_expression" {
        return None;
    }
    let module = child_by_field(n, "operand")?;
    let member = child_by_field(n, "field")?;
    (module.kind() == "identifier").then(|| (text(cx, module), text(cx, member)))
}

fn in_virtual_time(cx: &Cx<'_, '_>) -> bool {
    let modules = cx.file_names(FileSetKind::VirtualTimeModules, || {
        imported_modules(cx, "testing/synctest")
    });
    if modules.is_empty() {
        return false;
    }
    for closure in cx.ancestors().filter(|n| n.kind() == "func_literal") {
        let mut parent = cx.parent_of(closure);
        while let Some(n) = parent.filter(|n| n.kind() == "parenthesized_expression") {
            parent = cx.parent_of(n);
        }
        let Some(arguments) = parent.filter(|n| n.kind() == "argument_list") else {
            continue;
        };
        let Some(call) = cx
            .parent_of(arguments)
            .filter(|n| n.kind() == "call_expression")
        else {
            continue;
        };
        let Some(function) = child_by_field(call, "function") else {
            continue;
        };
        if let Some((module, member)) = selector(function, cx)
            && matches!(member, "Test" | "Run")
            && modules.binary_search(&module).is_ok()
        {
            return true;
        }
    }
    false
}

fn imported_modules<'s>(cx: &Cx<'_, 's>, package: &str) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut modules = Vec::new();
    let mut virtual_modules = Vec::new();
    let mut virtual_wildcard = false;
    let mut rebound = HashSet::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "import_spec" {
            if child_by_field(n, "path")
                .is_some_and(|path| strip_quotes(text(cx, path)) == "testing/synctest")
                && child_by_field(n, "name").is_some_and(|name| text(cx, name) == ".")
            {
                virtual_wildcard = true;
            }
            if let Some((path, name)) = import_binding(n, cx) {
                if path == package {
                    modules.push(name);
                }
                if path == "testing/synctest" {
                    virtual_modules.push(name);
                }
            }
            continue;
        }
        bound_names(n, cx, &mut rebound);
        stack.extend(named_children(n));
    }
    // An ambiguous virtual-time binding must not turn a fake sleep into a
    // wall-clock finding elsewhere in the same file.
    if package == "time"
        && (virtual_wildcard || virtual_modules.iter().any(|name| rebound.contains(name)))
    {
        return Vec::new();
    }
    modules.retain(|name| !rebound.contains(name));
    modules.sort_unstable();
    modules.dedup();
    modules
}

fn import_binding<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<(&'s str, &'s str)> {
    let path = strip_quotes(text(cx, child_by_field(n, "path")?));
    let name = child_by_field(n, "name")
        .map(|name| text(cx, name))
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path));
    (!matches!(name, "." | "_")).then_some((path, name))
}

fn bound_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut HashSet<&'s str>) {
    match n.kind() {
        "function_declaration" | "type_spec" | "type_alias" => {
            if let Some(name) = child_by_field(n, "name") {
                out.insert(text(cx, name));
            }
        }
        "parameter_declaration"
        | "variadic_parameter_declaration"
        | "type_parameter_declaration"
        | "var_spec"
        | "const_spec" => {
            out.extend(
                named_children(n)
                    .into_iter()
                    .filter(|n| n.kind() == "identifier")
                    .map(|n| text(cx, n)),
            );
        }
        "short_var_declaration" | "assignment_statement" | "range_clause" | "receive_statement" => {
            if let Some(left) = child_by_field(n, "left") {
                target_names(left, cx, out);
            }
        }
        "type_switch_statement" => {
            if let Some(alias) = child_by_field(n, "alias") {
                target_names(alias, cx, out);
            }
        }
        _ => {}
    }
}

fn target_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut HashSet<&'s str>) {
    if n.kind() == "identifier" {
        out.insert(text(cx, n));
    } else if n.kind() == "expression_list" {
        out.extend(
            named_children(n)
                .into_iter()
                .filter(|n| n.kind() == "identifier")
                .map(|n| text(cx, n)),
        );
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
            "unary_expression" => {
                let Some(operator) = child_by_field(n, "operator") else {
                    return false;
                };
                match text(cx, operator) {
                    "-" => negative = !negative,
                    "+" => {}
                    _ => return false,
                }
                let Some(inner) = child_by_field(n, "operand") else {
                    return false;
                };
                n = inner;
            }
            "int_literal" | "float_literal" => return negative || zero_literal(text(cx, n)),
            _ => return false,
        }
    }
}

fn zero_literal(raw: &str) -> bool {
    let digits = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        hex.split(['p', 'P']).next().unwrap_or(hex)
    } else if let Some(base) = raw
        .strip_prefix("0b")
        .or_else(|| raw.strip_prefix("0B"))
        .or_else(|| raw.strip_prefix("0o"))
        .or_else(|| raw.strip_prefix("0O"))
    {
        base
    } else {
        raw.split(['e', 'E']).next().unwrap_or(raw)
    };
    digits.bytes().any(|byte| byte == b'0')
        && digits
            .bytes()
            .all(|byte| matches!(byte, b'0' | b'_' | b'.'))
}
