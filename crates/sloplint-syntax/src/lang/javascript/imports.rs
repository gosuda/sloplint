//! Import and export facts for JavaScript.

use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, code_children, find_named_child, has_child_kind, pos_of, strip_quotes, text,
};

pub(crate) fn is_test_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    if file.contains(".test.") || file.contains(".spec.") {
        return true;
    }
    path.split('/')
        .any(|segment| segment == "test" || segment == "tests" || segment == "__tests__")
}

pub(crate) fn external_root_of(module: &str) -> Option<&str> {
    if module.starts_with('.') || module.starts_with('/') || module.starts_with("node:") {
        return None;
    }
    if let Some(scoped) = module.strip_prefix('@') {
        return scoped_root(module, scoped);
    }
    let end = module.find('/').unwrap_or(module.len());
    (end > 0).then(|| &module[..end])
}

fn scoped_root<'s>(module: &'s str, scoped: &str) -> Option<&'s str> {
    let slash = scoped.find('/')?;
    let tail = &scoped[slash + 1..];
    let end = tail.find('/').unwrap_or(tail.len());
    (end > 0).then(|| &module[..slash + end + 2])
}

pub(crate) fn import_scope(cx: &Cx<'_, '_>) -> ImportScope {
    for node in cx.ancestors() {
        if node.kind() == "program" {
            return ImportScope::Module;
        }
        if is_local_scope(node.kind()) {
            return ImportScope::FunctionLocal;
        }
    }
    ImportScope::Module
}

fn is_local_scope(kind: &str) -> bool {
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

struct ImportSite<'s> {
    pos: Pos,
    module: &'s str,
    scope: ImportScope,
    in_test: bool,
    is_reexport: bool,
    external: Option<&'s str>,
}

impl<'s> ImportSite<'s> {
    fn imported(cx: &Cx<'_, 's>, n: Node<'_>, module: &'s str) -> Self {
        Self::build(cx, n, module, false)
    }

    fn reexported(cx: &Cx<'_, 's>, n: Node<'_>, module: &'s str) -> Self {
        Self::build(cx, n, module, true)
    }

    fn build(cx: &Cx<'_, 's>, n: Node<'_>, module: &'s str, is_reexport: bool) -> Self {
        ImportSite {
            pos: pos_of(n),
            module,
            scope: import_scope(cx),
            in_test: is_test_path(cx.path),
            is_reexport,
            external: external_root_of(module),
        }
    }

    fn fact(&self, name: &'s str, alias: Option<&'s str>, kind: ImportKind) -> Import<'s> {
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

pub(crate) fn import_module<'s>(cx: &Cx<'_, 's>, statement: Node<'_>) -> Option<&'s str> {
    let source = child_by_field(statement, "source")?;
    Some(strip_quotes(text(cx, source)))
}

fn named_specifier_fact<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    let Some(name) = child_by_field(spec, "name") else {
        return;
    };
    let name_text = text(cx, name);
    let alias = child_by_field(spec, "alias")
        .map(|alias| text(cx, alias))
        .filter(|found| found != &name_text);
    out.push(site.fact(name_text, alias, ImportKind::Named));
}

fn named_import_facts<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    for child in code_children(clause) {
        match child.kind() {
            "identifier" => out.push(site.fact(text(cx, child), None, ImportKind::Named)),
            "namespace_import" => push_namespace(cx, child, site, out),
            "named_imports" => {
                for spec in code_children(child) {
                    named_specifier_fact(cx, spec, site, out);
                }
            }
            _ => {}
        }
    }
}

fn push_namespace<'s>(
    cx: &Cx<'_, 's>,
    clause: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    if let Some(id) = find_named_child(clause, |node| node.kind() == "identifier") {
        out.push(site.fact(text(cx, id), None, ImportKind::Namespace));
    }
}

fn export_facts<'s>(
    cx: &Cx<'_, 's>,
    statement: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    if let Some(clause) = find_named_child(statement, |child| child.kind() == "export_clause") {
        for spec in code_children(clause) {
            named_specifier_fact(cx, spec, site, out);
        }
        return;
    }
    if export_namespace(cx, statement, site, out) {
        return;
    }
    if has_child_kind(statement, "*") {
        out.push(site.fact("*", None, ImportKind::Wildcard));
    }
}

fn export_namespace<'s>(
    cx: &Cx<'_, 's>,
    statement: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) -> bool {
    let Some(namespace) = find_named_child(statement, |child| child.kind() == "namespace_export")
    else {
        return false;
    };
    let mut cursor = namespace.walk();
    let name = namespace
        .children(&mut cursor)
        .find(|node| node.is_named() && node.kind() == "identifier")
        .map_or("*", |id| text(cx, id));
    out.push(site.fact(name, None, ImportKind::Namespace));
    true
}

pub(crate) fn import_facts<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    match n.kind() {
        "import_statement" => push_imports(cx, n, out),
        "export_statement" => push_reexports(cx, n, out),
        "call_expression" => push_require(cx, n, out),
        _ => {}
    }
}

fn push_require<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    if !child_by_field(n, "function")
        .is_some_and(|function| function.kind() == "identifier" && text(cx, function) == "require")
    {
        return;
    }
    let Some(args) = child_by_field(n, "arguments") else {
        return;
    };
    let Some(first) = code_children(args).into_iter().next() else {
        return;
    };
    if first.kind() != "string" {
        return;
    }
    let module = strip_quotes(text(cx, first));
    if module.is_empty() {
        return;
    }
    let site = ImportSite::imported(cx, n, module);
    let bound = cx
        .parent()
        .filter(|parent| parent.kind() == "variable_declarator")
        .and_then(|parent| child_by_field(parent, "name"));
    match bound {
        Some(name) if name.kind() == "identifier" => {
            out.push(site.fact(text(cx, name), None, ImportKind::Namespace));
        }
        Some(pattern) if pattern.kind() == "object_pattern" => {
            push_require_pattern_names(cx, pattern, &site, out);
        }
        _ => out.push(site.fact("", None, ImportKind::SideEffect)),
    }
}

fn destructured_bound(value: Node<'_>) -> Option<Node<'_>> {
    match value.kind() {
        "identifier" => Some(value),
        "assignment_pattern" => {
            child_by_field(value, "left").filter(|left| left.kind() == "identifier")
        }
        _ => None,
    }
}

fn push_require_pattern_names<'s>(
    cx: &Cx<'_, 's>,
    pattern: Node<'_>,
    site: &ImportSite<'s>,
    out: &mut Vec<Import<'s>>,
) {
    for child in code_children(pattern) {
        match child.kind() {
            "shorthand_property_identifier_pattern" => {
                out.push(site.fact(text(cx, child), None, ImportKind::Named));
            }
            "pair_pattern" => {
                let key =
                    child_by_field(child, "key").filter(|key| key.kind() == "property_identifier");
                let bound = child_by_field(child, "value").and_then(destructured_bound);
                if let (Some(key), Some(bound)) = (key, bound) {
                    let name = text(cx, key);
                    let alias = text(cx, bound);
                    out.push(site.fact(name, (alias != name).then_some(alias), ImportKind::Named));
                }
            }
            "object_assignment_pattern" => {
                let left = child_by_field(child, "left")
                    .filter(|left| left.kind() == "shorthand_property_identifier_pattern");
                if let Some(left) = left {
                    out.push(site.fact(text(cx, left), None, ImportKind::Named));
                }
            }
            _ => {}
        }
    }
}

fn push_imports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(module) = import_module(cx, n) else {
        return;
    };
    let site = ImportSite::imported(cx, n, module);
    let clauses: Vec<Node<'_>> = code_children(n)
        .into_iter()
        .filter(|child| child.kind() == "import_clause")
        .collect();

    for clause in &clauses {
        named_import_facts(cx, *clause, &site, out);
    }

    if clauses.is_empty() {
        out.push(site.fact("", None, ImportKind::SideEffect));
    }
}

pub(crate) fn exported_names_of<'s>(root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    for statement in code_children(root) {
        if statement.kind() == "export_statement" && import_module(cx, statement).is_none() {
            local_export_names(cx, statement, out);
        } else if statement.kind() == "expression_statement" {
            commonjs_export_names(cx, statement, out);
        }
    }
}

fn is_module_exports(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "member_expression"
        && child_by_field(n, "object")
            .is_some_and(|object| object.kind() == "identifier" && text(cx, object) == "module")
        && child_by_field(n, "property").is_some_and(|property| text(cx, property) == "exports")
}

fn push_module_keys<'s>(cx: &Cx<'_, 's>, right: Node<'_>, out: &mut Vec<&'s str>) {
    match right.kind() {
        "object" => {
            for child in code_children(right) {
                match child.kind() {
                    "shorthand_property_identifier" => out.push(text(cx, child)),
                    "pair" => {
                        let key = child_by_field(child, "key")
                            .filter(|key| key.kind() == "property_identifier");
                        if let Some(key) = key {
                            out.push(text(cx, key));
                        }
                    }
                    "method_definition" => {
                        let name = child_by_field(child, "name")
                            .filter(|name| name.kind() == "property_identifier");
                        if let Some(name) = name {
                            out.push(text(cx, name));
                        }
                    }
                    _ => {}
                }
            }
        }
        "identifier" => out.push(text(cx, right)),
        _ => {}
    }
}

fn commonjs_export_names<'s>(cx: &Cx<'_, 's>, statement: Node<'_>, out: &mut Vec<&'s str>) {
    let Some(assignment) = code_children(statement).into_iter().next() else {
        return;
    };
    if assignment.kind() != "assignment_expression" {
        return;
    }
    let (Some(left), Some(right)) = (
        child_by_field(assignment, "left"),
        child_by_field(assignment, "right"),
    ) else {
        return;
    };
    if is_module_exports(cx, left) {
        push_module_keys(cx, right, out);
        return;
    }
    if left.kind() == "member_expression" {
        let target = child_by_field(left, "object").is_some_and(|object| {
            object.kind() == "identifier" && text(cx, object) == "exports"
                || is_module_exports(cx, object)
        });
        let property = child_by_field(left, "property")
            .filter(|property| property.kind() == "property_identifier");
        if let Some(property) = property.filter(|_| target) {
            out.push(text(cx, property));
        }
    }
}

fn push_reexports<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(module) = import_module(cx, n) else {
        return;
    };
    let site = ImportSite::reexported(cx, n, module);
    export_facts(cx, n, &site, out);
}

pub(crate) fn local_export_names<'s>(cx: &Cx<'_, 's>, statement: Node<'_>, out: &mut Vec<&'s str>) {
    for part in code_children(statement) {
        match part.kind() {
            "identifier" => out.push(text(cx, part)),
            "export_clause" => {
                for spec in code_children(part) {
                    out.extend(child_by_field(spec, "name").map(|name| text(cx, name)));
                }
            }
            _ => {}
        }
    }
}
