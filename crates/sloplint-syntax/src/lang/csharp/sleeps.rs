//! Keep thread-sleep recognition independent of similarly named application APIs.

use std::collections::HashSet;

use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, has_child_kind, named_children, pos_of,
    rfind_named_child, text,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportForm {
    Type,
    Function,
}

pub(crate) fn blocking_sleep(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if n.kind() != "invocation_expression" {
        return None;
    }
    let function = child_by_field(n, "function")?;
    let (form, name) = if function.kind() == "identifier" && ident(cx, function) == "Sleep" {
        (ImportForm::Function, "Sleep")
    } else if function.kind() == "member_access_expression"
        && child_by_field(function, "name")
            .is_some_and(|name| name.kind() == "identifier" && ident(cx, name) == "Sleep")
    {
        let receiver = child_by_field(function, "expression")?;
        let name = if path_is(receiver, cx, &["System", "Threading", "Thread"], false) {
            "System.Threading.Thread"
        } else if path_is(receiver, cx, &["System", "Threading", "Thread"], true) {
            "global::System.Threading.Thread"
        } else if receiver.kind() == "identifier" {
            ident(cx, receiver)
        } else {
            return None;
        };
        (ImportForm::Type, name)
    } else {
        return None;
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
    let args = code_children(child_by_field(n, "arguments")?);
    if args.len() != 1
        || integer_argument(args[0], cx).is_some_and(|value| value == 0 || value < -1)
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
    let mut aliases = Vec::new();
    let mut imported_thread = false;
    let mut static_thread = false;
    let mut other_static = false;
    let mut inherited = false;
    let mut thread_type = false;
    let mut stack = vec![(root, false)];
    while let Some((node, file_scope)) = stack.pop() {
        if matches!(
            node.kind(),
            "identifier" | "implicit_parameter" | "using_directive"
        ) && text(cx, node).contains('\\')
        {
            return Vec::new();
        }
        if node.kind() == "using_directive" {
            let alias = child_by_field(node, "name");
            let Some(target) = rfind_named_child(node, |child| {
                Some(child.id()) != alias.map(|name| name.id()) && child.kind() != "comment"
            }) else {
                continue;
            };
            let global = path_is(target, cx, &["System", "Threading", "Thread"], true);
            let is_thread =
                global || path_is(target, cx, &["System", "Threading", "Thread"], false);
            if let Some(alias) = alias {
                if file_scope && is_thread {
                    aliases.push((ident(cx, alias), global));
                } else {
                    rebound.insert(ident(cx, alias));
                }
            } else if has_child_kind(node, "static") {
                static_thread |= file_scope && is_thread;
                other_static |= !is_thread;
            } else if file_scope
                && (path_is(target, cx, &["System", "Threading"], false)
                    || path_is(target, cx, &["System", "Threading"], true))
            {
                imported_thread = true;
            }
            continue;
        }
        inherited |= node.kind() == "base_list";
        thread_type |= matches!(
            node.kind(),
            "class_declaration"
                | "struct_declaration"
                | "record_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "delegate_declaration"
        ) && child_by_field(node, "name")
            .is_some_and(|name| ident(cx, name) == "Thread");
        bound_names(node, cx, &mut rebound);
        stack.extend(
            named_children(node)
                .into_iter()
                .map(|child| (child, node.id() == root.id())),
        );
    }
    if thread_type {
        return Vec::new();
    }
    let mut names = Vec::new();
    if wanted == ImportForm::Type {
        names.push("global::System.Threading.Thread");
        if !rebound.contains("System") {
            names.push("System.Threading.Thread");
        }
        if !inherited && !other_static {
            if imported_thread && !rebound.contains("Thread") && !rebound.contains("System") {
                names.push("Thread");
            }
            names.extend(aliases.into_iter().filter_map(|(alias, global)| {
                (!rebound.contains(alias) && (global || !rebound.contains("System")))
                    .then_some(alias)
            }));
        }
    } else if static_thread
        && !other_static
        && !inherited
        && !rebound.contains("Sleep")
        && !rebound.contains("System")
    {
        names.push("Sleep");
    }
    names.sort_unstable();
    names.dedup();
    names
}

fn bound_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut HashSet<&'s str>) {
    if (n.kind().ends_with("_declaration")
        || n.kind().ends_with("_declarator")
        || matches!(
            n.kind(),
            "parameter"
                | "type_parameter"
                | "local_function_statement"
                | "from_clause"
                | "declaration_pattern"
                | "var_pattern"
                | "recursive_pattern"
        ))
        && let Some(name) = child_by_field(n, "name")
    {
        out.insert(ident(cx, name));
    }
    match n.kind() {
        "implicit_parameter" => {
            out.insert(ident(cx, n));
        }
        "tuple_pattern"
        | "parenthesized_variable_designation"
        | "join_clause"
        | "join_into_clause"
        | "let_clause"
        | "query_expression" => {
            out.extend(
                code_children(n)
                    .into_iter()
                    .filter(|child| child.kind() == "identifier")
                    .map(|child| ident(cx, child)),
            );
        }
        "foreach_statement" => {
            if let Some(left) = child_by_field(n, "left")
                && left.kind() == "identifier"
            {
                out.insert(ident(cx, left));
            }
        }
        _ => {}
    }
}

fn path_is(mut n: Node<'_>, cx: &Cx<'_, '_>, mut parts: &[&str], global: bool) -> bool {
    while parts.len() > 1 {
        let prefix = match n.kind() {
            "member_access_expression" => "expression",
            "qualified_name" => "qualifier",
            _ => return false,
        };
        if !child_by_field(n, "name").is_some_and(|name| {
            name.kind() == "identifier" && ident(cx, name) == parts[parts.len() - 1]
        }) {
            return false;
        }
        let Some(next) = child_by_field(n, prefix) else {
            return false;
        };
        n = next;
        parts = &parts[..parts.len() - 1];
    }
    let Some(first) = parts.first() else {
        return false;
    };
    if global {
        n.kind() == "alias_qualified_name"
            && child_by_field(n, "alias").is_some_and(|alias| text(cx, alias) == "global")
            && child_by_field(n, "name").is_some_and(|name| ident(cx, name) == *first)
    } else {
        n.kind() == "identifier" && ident(cx, n) == *first
    }
}

fn ident<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let raw = text(cx, n);
    raw.strip_prefix('@').unwrap_or(raw)
}

fn integer_argument(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<i64> {
    let label = child_by_field(n, "name").map(|name| name.id());
    let mut expr = code_children(n)
        .into_iter()
        .find(|child| Some(child.id()) != label)?;
    let mut negative = false;
    loop {
        expr = match expr.kind() {
            "parenthesized_expression" => code_children(expr).first().copied()?,
            "prefix_unary_expression" if has_child_kind(expr, "+") || has_child_kind(expr, "-") => {
                negative ^= has_child_kind(expr, "-");
                code_children(expr).first().copied()?
            }
            "integer_literal" => {
                let raw = text(cx, expr).trim_end_matches(['u', 'U', 'l', 'L']);
                let (radix, digits) =
                    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
                        (16, hex)
                    } else if let Some(binary) =
                        raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B"))
                    {
                        (2, binary)
                    } else {
                        (10, raw)
                    };
                let value = digits.bytes().filter(|&byte| byte != b'_').try_fold(
                    0_i64,
                    |value, byte| {
                        let digit = char::from(byte).to_digit(radix)?;
                        value
                            .checked_mul(i64::from(radix))?
                            .checked_add(i64::from(digit))
                    },
                )?;
                return if negative {
                    value.checked_neg()
                } else {
                    Some(value)
                };
            }
            _ => return None,
        };
    }
}
