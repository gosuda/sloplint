//! Resolve standard thread sleeps from imports and visible bindings.

use std::collections::HashSet;

use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, first_of_kind, has_child_kind, named_children, pos_of, text,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportForm {
    Type,
    Function,
}

pub(crate) fn blocking_sleep(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if n.kind() != "method_invocation"
        || text(cx, child_by_field(n, "name")?) != "sleep"
        || child_by_field(n, "type_arguments").is_some()
    {
        return None;
    }
    let (form, name) = match child_by_field(n, "object") {
        Some(object) if path_is(object, cx, &["java", "lang", "Thread"]) => {
            (ImportForm::Type, "java.lang.Thread")
        }
        Some(object) if object.kind() == "identifier" && text(cx, object) == "Thread" => {
            (ImportForm::Type, "Thread")
        }
        None => (ImportForm::Function, "sleep"),
        _ => return None,
    };
    let slot = match form {
        ImportForm::Type => FileSetKind::SleepModules,
        ImportForm::Function => FileSetKind::SleepFunctions,
    };
    if cx
        .file_names(slot, || imported_names(cx, form))
        .binary_search(&name)
        .is_err()
    {
        return None;
    }
    let args: Vec<_> = named_children(child_by_field(n, "arguments")?)
        .into_iter()
        .filter(|arg| !super::COMMENTS.contains(&arg.kind()))
        .collect();
    if !(1..=2).contains(&args.len()) {
        return None;
    }
    let millis = integer_value(args[0], cx);
    let nanos = args.get(1).and_then(|&arg| integer_value(arg, cx));
    if millis.is_some_and(|value| value < 0)
        || nanos.is_some_and(|value| !(0..=999_999).contains(&value))
        || (millis == Some(0) && (args.len() == 1 || nanos == Some(0)))
    {
        return None;
    }
    Some(pos_of(n))
}

fn imported_names<'s>(cx: &Cx<'_, 's>, wanted: ImportForm) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut rebound = HashSet::new();
    let mut explicit_thread = false;
    let mut static_sleep = false;
    let mut wildcard_type = false;
    let mut wildcard_static = false;
    let mut inherited = false;
    let mut thread_type = false;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "import_declaration"
        ) && text(cx, node).contains('\\')
        {
            return Vec::new();
        }
        if node.kind() == "import_declaration" {
            let Some(path) = first_of_kind(node, &["scoped_identifier", "identifier"]) else {
                continue;
            };
            let is_thread = path_is(path, cx, &["java", "lang", "Thread"]);
            if has_child_kind(node, "static") {
                if has_child_kind(node, "asterisk") {
                    static_sleep |= is_thread;
                    wildcard_static |= !is_thread;
                } else if path_is(path, cx, &["java", "lang", "Thread", "sleep"]) {
                    static_sleep = true;
                } else {
                    rebound.insert(text(cx, child_by_field(path, "name").unwrap_or(path)));
                }
            } else if has_child_kind(node, "asterisk") {
                wildcard_type |= !path_is(path, cx, &["java", "lang"]);
            } else if is_thread {
                explicit_thread = true;
            } else {
                rebound.insert(text(cx, child_by_field(path, "name").unwrap_or(path)));
            }
            continue;
        }
        inherited |= matches!(
            node.kind(),
            "superclass" | "super_interfaces" | "extends_interfaces"
        );
        thread_type |= is_type_declaration(node.kind())
            && child_by_field(node, "name").is_some_and(|name| text(cx, name) == "Thread");
        bound_names(node, cx, &mut rebound);
        stack.extend(named_children(node));
    }
    let mut names = Vec::new();
    if wanted == ImportForm::Type {
        if !rebound.contains("java") && !thread_type {
            names.push("java.lang.Thread");
        }
        if !rebound.contains("Thread")
            && !inherited
            && !wildcard_static
            && (!wildcard_type || explicit_thread)
        {
            names.push("Thread");
        }
    } else if static_sleep
        && !thread_type
        && !rebound.contains("sleep")
        && !wildcard_static
        && !inherited
    {
        names.push("sleep");
    }
    names.sort_unstable();
    names
}

fn is_type_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

fn bound_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut HashSet<&'s str>) {
    if (n.kind().ends_with("_declaration")
        || matches!(
            n.kind(),
            "variable_declarator"
                | "formal_parameter"
                | "catch_formal_parameter"
                | "enhanced_for_statement"
                | "resource"
                | "instanceof_expression"
                | "enum_constant"
        ))
        && let Some(name) = child_by_field(n, "name")
    {
        out.insert(text(cx, name));
    }
    match n.kind() {
        "type_parameter" | "type_pattern" | "record_pattern_component" | "inferred_parameters" => {
            out.extend(
                named_children(n)
                    .into_iter()
                    .filter(|n| matches!(n.kind(), "identifier" | "type_identifier"))
                    .map(|n| text(cx, n)),
            );
        }
        "lambda_expression" => {
            if let Some(parameter) = child_by_field(n, "parameters")
                && parameter.kind() == "identifier"
            {
                out.insert(text(cx, parameter));
            }
        }
        _ => {}
    }
}

fn path_is(mut n: Node<'_>, cx: &Cx<'_, '_>, mut parts: &[&str]) -> bool {
    while parts.len() > 1 {
        let (prefix, leaf) = match n.kind() {
            "field_access" => ("object", "field"),
            "scoped_identifier" => ("scope", "name"),
            _ => return false,
        };
        if !child_by_field(n, leaf).is_some_and(|name| text(cx, name) == parts[parts.len() - 1]) {
            return false;
        }
        let Some(next) = child_by_field(n, prefix) else {
            return false;
        };
        n = next;
        parts = &parts[..parts.len() - 1];
    }
    n.kind() == "identifier" && parts.first().is_some_and(|name| text(cx, n) == *name)
}

fn integer_value(mut n: Node<'_>, cx: &Cx<'_, '_>) -> Option<i64> {
    let mut negative = false;
    loop {
        n = match n.kind() {
            "parenthesized_expression" => named_children(n)
                .into_iter()
                .find(|child| !super::COMMENTS.contains(&child.kind()))?,
            "unary_expression"
                if child_by_field(n, "operator")
                    .is_some_and(|op| matches!(text(cx, op), "+" | "-")) =>
            {
                negative ^= has_child_kind(n, "-");
                child_by_field(n, "operand")?
            }
            "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
            | "binary_integer_literal" => {
                return literal_value(n, cx, negative);
            }
            _ => return None,
        };
    }
}

fn literal_value(n: Node<'_>, cx: &Cx<'_, '_>, negative: bool) -> Option<i64> {
    let raw = text(cx, n);
    let long = raw.ends_with(['l', 'L']);
    let raw = raw.trim_end_matches(['l', 'L']);
    let (radix, digits) = match n.kind() {
        "hex_integer_literal" => (16, raw.get(2..)?),
        "binary_integer_literal" => (2, raw.get(2..)?),
        "octal_integer_literal" => (8, raw),
        _ => (10, raw),
    };
    let value = digits
        .bytes()
        .filter(|&byte| byte != b'_')
        .try_fold(0_u64, |value, byte| {
            let digit = char::from(byte).to_digit(radix)?;
            value
                .checked_mul(u64::from(radix))?
                .checked_add(u64::from(digit))
        })?;
    // Java's nondecimal literals and unary negation retain their int/long
    // width, including the minimum value whose negation wraps to itself.
    if long {
        if radix == 10 && value > (1_u64 << 63) {
            return None;
        }
        let value = i64::from_ne_bytes(value.to_ne_bytes());
        Some(if negative {
            value.wrapping_neg()
        } else {
            value
        })
    } else {
        let value = u32::try_from(value).ok()?;
        if radix == 10 && value > (1_u32 << 31) {
            return None;
        }
        let value = i32::from_ne_bytes(value.to_ne_bytes());
        Some(i64::from(if negative {
            value.wrapping_neg()
        } else {
            value
        }))
    }
}
