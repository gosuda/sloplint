use sloplint_core::facts::{Import, ImportScope, Pos};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use super::imports::{UseFacts, push_use};
use super::params::push_ident_tree;
use crate::hooks::{Cx, FileSetKind, child_by_field, has_child_kind, pos_of, text};

// Qualified markers cannot collide with imported identifiers. Keep the global
// root distinct because nested module globs cannot redirect an absolute path.
const STD_DURATION: &str = "std::time::Duration";
const ABS_STD_DURATION: &str = "::std::time::Duration";

pub(crate) fn blocking_sleep(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Pos> {
    if cx.lang != Lang::Rust {
        return None;
    }
    let function = child_by_field(n, "function")?;
    let mut parts = [""; 4];
    let (length, absolute) = path_parts(function, cx, &mut parts)?;
    let known = match &parts[..length] {
        ["std", "thread", "sleep"] => standard_path(cx, absolute),
        [module, "sleep"] if !absolute && root_scope(cx) => {
            names(cx, FileSetKind::SleepModules).contains(module)
        }
        [function] if !absolute && root_scope(cx) => {
            names(cx, FileSetKind::SleepFunctions).contains(function)
        }
        _ => false,
    };
    if !known || zero_duration(single_argument(n)?, cx) {
        return None;
    }
    Some(pos_of(n))
}

fn standard_path(cx: &Cx<'_, '_>, absolute: bool) -> bool {
    let known = names(cx, FileSetKind::SleepDurationTypes);
    if absolute {
        return known.contains(&ABS_STD_DURATION);
    }
    known.contains(&STD_DURATION) || (known.contains(&ABS_STD_DURATION) && !enclosing_glob(cx))
}

fn enclosing_glob(cx: &Cx<'_, '_>) -> bool {
    // Module imports do not enter child modules. Block imports remain in scope
    // throughout their block, including before the use declaration itself.
    for scope in cx.ancestors().take_while(|node| node.kind() != "mod_item") {
        if matches!(scope.kind(), "source_file" | "declaration_list" | "block")
            && uncommented_children(scope)
                .any(|child| child.kind() == "use_declaration" && contains_glob(child))
        {
            return true;
        }
    }
    false
}

fn contains_glob(import: Node<'_>) -> bool {
    let mut cursor = import.walk();
    loop {
        if cursor.node().kind() == "use_wildcard" {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return false;
            }
        }
    }
}

fn root_scope(cx: &Cx<'_, '_>) -> bool {
    !cx.ancestors().any(|ancestor| ancestor.kind() == "mod_item")
}

fn identifier(value: &str) -> &str {
    value.strip_prefix("r#").unwrap_or(value)
}

fn uncommented_children(n: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    (0..n.named_child_count()).filter_map(move |index| {
        // Tree-sitter's underlying child count and child index are both u32.
        n.named_child(index as u32)
            .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
    })
}

fn unparenthesized(mut n: Node<'_>) -> Option<Node<'_>> {
    while n.kind() == "parenthesized_expression" {
        let mut children = uncommented_children(n);
        n = children.next()?;
        if children.next().is_some() {
            return None;
        }
    }
    Some(n)
}

// Only the four-segment paths used here fit. Traversal never allocates, and
// methods, generic paths, macros, and longer user paths stay unclassified.
fn path_parts<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut [&'s str; 4]) -> Option<(usize, bool)> {
    let mut current = unparenthesized(n)?;
    let mut length = 0;
    loop {
        let (name, parent) = match current.kind() {
            "identifier" | "type_identifier" => (current, None),
            "scoped_identifier" | "scoped_type_identifier" => (
                child_by_field(current, "name")?,
                child_by_field(current, "path"),
            ),
            _ => return None,
        };
        *out.get_mut(length)? = identifier(text(cx, name));
        length += 1;
        let Some(parent) = parent else {
            break;
        };
        current = parent;
    }
    out[..length].reverse();
    Some((length, has_child_kind(current, "::")))
}

fn single_argument(call: Node<'_>) -> Option<Node<'_>> {
    let arguments = child_by_field(call, "arguments")?;
    let mut children = uncommented_children(arguments);
    let argument = children.next()?;
    children.next().is_none().then_some(argument)
}

fn zero_duration(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let Some(value) = unparenthesized(n) else {
        return false;
    };
    let path = if value.kind() == "call_expression" {
        let Some(function) = child_by_field(value, "function") else {
            return false;
        };
        function
    } else {
        value
    };
    let mut parts = [""; 4];
    let Some((length, absolute)) = path_parts(path, cx, &mut parts) else {
        return false;
    };
    let member = parts[length - 1];
    if value.kind() == "call_expression" {
        let count = match member {
            "new" => 2,
            "from_secs" | "from_millis" | "from_micros" | "from_nanos" | "from_secs_f32"
            | "from_secs_f64" => 1,
            _ => return false,
        };
        let Some(arguments) = child_by_field(value, "arguments") else {
            return false;
        };
        let mut found = 0;
        for argument in uncommented_children(arguments) {
            if !numeric_zero(argument, cx) {
                return false;
            }
            found += 1;
        }
        if found != count {
            return false;
        }
    } else if member != "ZERO" {
        return false;
    }
    match &parts[..length - 1] {
        ["std", "time", "Duration"] => standard_path(cx, absolute),
        [alias] if !absolute && root_scope(cx) => {
            names(cx, FileSetKind::SleepDurationTypes).contains(alias)
        }
        _ => false,
    }
}

fn numeric_zero(n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
    let Some(mut n) = unparenthesized(n) else {
        return false;
    };
    while n.kind() == "unary_expression" {
        if !has_child_kind(n, "-") {
            return false;
        }
        let mut operands = uncommented_children(n);
        let Some(operand) = operands.next() else {
            return false;
        };
        if operands.next().is_some() {
            return false;
        }
        let Some(inner) = unparenthesized(operand) else {
            return false;
        };
        n = inner;
    }
    if !matches!(n.kind(), "integer_literal" | "float_literal") {
        return false;
    }
    let mut value = text(cx, n);
    for suffix in [
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            value = number;
            break;
        }
    }
    if let Some(number) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0o"))
        .or_else(|| value.strip_prefix("0b"))
    {
        return number.bytes().any(|byte| byte == b'0')
            && number.bytes().all(|byte| matches!(byte, b'0' | b'_'));
    }
    value = value
        .strip_suffix("f32")
        .or_else(|| value.strip_suffix("f64"))
        .unwrap_or(value);
    let mantissa = value.split(['e', 'E']).next().unwrap_or("");
    mantissa.bytes().any(|byte| byte == b'0')
        && mantissa
            .bytes()
            .all(|byte| matches!(byte, b'0' | b'_' | b'.'))
}

fn names<'a, 's>(cx: &'a Cx<'_, 's>, kind: FileSetKind) -> &'a [&'s str] {
    cx.file_names(kind, || imported_names(cx, kind))
}

fn standard_import(import: &Import<'_>, kind: FileSetKind) -> bool {
    let (module, name): (&[&str], &str) = match kind {
        FileSetKind::SleepModules => (&["std"], "thread"),
        FileSetKind::SleepFunctions => (&["std", "thread"], "sleep"),
        FileSetKind::SleepDurationTypes => (&["std", "time"], "Duration"),
        _ => return false,
    };
    identifier(import.name.trim()) == name
        && import
            .module
            .trim_start_matches(':')
            .split("::")
            .map(str::trim)
            .map(identifier)
            .eq(module.iter().copied())
}

fn uses<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
    out.clear();
    let Some(argument) = child_by_field(n, "argument") else {
        return;
    };
    let facts = UseFacts {
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
    };
    push_use(cx, argument, &mut Vec::new(), &facts, out);
}

fn imported_names<'s>(cx: &Cx<'_, 's>, kind: FileSetKind) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut imports = Vec::new();
    let mut found = Vec::new();
    for child in uncommented_children(root).filter(|child| child.kind() == "use_declaration") {
        uses(child, cx, &mut imports);
        found.extend(
            imports
                .iter()
                .filter(|import| standard_import(import, kind))
                .map(|import| identifier(import.alias.unwrap_or(import.name))),
        );
    }
    if kind == FileSetKind::SleepDurationTypes {
        found.extend([STD_DURATION, ABS_STD_DURATION]);
    }
    // Ordinary files without a relevant import need no whole-tree binding scan.
    if found.is_empty() {
        return found;
    }
    let mut stack = vec![root];
    let mut bound = Vec::new();
    while let Some(n) = stack.pop() {
        if n.kind() == "use_declaration" {
            uses(n, cx, &mut imports);
            for import in &imports {
                let name = identifier(import.alias.unwrap_or(import.name));
                if name == "std" {
                    return Vec::new();
                }
                if import.name == "*" {
                    // File-scope globs may alter the crate root in Rust 2015.
                    // Nested globs cannot alter either that root or the extern
                    // prelude used by absolute paths in later editions.
                    if n.parent() == Some(root) {
                        return Vec::new();
                    }
                    // Canonical relative paths can recover their identity at
                    // the call site when this glob belongs to another scope.
                    found.retain(|candidate| *candidate == ABS_STD_DURATION);
                    continue;
                }
                if n.parent() != Some(root) || !standard_import(import, kind) {
                    found.retain(|candidate| *candidate != name);
                }
            }
            continue;
        }
        binding_nodes(n, cx, &mut bound);
        for binding in bound.drain(..) {
            let name = identifier(text(cx, binding));
            if name == "std" {
                return Vec::new();
            }
            found.retain(|candidate| *candidate != name);
        }
        if found.is_empty() {
            return found;
        }
        stack.extend(uncommented_children(n));
    }
    found.sort_unstable();
    found.dedup();
    found
}

// A binding anywhere vetoes the same root alias throughout the file. This
// deliberately gives up separate-scope matches instead of guessing resolution.
fn binding_nodes<'t>(n: Node<'t>, cx: &Cx<'_, '_>, out: &mut Vec<Node<'t>>) {
    match n.kind() {
        "let_declaration" | "parameter" | "for_expression" | "let_condition" => {
            if let Some(pattern) = child_by_field(n, "pattern") {
                push_ident_tree(pattern, out);
            }
        }
        "closure_parameters" => {
            for pattern in uncommented_children(n) {
                if !matches!(pattern.kind(), "parameter" | "attribute_item") {
                    push_ident_tree(pattern, out);
                }
            }
        }
        "match_pattern" => {
            if let Some(pattern) = uncommented_children(n).next() {
                push_ident_tree(pattern, out);
            }
        }
        "extern_crate_declaration" => {
            let name = child_by_field(n, "name");
            let alias = child_by_field(n, "alias");
            if name.is_some_and(|name| identifier(text(cx, name)) == "std") && alias.is_none() {
                return;
            }
            out.extend(alias.or(name));
        }
        "function_item"
        | "function_signature_item"
        | "mod_item"
        | "struct_item"
        | "enum_item"
        | "union_item"
        | "trait_item"
        | "type_item"
        | "associated_type"
        | "const_item"
        | "static_item"
        | "type_parameter"
        | "const_parameter" => {
            out.extend(child_by_field(n, "name"));
        }
        _ => {}
    }
}
